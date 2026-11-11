#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Host-independent voice sessions. No UI, I/O, provider, or notebook dependency.

use std::fmt;

pub const MAX_TRANSCRIPT_BYTES: usize = 64 * 1024;
pub const MAX_SECONDS: usize = 60;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum State {
    #[default]
    Idle,
    Preparing,
    Recording,
    Finalizing,
    Review,
    Applying,
    Complete,
    Cancelled,
    Failed,
}

impl State {
    pub fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Preparing => "preparing",
            Self::Recording => "recording",
            Self::Finalizing => "finalizing",
            Self::Review => "review",
            Self::Applying => "applying",
            Self::Complete => "complete",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }

    pub fn active(self) -> bool {
        matches!(self, Self::Preparing | Self::Recording | Self::Finalizing | Self::Applying)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Capabilities {
    pub batch_transcription: bool,
    pub partial_transcripts: bool,
    pub language_selection: bool,
    pub speech_output: bool,
    pub conversation: bool,
}

#[derive(Clone, PartialEq, Eq)]
pub struct Transcript {
    pub session: u64,
    pub revision: u64,
    pub text: String,
    pub is_final: bool,
}

impl Transcript {
    pub fn new(session: u64, revision: u64, text: impl Into<String>, is_final: bool) -> Self {
        Self { session, revision, text: text.into(), is_final }
    }
}

impl fmt::Debug for Transcript {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Transcript")
            .field("session", &self.session)
            .field("revision", &self.revision)
            .field("bytes", &self.text.len())
            .field("is_final", &self.is_final)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoiceError {
    InvalidState,
    StaleSession,
    StaleRevision,
    TooLong,
    Empty,
    InvalidTarget,
    Exhausted,
}

impl fmt::Display for VoiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidState => "that action is not available in this voice state",
            Self::StaleSession => "that voice session has ended",
            Self::StaleRevision => "the transcript changed; review it again",
            Self::TooLong => "the transcript exceeds 64 KiB",
            Self::Empty => "there are no words to insert",
            Self::InvalidTarget => "the host must supply a nonempty target ticket of at most 128 bytes",
            Self::Exhausted => "voice sequence exhausted; reopen the app",
        })
    }
}

impl std::error::Error for VoiceError {}

#[derive(Clone, Default)]
pub struct VoiceSession {
    id: u64,
    state: State,
    revision: u64,
    draft: String,
    target: String,
    error: Option<String>,
}

impl fmt::Debug for VoiceSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VoiceSession")
            .field("id", &self.id)
            .field("state", &self.state)
            .field("revision", &self.revision)
            .field("draft_bytes", &self.draft.len())
            .finish()
    }
}

impl VoiceSession {
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn state(&self) -> State {
        self.state
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn draft(&self) -> &str {
        &self.draft
    }
    pub fn target(&self) -> &str {
        &self.target
    }
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn start(&mut self, target: &str) -> Result<u64, VoiceError> {
        if self.state.active() || self.state == State::Review {
            return Err(VoiceError::InvalidState);
        }
        validate_target(target)?;
        self.id = self.id.checked_add(1).ok_or(VoiceError::Exhausted)?;
        self.state = State::Preparing;
        self.revision = 0;
        self.draft.clear();
        self.target = target.into();
        self.error = None;
        Ok(self.id)
    }

    fn check(&self, id: u64, states: &[State]) -> Result<(), VoiceError> {
        if id != self.id {
            return Err(VoiceError::StaleSession);
        }
        if !states.contains(&self.state) {
            return Err(VoiceError::InvalidState);
        }
        Ok(())
    }

    pub fn ready(&mut self, id: u64) -> Result<(), VoiceError> {
        self.check(id, &[State::Preparing])?;
        self.state = State::Recording;
        Ok(())
    }

    pub fn stop(&mut self, id: u64) -> Result<(), VoiceError> {
        self.check(id, &[State::Recording])?;
        self.state = State::Finalizing;
        Ok(())
    }

    pub fn accept(&mut self, event: Transcript) -> Result<(), VoiceError> {
        let states: &[State] = if event.is_final { &[State::Finalizing] } else { &[State::Recording, State::Finalizing] };
        self.check(event.session, states)?;
        if event.revision <= self.revision {
            return Err(VoiceError::StaleRevision);
        }
        validate_text(&event.text)?;
        self.revision = event.revision;
        self.draft = event.text;
        if event.is_final {
            self.state = State::Review;
        }
        Ok(())
    }

    pub fn set_draft(&mut self, id: u64, expected: u64, text: &str) -> Result<(), VoiceError> {
        self.check(id, &[State::Review])?;
        if expected != self.revision {
            return Err(VoiceError::StaleRevision);
        }
        validate_text(text)?;
        self.revision = self.revision.checked_add(1).ok_or(VoiceError::Exhausted)?;
        self.draft = text.into();
        self.error = None;
        Ok(())
    }

    pub fn retarget(&mut self, id: u64, target: &str) -> Result<(), VoiceError> {
        self.check(id, &[State::Review])?;
        validate_target(target)?;
        self.revision = self.revision.checked_add(1).ok_or(VoiceError::Exhausted)?;
        self.target = target.into();
        self.error = None;
        Ok(())
    }

    pub fn begin_apply(&mut self, id: u64, expected: u64) -> Result<String, VoiceError> {
        self.check(id, &[State::Review])?;
        if expected != self.revision {
            return Err(VoiceError::StaleRevision);
        }
        if self.draft.trim().is_empty() {
            return Err(VoiceError::Empty);
        }
        self.state = State::Applying;
        Ok(self.draft.clone())
    }

    pub fn applied(&mut self, id: u64, result: Result<(), String>) -> Result<(), VoiceError> {
        self.check(id, &[State::Applying])?;
        self.state = if result.is_ok() { State::Complete } else { State::Review };
        self.error = result.err();
        if self.state == State::Complete {
            self.draft.clear();
            self.target.clear();
        }
        Ok(())
    }

    pub fn cancel(&mut self, id: u64) -> Result<(), VoiceError> {
        self.check(id, &[State::Preparing, State::Recording, State::Finalizing, State::Review, State::Failed])?;
        self.state = State::Cancelled;
        self.draft.clear();
        self.target.clear();
        self.error = None;
        Ok(())
    }

    pub fn fail(&mut self, id: u64, message: &str) -> Result<(), VoiceError> {
        self.check(id, &[State::Preparing, State::Recording, State::Finalizing])?;
        self.state = if self.draft.trim().is_empty() { State::Failed } else { State::Review };
        self.error = Some(message.chars().take(1024).collect());
        Ok(())
    }
}

fn validate_text(text: &str) -> Result<(), VoiceError> {
    if text.len() > MAX_TRANSCRIPT_BYTES { Err(VoiceError::TooLong) } else { Ok(()) }
}

fn validate_target(target: &str) -> Result<(), VoiceError> {
    if target.is_empty() || target.len() > 128 { Err(VoiceError::InvalidTarget) } else { Ok(()) }
}
