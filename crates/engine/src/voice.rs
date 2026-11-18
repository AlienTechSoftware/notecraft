//! Voice commands are pure state transitions; only the local host executes effects.
use crate::{EngineError, Output, Params, Session};
use craft_voice_core::{State, Transcript};
use std::fmt;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InvocationOrigin {
    LocalUi,
    Automation,
    #[default]
    Headless,
}

#[derive(Clone)]
pub enum VoiceEffect {
    Start { session: u64, target: String },
    Stop { session: u64 },
    Cancel { session: u64 },
    Commit { session: u64, revision: u64, target: String, text: String },
    Status { session: u64, state: State, revision: u64 },
}

impl VoiceEffect {
    pub fn session(&self) -> u64 {
        match self {
            Self::Start { session, .. }
            | Self::Stop { session }
            | Self::Cancel { session }
            | Self::Commit { session, .. }
            | Self::Status { session, .. } => *session,
        }
    }
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Start { .. } => "start",
            Self::Stop { .. } => "stop",
            Self::Cancel { .. } => "cancel",
            Self::Commit { .. } => "commit",
            Self::Status { .. } => "status",
        }
    }
}

impl fmt::Debug for VoiceEffect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VoiceEffect").field("kind", &self.kind()).field("session", &self.session()).finish()
    }
}

pub enum VoiceUpdate {
    Ready(u64),
    Stopped(u64),
    Transcript(Transcript),
    Failed(u64, String),
    Applied(u64, Result<(), String>),
}

fn field<'a>(p: &'a Params, key: &'static str) -> Result<&'a str, EngineError> {
    p.get(key).map(String::as_str).ok_or(EngineError::MissingParam(key))
}
fn number(p: &Params, key: &'static str) -> Result<u64, EngineError> {
    field(p, key)?.parse().map_err(|_| EngineError::BadParam(format!("{key} must be a whole number")))
}
fn error(e: craft_voice_core::VoiceError) -> EngineError {
    EngineError::BadParam(e.to_string())
}

impl Session {
    pub fn voice(&self) -> &craft_voice_core::VoiceSession {
        &self.voice
    }

    /// Native controller delivery. It is intentionally absent from the command registry.
    pub fn voice_update(&mut self, update: VoiceUpdate) -> Result<(), EngineError> {
        match update {
            VoiceUpdate::Ready(id) => self.voice.ready(id),
            VoiceUpdate::Stopped(id) if self.voice.id() == id && self.voice.state() == State::Finalizing => Ok(()),
            VoiceUpdate::Stopped(id) => self.voice.stop(id),
            VoiceUpdate::Transcript(event) => self.voice.accept(event),
            VoiceUpdate::Failed(id, message) => self.voice.fail(id, &message),
            VoiceUpdate::Applied(id, result) => self.voice.applied(id, result),
        }
        .map_err(error)
    }

    fn require_local_voice(&self) -> Result<(), EngineError> {
        if self.invocation_origin == InvocationOrigin::LocalUi {
            Ok(())
        } else {
            Err(EngineError::BadParam("voice recording and insertion require the local NoteCraft controls".into()))
        }
    }
}

pub(crate) fn start(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    s.require_local_voice()?;
    let target = field(p, "target")?.to_string();
    let session = s.voice.start(&target).map_err(error)?;
    Ok(Output::Voice(VoiceEffect::Start { session, target }))
}
pub(crate) fn stop(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    s.require_local_voice()?;
    let session = number(p, "session")?;
    s.voice.stop(session).map_err(error)?;
    Ok(Output::Voice(VoiceEffect::Stop { session }))
}
pub(crate) fn cancel(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    s.require_local_voice()?;
    let session = number(p, "session")?;
    s.voice.cancel(session).map_err(error)?;
    Ok(Output::Voice(VoiceEffect::Cancel { session }))
}
pub(crate) fn status(s: &mut Session, _: &Params) -> Result<Output, EngineError> {
    Ok(Output::Voice(VoiceEffect::Status { session: s.voice.id(), state: s.voice.state(), revision: s.voice.revision() }))
}
pub(crate) fn draft(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    s.require_local_voice()?;
    s.voice.set_draft(number(p, "session")?, number(p, "revision")?, field(p, "text")?).map_err(error)?;
    status(s, p)
}
pub(crate) fn retarget(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    s.require_local_voice()?;
    s.voice.retarget(number(p, "session")?, field(p, "target")?).map_err(error)?;
    status(s, p)
}
pub(crate) fn commit(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    s.require_local_voice()?;
    let session = number(p, "session")?;
    let revision = number(p, "revision")?;
    let text = s.voice.begin_apply(session, revision).map_err(error)?;
    Ok(Output::Voice(VoiceEffect::Commit { session, revision, target: s.voice.target().into(), text }))
}
