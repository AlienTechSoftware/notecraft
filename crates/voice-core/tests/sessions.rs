use craft_voice_core::{MAX_TRANSCRIPT_BYTES, State, Transcript, VoiceError, VoiceSession};

fn recording() -> (VoiceSession, u64) {
    let mut s = VoiceSession::default();
    let id = s.start("opaque-target").unwrap();
    s.ready(id).unwrap();
    (s, id)
}
fn review() -> (VoiceSession, u64) {
    let (mut s, id) = recording();
    s.stop(id).unwrap();
    s.accept(Transcript::new(id, 1, "Hola 世界", true)).unwrap();
    (s, id)
}

#[test]
fn partials_replace_and_final_requires_stop() {
    let (mut s, id) = recording();
    s.accept(Transcript::new(id, 1, "Hello", false)).unwrap();
    s.accept(Transcript::new(id, 2, "Hola mundo", false)).unwrap();
    assert_eq!(s.draft(), "Hola mundo");
    assert_eq!(s.accept(Transcript::new(id, 1, "old", false)), Err(VoiceError::StaleRevision));
    assert_eq!(s.accept(Transcript::new(id, 3, "final", true)), Err(VoiceError::InvalidState));
    s.stop(id).unwrap();
    s.accept(Transcript::new(id, 3, "Hola mundo.", true)).unwrap();
    assert_eq!(s.state(), State::Review);
    assert_eq!(s.draft(), "Hola mundo.");
}

#[test]
fn edits_and_retarget_invalidate_previous_approval() {
    let (mut s, id) = review();
    s.set_draft(id, 1, "reviewed text").unwrap();
    assert_eq!(s.begin_apply(id, 1), Err(VoiceError::StaleRevision));
    s.retarget(id, "other-position").unwrap();
    assert_eq!(s.begin_apply(id, 2), Err(VoiceError::StaleRevision));
    assert_eq!(s.begin_apply(id, 3).unwrap(), "reviewed text");
}

#[test]
fn apply_is_one_shot_and_target_failure_keeps_draft() {
    let (mut s, id) = review();
    assert_eq!(s.begin_apply(id, 1).unwrap(), "Hola 世界");
    assert_eq!(s.begin_apply(id, 1), Err(VoiceError::InvalidState));
    s.applied(id, Err("target moved".into())).unwrap();
    assert_eq!(s.state(), State::Review);
    assert_eq!(s.draft(), "Hola 世界");
    s.begin_apply(id, 1).unwrap();
    s.applied(id, Ok(())).unwrap();
    assert_eq!(s.state(), State::Complete);
    assert!(s.draft().is_empty());
    assert_eq!(s.begin_apply(id, 1), Err(VoiceError::InvalidState));
}

#[test]
fn cancellation_rejects_late_results_and_new_session_has_new_id() {
    let (mut s, id) = recording();
    s.accept(Transcript::new(id, 1, "private words", false)).unwrap();
    s.cancel(id).unwrap();
    assert!(s.draft().is_empty());
    let next = s.start("new-target").unwrap();
    assert!(next > id);
    assert_eq!(s.accept(Transcript::new(id, 99, "late", false)), Err(VoiceError::StaleSession));
    assert_eq!(s.state(), State::Preparing);
}

#[test]
fn oversized_transcript_is_rejected_without_changing_draft() {
    let (mut s, id) = review();
    assert_eq!(s.set_draft(id, 1, &"x".repeat(MAX_TRANSCRIPT_BYTES + 1)), Err(VoiceError::TooLong));
    assert_eq!(s.draft(), "Hola 世界");
    assert_eq!(s.revision(), 1);
    s.set_draft(id, 1, "").unwrap();
    assert_eq!(s.begin_apply(id, 2), Err(VoiceError::Empty));
}

#[test]
fn failure_retains_partial_and_debug_redacts_transcript() {
    let (mut s, id) = recording();
    let t = Transcript::new(id, 1, "secret personal text", false);
    assert!(!format!("{t:?}").contains("secret personal text"));
    s.accept(t).unwrap();
    s.fail(id, "device disconnected").unwrap();
    assert_eq!(s.state(), State::Review);
    assert_eq!(s.error(), Some("device disconnected"));
    assert_eq!(s.draft(), "secret personal text");
    assert!(!format!("{s:?}").contains("secret personal text"));
}
