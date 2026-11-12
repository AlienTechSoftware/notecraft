use craft_voice_runtime::whisper::{parse_transcript, wait_child};
use craft_voice_runtime::{Cancellation, WhisperConfig};
use std::time::{Duration, Instant};

#[test]
fn whisper_json_fixture_is_text_not_commands_or_logs() {
    let json = br#"{"transcription":[{"text":" Hola"},{"text":" mundo."}]}"#;
    assert_eq!(parse_transcript(json).expect("transcript"), "Hola mundo.");
    assert!(parse_transcript(br#"{"wrong":[]}"#).is_err());
    assert!(parse_transcript(b"not json").is_err());
    let huge = format!("{{\"transcription\":[{{\"text\":\"{}\"}}]}}", "x".repeat(65537));
    assert!(parse_transcript(huge.as_bytes()).is_err());
}

#[test]
fn missing_recognizer_is_a_setup_error() {
    let t = tempfile::tempdir().expect("temporary root");
    let mut c = WhisperConfig::new(t.path().join("missing"), t.path().join("model"), t.path().into());
    assert!(c.validate(&Cancellation::default()).is_err());
}

#[cfg(unix)]
#[test]
fn cancellation_kills_and_reaps_a_real_child() {
    let mut child = std::process::Command::new("/bin/sleep").arg("10").spawn().expect("sleep");
    let cancel = Cancellation::default();
    cancel.cancel();
    let start = Instant::now();
    assert!(wait_child(&mut child, &cancel, Duration::from_secs(20)).is_err());
    assert!(start.elapsed() < Duration::from_secs(2));
    assert!(child.try_wait().expect("reaped status").is_some());
}

#[cfg(unix)]
#[test]
fn timeout_reaps_without_waiting_for_the_childs_normal_exit() {
    let mut child = std::process::Command::new("/bin/sleep").arg("10").spawn().expect("sleep");
    assert!(wait_child(&mut child, &Cancellation::default(), Duration::from_millis(20)).is_err());
    assert!(child.try_wait().expect("status").is_some());
}

#[test]
fn words_in_brackets_are_still_words() {
    let json = br#"{"transcription":[{"text":" Call Ana"},{"text":" (double-check the date)"},{"text":" [laughs] tomorrow."}]}"#;
    assert_eq!(parse_transcript(json).expect("transcript"), "Call Ana (double-check the date) [laughs] tomorrow.");
}

#[test]
fn silence_labels_are_not_words() {
    let json = br#"{"transcription":[{"text":" [BLANK_AUDIO]"},{"text":" Hello"},{"text":" (music)"},{"text":" world."}]}"#;
    assert_eq!(parse_transcript(json).expect("transcript"), "Hello world.");
    assert_eq!(parse_transcript(br#"{"transcription":[{"text":" [BLANK_AUDIO]"}]}"#).expect("silence"), "");
}
