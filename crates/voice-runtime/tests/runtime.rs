use craft_voice_runtime::audio::PcmAudio;
use craft_voice_runtime::{Runtime, RuntimeEvent};

#[test]
fn a_missing_local_provider_fails_without_starting_a_microphone() {
    let t = tempfile::tempdir().expect("temp");
    let config = craft_voice_runtime::WhisperConfig::new(t.path().join("missing"), t.path().join("model"), t.path().into());
    let mut runtime = Runtime::from_audio(7, Box::new(config), PcmAudio::new(vec![0; 160]).expect("audio")).expect("worker");
    let end = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut failed = false;
    while std::time::Instant::now() < end {
        for event in runtime.poll() {
            if matches!(event, RuntimeEvent::Error { session: 7, .. }) {
                failed = true;
            }
            assert!(!matches!(event, RuntimeEvent::Ready { .. }));
        }
        if failed {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(failed, "invalid setup must be reported");
}
