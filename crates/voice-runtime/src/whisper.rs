//! A user-selected whisper-cli process. No shell, downloads, or provider accounts.

use crate::{Cancellation, Transcriber, audio::PcmAudio, temporary::JobFiles};
use craft_voice_core::{Capabilities, MAX_TRANSCRIPT_BYTES};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

const MAX_OUTPUT: usize = 1024 * 1024;
/// whisper.cpp skips input shorter than one second; a short press is padded with silence.
const MIN_SAMPLES: usize = crate::audio::SAMPLE_RATE * 11 / 10;

/// An executable that passed the `--help` check, by path, size and modification time, and
/// whether it can suppress non-speech tokens. Checked once per executable, not per recording,
/// so pressing the talk key does not wait for a process start before the microphone opens.
type Checked = HashMap<PathBuf, (u64, Option<SystemTime>, Option<&'static str>)>;
static CHECKED: OnceLock<Mutex<Checked>> = OnceLock::new();

fn identity(path: &Path) -> Option<(u64, Option<SystemTime>)> {
    let meta = path.metadata().ok()?;
    Some((meta.len(), meta.modified().ok()))
}

#[cfg(windows)]
#[path = "whisper.win.rs"]
mod platform;
#[cfg(windows)]
use platform::suppress_console;

#[cfg(not(windows))]
fn suppress_console(_command: &mut Command) {}

/// The option names a `--help` text advertises, as whole words (`-f` is not found inside `-of`).
fn advertised(help: &str) -> std::collections::HashSet<&str> {
    help.split(|c: char| c.is_whitespace() || c == ',' || c == '[' || c == ']').filter(|w| w.starts_with('-')).collect()
}

#[derive(Clone)]
pub struct WhisperConfig {
    pub executable: PathBuf,
    pub model: PathBuf,
    pub language: String,
    pub temporary_root: PathBuf,
    pub timeout: Duration,
    suppress_non_speech: Option<&'static str>,
}

impl WhisperConfig {
    pub fn new(executable: PathBuf, model: PathBuf, temporary_root: PathBuf) -> Self {
        Self { executable, model, temporary_root, language: "auto".into(), timeout: Duration::from_secs(120), suppress_non_speech: None }
    }

    pub fn validate(&mut self, cancel: &Cancellation) -> Result<(), String> {
        if !self.executable.is_absolute() || !self.executable.is_file() {
            return Err("choose an existing absolute path to whisper-cli".into());
        }
        if !self.model.is_absolute() || !self.model.metadata().is_ok_and(|m| m.is_file() && m.len() > 0) {
            return Err("choose an existing local Whisper model file".into());
        }
        if self.language.len() > 16 || self.language.is_empty() || !self.language.chars().all(|c| c.is_ascii_alphabetic() || c == '-') {
            return Err("choose auto or a language code such as en or es".into());
        }
        let id = identity(&self.executable);
        let cache = CHECKED.get_or_init(Default::default);
        if let (Some((len, modified)), Ok(known)) = (id, cache.lock())
            && let Some(&(l, m, sns)) = known.get(&self.executable)
            && l == len
            && m == modified
        {
            self.suppress_non_speech = sns;
            return Ok(());
        }
        let files = JobFiles::new(&self.temporary_root)?;
        let help_path = files.dir.path().join("help");
        let out = File::create(&help_path).map_err(|e| e.to_string())?;
        let err = out.try_clone().map_err(|e| e.to_string())?;
        let mut command = Command::new(&self.executable);
        command.arg("--help").stdin(Stdio::null()).stdout(out).stderr(err);
        suppress_console(&mut command);
        let mut child = command.spawn().map_err(|e| format!("cannot start whisper-cli: {e}"))?;
        // Some versions use a nonzero exit status for --help. Flags still must be present.
        let result = wait_child(&mut child, cancel, Duration::from_secs(10));
        if cancel.is_cancelled() {
            return Err("voice operation cancelled".into());
        }
        if result.as_ref().is_err_and(|e| e == "recognizer timed out") {
            return result;
        }
        let bytes = read_bounded(&help_path)?;
        let help = String::from_utf8_lossy(&bytes);
        let flags = advertised(&help);
        if !["-oj", "-of", "-m", "-f", "-l"].iter().all(|flag| flags.contains(flag)) {
            return Err("this whisper-cli does not advertise the required JSON/model/file/language options".into());
        }
        self.suppress_non_speech = non_speech_option(&flags);
        if let (Some((len, modified)), Ok(mut known)) = (id, cache.lock()) {
            if known.len() >= 32 {
                known.clear();
            }
            known.insert(self.executable.clone(), (len, modified, self.suppress_non_speech));
        }
        Ok(())
    }

    pub fn recognize(&self, audio: &PcmAudio, cancel: &Cancellation) -> Result<String, String> {
        if audio.samples().is_empty() {
            return Ok(String::new());
        }
        if cancel.is_cancelled() {
            return Err("voice operation cancelled".into());
        }
        let files = JobFiles::new(&self.temporary_root)?;
        let wav = files.dir.path().join("input.wav");
        let output = files.dir.path().join("result");
        let bytes = if audio.samples().len() < MIN_SAMPLES {
            let mut samples = audio.samples().to_vec();
            samples.resize(MIN_SAMPLES, 0);
            PcmAudio::new(samples)?.wav()
        } else {
            audio.wav()
        };
        fs::write(&wav, bytes).map_err(|e| format!("cannot stage voice audio: {e}"))?;
        let mut command = Command::new(&self.executable);
        command.arg("-m").arg(&self.model).arg("-f").arg(&wav).arg("-l").arg(&self.language).arg("-oj").arg("-of").arg(&output);
        if let Some(flag) = self.suppress_non_speech {
            command.arg(flag);
        }
        command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        suppress_console(&mut command);
        let mut child = command.spawn().map_err(|e| format!("cannot start whisper-cli: {e}"))?;
        wait_child(&mut child, cancel, self.timeout)?;
        parse_transcript(&read_bounded(&output.with_extension("json"))?)
    }
}

impl Transcriber for WhisperConfig {
    fn capabilities(&self) -> Capabilities {
        Capabilities { batch_transcription: true, partial_transcripts: true, language_selection: true, ..Capabilities::default() }
    }
    fn prepare(&mut self, cancel: &Cancellation) -> Result<(), String> {
        self.validate(cancel)
    }
    fn transcribe(&mut self, audio: &PcmAudio, cancel: &Cancellation) -> Result<String, String> {
        self.recognize(audio, cancel)
    }
}

/// Always reaps on cancellation/timeout. Diagnostics do not include speech or subprocess output.
pub fn wait_child(child: &mut Child, cancel: &Cancellation, timeout: Duration) -> Result<(), String> {
    let started = Instant::now();
    loop {
        if cancel.is_cancelled() || started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(if cancel.is_cancelled() { "voice operation cancelled" } else { "recognizer timed out" }.into());
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                return if status.success() {
                    Ok(())
                } else {
                    Err("recognizer exited unsuccessfully; check the selected model and executable".into())
                };
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("cannot observe recognizer: {e}"));
            }
        }
    }
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, String> {
    let file = File::open(path).map_err(|e| format!("cannot read recognizer result: {e}"))?;
    let mut bytes = Vec::new();
    file.take((MAX_OUTPUT + 1) as u64).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_OUTPUT {
        return Err("recognizer result exceeds 1 MiB".into());
    }
    Ok(bytes)
}

