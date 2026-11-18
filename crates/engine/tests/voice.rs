use craft_voice_core::{State, Transcript};
use notecraft_engine::{InvocationOrigin, Output, Params, Session, VoiceUpdate, output_json};
use notecraft_store::{Settings, testing::TempDir};

fn p(values: &[(&str, &str)]) -> Params {
    values.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}
fn session(t: &TempDir) -> Session {
    Session::new(&t.path().join("notebooks"), Settings::at(t.path().join("settings/s.toml"))).unwrap()
}

#[test]
fn serialized_params_cannot_grant_local_microphone_or_edit_authority() {
    let t = TempDir::new("voice-origin");
    let mut s = session(&t);
    let args = p(&[("target", "opaque"), ("origin", "LocalUi")]);
    assert!(s.run("voice.start", &args).is_err());
    assert!(s.run_with_origin("voice.start", &args, InvocationOrigin::Automation).is_err());
    assert_eq!(s.voice().state(), State::Idle);
    assert!(s.run_with_origin("unknown", &args, InvocationOrigin::LocalUi).is_err());
    assert!(s.run("voice.start", &args).is_err());
    s.run_with_origin("voice.start", &args, InvocationOrigin::LocalUi).unwrap();
    assert_eq!(s.voice().state(), State::Preparing);
    assert!(s.run("voice.cancel", &p(&[("session", "1")])).is_err());
}

#[test]
fn headless_status_and_effect_debug_never_reveal_words() {
    let t = TempDir::new("voice-redaction");
    let mut s = session(&t);
    s.run_with_origin("voice.start", &p(&[("target", "opaque")]), InvocationOrigin::LocalUi).unwrap();
    s.voice_update(VoiceUpdate::Ready(1)).unwrap();
    s.voice_update(VoiceUpdate::Stopped(1)).unwrap();
    s.voice_update(VoiceUpdate::Transcript(Transcript::new(1, 1, "private spoken words", true))).unwrap();
    let status = s.run("voice.status", &Params::new()).unwrap();
    assert!(!output_json(&status).to_string().contains("private spoken words"));
    let effect = s.run_with_origin("voice.commit", &p(&[("session", "1"), ("revision", "1")]), InvocationOrigin::LocalUi).unwrap();
    assert!(matches!(effect, Output::Voice(_)));
    assert!(!format!("{effect:?}").contains("private spoken words"));
    assert!(!output_json(&effect).to_string().contains("private spoken words"));
    assert!(s.run_with_origin("voice.commit", &p(&[("session", "1"), ("revision", "1")]), InvocationOrigin::LocalUi).is_err());
    s.voice_update(VoiceUpdate::Applied(1, Err("stale target".into()))).unwrap();
    assert_eq!(s.voice().state(), State::Review);
    assert_eq!(s.voice().draft(), "private spoken words");
}
