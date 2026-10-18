use super::*;
#[test]
fn cancel_during_preflight_preserves_429_deadline_through_local_settlement() {
    let mut s = shell();
    let p = s.profile_edit.recording.pane.clone().unwrap();
    s.profile_edit
        .recording
        .pane
        .as_mut()
        .unwrap()
        .capture_phase = "preview".into();
    let r = begin(&mut s, p.visit, p.serial, Operation::Upload).unwrap();
    let scope = protocol::custody::Scope::new(
        &r.token.owner.account,
        &r.token.owner.team,
        &r.token.owner.user,
    )
    .unwrap();
    let mut record = protocol::custody::Record::capture(scope, &r.form).unwrap();
    record
        .set_audio(protocol::custody::Audio::new(vec![1, 2, 3], "audio/webm", 1000).unwrap())
        .unwrap();
    record
        .begin(protocol::custody::Operation::Upload, 1)
        .unwrap();
    let e = entry_mut(&mut s, &r).unwrap();
    e.retained = Some(record.clone());
    e.inflight = Some(r.id);
    cancel::request_shell(&mut s, p.visit, p.serial).unwrap();
    let before = now();
    let error = Error::RateLimited {
        retry_after_secs: Some(120),
    };
    storage::failure_record(&mut record, &error, false, true).unwrap();
    cancel::merge_intent(entry_mut(&mut s, &r).unwrap(), &mut record).unwrap();
    entry_mut(&mut s, &r).unwrap().retained = Some(record.clone());
    network::finish_failed(&mut s, &r, error, false);
    assert!(entry_mut(&mut s, &r).unwrap().receipt.is_some());
    assert!(record.retry_at() >= (before + 120) as u64);
    record.cancel_before_dispatch().unwrap();
    let e = entry_mut(&mut s, &r).unwrap();
    e.cancel_requested = None;
    e.retained = Some(record);
    storage::finish_task_shell(&mut s, &r.token.owner, r.id);
    assert!(!presentation(&s).unwrap().can_record);
    assert!(!presentation(&s).unwrap().can_save);
    assert!(
        s.profile_edit
            .recording
            .entries
            .get(&r.token.owner)
            .unwrap()
            .retry_at
            >= before + 120
    );
}
#[test]
fn adopted_staged_file_cleanup_retains_readback_without_deleting() {
    let mut s = shell();
    fixture::install_recovery(&mut s, false);
    let (v, n) = keys(&s);
    s.profile_edit.recording.fixture.as_mut().unwrap().raw["result"]["data"]["user"]["profileSections"]
        [0]["profileElements"][2]["fileId"] = serde_json::json!("F00000002");
    fixture::cleanup_shell(&mut s, v, n);
    assert!(
        s.profile_edit
            .recording
            .fixture
            .as_ref()
            .unwrap()
            .attempts
            .is_empty()
    );
    assert!(presentation(&s).unwrap().can_check);
    assert!(presentation(&s).unwrap().can_keep);
    let p = s.profile_edit.recording.pane.clone().unwrap();
    let e = s
        .profile_edit
        .recording
        .entries
        .get_mut(&p.token.owner)
        .unwrap();
    let fresh = e.checked.clone().unwrap();
    let record = e.retained.as_mut().unwrap();
    record.keep_current(&fresh).unwrap();
    assert!(record.staged_file().is_none());
    assert_eq!(record.form().unwrap().current_file(), Some("F00000002"));
}
#[test]
fn queued_audio_owns_memory_and_io_before_task_start_and_survives_retirement() {
    let mut s = shell();
    let p = s.profile_edit.recording.pane.clone().unwrap();
    let scope = protocol::custody::Scope::new(
        &p.token.owner.account,
        &p.token.owner.team,
        &p.token.owner.user,
    )
    .unwrap();
    let mut record = protocol::custody::Record::capture(scope, p.form.as_ref().unwrap()).unwrap();
    record
        .set_audio(
            protocol::custody::Audio::new(b"new synthetic clip".to_vec(), "audio/webm", 1000)
                .unwrap(),
        )
        .unwrap();
    let queued = queue::reserve(&mut s, &p.token, record).unwrap();
    assert!(presentation(&s).unwrap().busy);
    let e = s
        .profile_edit
        .recording
        .entries
        .get(&p.token.owner)
        .unwrap();
    assert!(
        e.io_gate.try_lock().is_err(),
        "reopen cannot overtake the queued local write"
    );
    assert_eq!(
        e.retained.as_ref().unwrap().audio().unwrap().bytes(),
        b"new synthetic clip"
    );
    s.core.account_epoch += 1;
    poll(&mut s);
    let e = s
        .profile_edit
        .recording
        .entries
        .get(&p.token.owner)
        .unwrap();
    assert!(queue::owns(e, &p.token));
    assert!(!queue::owns(e, &Token::capture(&s).unwrap()));
    assert!(e.retained.is_some());
    drop(queued);
}
#[test]
fn fixture_cancellation_deletes_only_owned_ticket_and_keeps_header() {
    let mut s = shell();
    fixture::install_cancellation(&mut s, true);
    let (v, n) = keys(&s);
    assert!(presentation(&s).unwrap().can_cancel_upload);
    fixture::cancel_upload_shell(&mut s, v, n);
    let f = s.profile_edit.recording.fixture.as_ref().unwrap();
    assert_eq!(
        f.attempts,
        vec![serde_json::json!({"method":"files.delete","file":"F00000002"})]
    );
    assert_eq!(
        s.profile_edit
            .recording
            .pane
            .as_ref()
            .unwrap()
            .form
            .as_ref()
            .unwrap()
            .current_file(),
        Some("F00000001")
    );
    assert!(!presentation(&s).unwrap().uncertain);
    assert!(presentation(&s).unwrap().can_record);
}
#[test]
fn recovered_cancel_check_never_attaches_or_reuploads_the_clip() {
    let mut s = shell();
    fixture::install_cancellation(&mut s, false);
    let (v, n) = keys(&s);
    assert!(presentation(&s).unwrap().can_cleanup);
    fixture::check_shell(&mut s, v, n);
    assert_eq!(
        s.profile_edit
            .recording
            .pane
            .as_ref()
            .unwrap()
            .form
            .as_ref()
            .unwrap()
            .current_file(),
        Some("F00000001")
    );
    assert!(
        s.profile_edit
            .recording
            .fixture
            .as_ref()
            .unwrap()
            .attempts
            .is_empty()
    );
    assert!(presentation(&s).unwrap().can_keep);
}
#[test]
fn cancellation_merges_into_late_stage_but_rejects_stale_completion_snapshot() {
    let mut s = shell();
    fixture::install_cancellation(&mut s, true);
    let (v, n) = keys(&s);
    let r = cancel::request_shell(&mut s, v, n).unwrap();
    let e = entry_mut(&mut s, &r).unwrap();
    let mut late = e.retained.as_ref().unwrap().clone();
    let mut completion = late.clone();
    completion.confirm_upload("F00000002").unwrap();
    assert!(cancel::merge_intent(e, &mut completion).is_err());
    cancel::merge_intent(e, &mut late).unwrap();
    assert!(late.cancel_requested());
    assert_eq!(late.staged_file(), Some("F00000002"));
    assert!(late.confirm_upload("F00000002").is_err());
}
#[test]
fn retaining_audio_blocks_new_capture_and_header_mutation() {
    let mut s = shell();
    let (v, n) = keys(&s);
    let p = s.profile_edit.recording.pane.as_mut().unwrap();
    p.capture_phase = "retaining".into();
    p.form.as_mut().unwrap().remove();
    let view = presentation(&s).unwrap();
    assert!(!view.can_record && !view.can_save && !view.can_remove && !view.can_cleanup);
    assert!(!reserve_capture(&mut s, v, n));
    assert!(begin(&mut s, v, n, Operation::Save).is_none());
}
#[test]
fn retirement_preserves_active_preparation_until_original_task_settles() {
    let mut s = shell();
    let (v, n) = keys(&s);
    s.profile_edit
        .recording
        .pane
        .as_mut()
        .unwrap()
        .capture_phase = "preview".into();
    let r = begin(&mut s, v, n, Operation::Upload).unwrap();
    entry_mut(&mut s, &r).unwrap().inflight = Some(r.id);
    s.core.account_epoch += 1;
    poll(&mut s);
    assert!(entry_mut(&mut s, &r).is_some());
    assert!(
        entry_mut(&mut s, &r)
            .unwrap()
            .receipt
            .as_ref()
            .unwrap()
            .stage
            == Stage::Unknown
    );
}
#[test]
fn retirement_reopen_keeps_inflight_identity_until_late_ticket_is_retained() {
    use protocol::custody as d;
    let mut s = shell();
    let (v, n) = keys(&s);
    s.profile_edit
        .recording
        .pane
        .as_mut()
        .unwrap()
        .capture_phase = "preview".into();
    let r = begin(&mut s, v, n, Operation::Upload).unwrap();
    let scope = d::Scope::new(
        &r.token.owner.account,
        &r.token.owner.team,
        &r.token.owner.user,
    )
    .unwrap();
    let mut retained = d::Record::capture(scope, &r.form).unwrap();
    retained
        .set_audio(d::Audio::new(vec![1, 2, 3], "audio/webm", 1000).unwrap())
        .unwrap();
    retained.begin(d::Operation::Upload, 1).unwrap();
    retained.mark(d::Stage::Ticket).unwrap();
    let e = entry_mut(&mut s, &r).unwrap();
    e.receipt.as_mut().unwrap().stage = Stage::Ticket;
    e.retained = Some(retained);
    e.inflight = Some(r.id);
    s.core.account_epoch += 1;
    poll(&mut s);
    let current_token = Token::capture(&s).unwrap();
    s.profile_edit.draft.as_mut().unwrap().token = current_token.clone();
    let _ = open_shell(&mut s, v);
    let reopened = s.profile_edit.recording.pane.clone().unwrap();
    storage::install(&mut s, &current_token, v, reopened.serial);
    assert!(presentation(&s).unwrap().busy);
    assert!(!presentation(&s).unwrap().can_check);
    let e =
        entry_mut(&mut s, &r).expect("original operation retained across reopened presentation");
    let ticket=protocol::UploadTicket::decode(&serde_json::json!({"ok":true,"file":"F00000002","upload_url":"https://files.slack.com/upload/synthetic"})).unwrap();
    e.receipt.as_mut().unwrap().ticket = Some(ticket.clone());
    let retained = e.retained.as_mut().unwrap();
    retained.remember_ticket(ticket.file()).unwrap();
    retained.mark(d::Stage::Bytes).unwrap();
    storage::finish_task_shell(&mut s, &r.token.owner, r.id);
    let e = s
        .profile_edit
        .recording
        .entries
        .get(&r.token.owner)
        .unwrap();
    assert_eq!(
        e.retained.as_ref().unwrap().staged_file(),
        Some("F00000002")
    );
    assert_eq!(e.receipt.as_ref().unwrap().file(), Some("F00000002"));
    assert!(e.receipt.as_ref().unwrap().token == current_token);
    assert!(presentation(&s).unwrap().can_check);
}
#[test]
fn finished_capture_attaches_upload_but_still_requires_explicit_profile_save() {
    let mut s = shell();
    let (v, n) = keys(&s);
    fixture::finish_capture_shell(&mut s, v, n);
    let p = s.profile_edit.recording.pane.as_ref().unwrap();
    assert_eq!(p.form.as_ref().unwrap().current_file(), Some("F00000002"));
    assert!(presentation(&s).unwrap().can_save);
    assert!(presentation(&s).unwrap().can_cleanup);
    assert!(
        s.profile_edit
            .recording
            .fixture
            .as_ref()
            .unwrap()
            .attempts
            .is_empty()
    );
    fixture::save_shell(&mut s, v, n);
    let attempts = &s.profile_edit.recording.fixture.as_ref().unwrap().attempts;
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0]["method"], "users.profile.setSections");
    assert!(!presentation(&s).unwrap().can_cleanup);
}
#[test]
fn recovered_recording_custody_is_owned_check_only_and_disconnected() {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install_recovery(&mut s, true);
    let (v, n) = keys(&s);
    assert!(presentation(&s).unwrap().can_check);
    assert!(!presentation(&s).unwrap().can_save);
    assert!(!presentation(&s).unwrap().can_record);
    assert!(s.core.session.is_none() && s.core.transport.is_none());
    fixture::check_shell(&mut s, v, n);
    assert!(presentation(&s).unwrap().can_save);
    assert!(presentation(&s).unwrap().can_cleanup);
    assert!(
        s.profile_edit
            .recording
            .fixture
            .as_ref()
            .unwrap()
            .attempts
            .is_empty()
    );
    fixture::cleanup_shell(&mut s, v, n);
    let attempts = &s.profile_edit.recording.fixture.as_ref().unwrap().attempts;
    assert_eq!(attempts.len(), 1);
    assert_eq!(
        attempts[0],
        serde_json::json!({"method":"files.delete","file":"F00000002"})
    );
    assert!(!presentation(&s).unwrap().can_cleanup);
}
#[test]
fn storage_failure_blocks_recording_save_check_and_cleanup_but_retains_audio() {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install_recovery(&mut s, true);
    let owner = s
        .profile_edit
        .recording
        .pane
        .as_ref()
        .unwrap()
        .token
        .owner
        .clone();
    let e = s.profile_edit.recording.entries.get_mut(&owner).unwrap();
    e.storage_error = true;
    assert!(e.retained.as_ref().unwrap().audio().is_some());
    let view = presentation(&s).unwrap();
    assert!(view.storage_error);
    assert!(!view.can_record && !view.can_save && !view.can_check && !view.can_cleanup);
}
#[test]
fn retirement_keeps_staged_owner_and_drops_capture_authority() {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install_recovery(&mut s, true);
    let owner = s
        .profile_edit
        .recording
        .pane
        .as_ref()
        .unwrap()
        .token
        .owner
        .clone();
    s.core.account_epoch += 1;
    poll(&mut s);
    assert!(s.profile_edit.recording.pane.is_none());
    let e = s.profile_edit.recording.entries.get(&owner).unwrap();
    assert_eq!(
        e.retained.as_ref().unwrap().staged_file(),
        Some("F00000002")
    );
    assert!(e.receipt.as_ref().unwrap().stage == Stage::Unknown);
}
fn shell() -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install(&mut s);
    s
}
fn keys(s: &ShellState) -> (u64, u64) {
    let p = s.profile_edit.recording.pane.as_ref().unwrap();
    (p.visit, p.serial)
}
#[test]
fn synthetic_recording_upload_then_explicit_save_preserves_pronunciation_and_title() {
    let mut s = shell();
    let (v, n) = keys(&s);
    let p = s.profile_edit.recording.pane.as_mut().unwrap();
    p.capture_phase = "preview".into();
    p.elapsed = 1000;
    fixture::upload_shell(&mut s, v, n);
    assert!(presentation(&s).unwrap().can_save);
    assert_eq!(
        s.profile_edit
            .recording
            .fixture
            .as_ref()
            .unwrap()
            .attempts
            .len(),
        0
    );
    fixture::save_shell(&mut s, v, n);
    let f = s.profile_edit.recording.fixture.as_ref().unwrap();
    assert_eq!(f.attempts.len(), 1);
    assert_eq!(f.reads, 1);
    assert_eq!(
        f.attempts[0]["elements"],
        serde_json::json!([
            {"element_id":"name","text":{"text":"Zo-ee"}},
            {"element_id":"title","text":{"text":"Product engineer"}},
            {"element_id":"recording","file":{"fileId":"F00000002"}}
        ])
    );
    assert!(!pending(&s));
    assert!(!presentation(&s).unwrap().can_save);
}
#[test]
fn recording_clear_uses_file_null_and_never_deletes_the_shared_file() {
    let mut s = shell();
    let (v, n) = keys(&s);
    s.profile_edit
        .recording
        .pane
        .as_mut()
        .unwrap()
        .form
        .as_mut()
        .unwrap()
        .remove();
    fixture::save_shell(&mut s, v, n);
    let f = s.profile_edit.recording.fixture.as_ref().unwrap();
    assert_eq!(f.attempts.len(), 1);
    assert_eq!(f.attempts[0]["method"], "users.profile.setSections");
    assert!(f.attempts[0]["elements"][2]["file"].is_null());
}
#[test]
fn recording_uncertain_receipt_blocks_replay_and_checks_without_a_write() {
    let mut s = shell();
    let (v, n) = keys(&s);
    s.profile_edit
        .recording
        .pane
        .as_mut()
        .unwrap()
        .form
        .as_mut()
        .unwrap()
        .remove();
    let r = begin(&mut s, v, n, Operation::Save).unwrap();
    entry_mut(&mut s, &r)
        .unwrap()
        .receipt
        .as_mut()
        .unwrap()
        .stage = Stage::Unknown;
    assert!(pending(&s));
    assert!(begin(&mut s, v, n, Operation::Save).is_none());
    fixture::check_shell(&mut s, v, n);
    assert!(pending(&s));
    assert!(presentation(&s).unwrap().can_keep);
    assert_eq!(
        s.profile_edit
            .recording
            .fixture
            .as_ref()
            .unwrap()
            .attempts
            .len(),
        0
    );
    assert_eq!(s.profile_edit.recording.fixture.as_ref().unwrap().reads, 1);
}
#[test]
fn recording_account_retirement_removes_ui_authority_but_retains_dispatched_custody() {
    let mut s = shell();
    let (v, n) = keys(&s);
    s.profile_edit
        .recording
        .pane
        .as_mut()
        .unwrap()
        .form
        .as_mut()
        .unwrap()
        .remove();
    let r = begin(&mut s, v, n, Operation::Save).unwrap();
    entry_mut(&mut s, &r)
        .unwrap()
        .receipt
        .as_mut()
        .unwrap()
        .stage = Stage::Saving;
    s.core.account_epoch += 1;
    assert!(poll(&mut s));
    assert!(presentation(&s).is_none());
    assert!(
        s.profile_edit
            .recording
            .entries
            .get(&r.token.owner)
            .unwrap()
            .receipt
            .is_some()
    );
    assert_eq!(
        s.profile_edit
            .recording
            .fixture
            .as_ref()
            .unwrap()
            .attempts
            .len(),
        0
    );
}
#[test]
fn recording_preparing_cancels_on_retirement_and_known_upload_is_not_replayed() {
    let mut s = shell();
    let (v, n) = keys(&s);
    s.profile_edit
        .recording
        .pane
        .as_mut()
        .unwrap()
        .capture_phase = "preview".into();
    let r = begin(&mut s, v, n, Operation::Upload).unwrap();
    s.core.account_epoch += 1;
    poll(&mut s);
    assert!(
        s.profile_edit
            .recording
            .entries
            .get(&r.token.owner)
            .is_none_or(|e| e.receipt.is_none())
    );
    let mut s = shell();
    let (v, n) = keys(&s);
    s.profile_edit
        .recording
        .pane
        .as_mut()
        .unwrap()
        .capture_phase = "preview".into();
    let r = begin(&mut s, v, n, Operation::Upload).unwrap();
    let e = entry_mut(&mut s, &r).unwrap();
    e.receipt.as_mut().unwrap().stage = Stage::Unknown;
    assert!(!presentation(&s).unwrap().can_record);
    assert!(presentation(&s).unwrap().can_check);
    assert!(begin(&mut s, v, n, Operation::Upload).is_none());
}

#[test]
fn recording_start_reserves_before_renderer_and_blocks_stale_save() {
    let mut s = shell();
    let (v, n) = keys(&s);
    s.profile_edit
        .recording
        .pane
        .as_mut()
        .unwrap()
        .form
        .as_mut()
        .unwrap()
        .remove();
    assert!(presentation(&s).unwrap().can_save);
    assert!(reserve_capture(&mut s, v, n));
    assert!(!presentation(&s).unwrap().can_save);
    assert!(begin(&mut s, v, n, Operation::Save).is_none());
    assert!(!reserve_capture(&mut s, v, n));
}