pub fn parse_transcript(bytes: &[u8]) -> Result<String, String> {
    if bytes.len() > MAX_OUTPUT {
        return Err("recognizer result exceeds 1 MiB".into());
    }
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| "recognizer returned invalid JSON")?;
    let segments = value.get("transcription").and_then(|v| v.as_array()).ok_or("recognizer JSON has no transcription array")?;
    if segments.len() > 4096 {
        return Err("recognizer returned too many segments".into());
    }
    let mut text = String::new();
    for segment in segments {
        let words = segment.get("text").and_then(|t| t.as_str()).ok_or("recognizer segment has no text")?;
        if non_speech(words) {
            continue;
        }
        if text.len().saturating_add(words.len()) > MAX_TRANSCRIPT_BYTES {
            return Err("transcript exceeds 64 KiB".into());
        }
        text.push_str(words);
    }
    Ok(text.trim().into())
}

/// whisper.cpp labels silence and noise as whole segments such as `[BLANK_AUDIO]` or `(music)`.
/// Only those known labels are dropped: a spoken aside such as `(double-check the date)` is the
/// person's words and goes to review like the rest.
fn non_speech(segment: &str) -> bool {
    let t = segment.trim();
    if t.is_empty() {
        return true;
    }
    let wrapped = [('[', ']'), ('(', ')'), ('*', '*')].iter().any(|&(open, close)| t.len() >= 2 && t.starts_with(open) && t.ends_with(close));
    let inner = t.get(1..t.len().saturating_sub(1)).unwrap_or_default().trim().to_ascii_lowercase().replace(['_', '-'], " ");
    wrapped
        && matches!(
            inner.as_str(),
            "blank audio" | "silence" | "music" | "noise" | "background noise" | "no speech" | "inaudible" | "static" | "sound"
        )
}

fn non_speech_option(flags: &std::collections::HashSet<&str>) -> Option<&'static str> {
    ["-sns", "--suppress-nst"].into_iter().find(|flag| flags.contains(flag))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uses_the_exact_suppression_option_advertised_by_the_program() {
        assert_eq!(non_speech_option(&advertised("-m MODEL -f FILE --suppress-nst")), Some("--suppress-nst"));
        assert_eq!(non_speech_option(&advertised("[-sns, --suppress-nst]")), Some("-sns"));
        assert_eq!(non_speech_option(&advertised("--something-else")), None);
        assert!(!advertised("-of FNAME").contains("-f"));
    }
}
