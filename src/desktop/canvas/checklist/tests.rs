use super::*;
fn shell() -> ShellState {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.core.active_account = Some("00000000-0000-4000-8000-000000000091".into());
    shell
}
fn open(shell: &mut ShellState, file: &str) {
    shell.shortcut_generation = shell.shortcut_generation.wrapping_add(1);
    shell.canvas = Some(crate::state::CanvasTarget {
        id: file.into(),
        title: "Synthetic canvas".into(),
        permalink: None,
    });
    shell.overlay = Some(Overlay::Canvas);
    open_visit(shell);
}
#[test]
fn read_generation_and_reopen_reject_old_completion_without_touching_new_view() {
    let mut shell = shell();
    open(&mut shell, "FCANVAS");
    let first = begin_read(&mut shell).unwrap();
    assert!(begin_read(&mut shell).is_none(), "only one current load");
    assert!(invalidate(&mut shell, "T1", "FCANVAS"));
    let second = begin_read(&mut shell).unwrap();
    assert!(!finish_read(&mut shell, &first, Err("old failure")));
    assert!(presentation(&shell).unwrap().loading);
    assert!(finish_read(&mut shell, &second, Err("current failure")));
    assert_eq!(presentation(&shell).unwrap().error, Some("current failure"));
    let old_visit = second.visit;
    open(&mut shell, "FCANVAS");
    assert_ne!(presentation(&shell).unwrap().visit, old_visit);
    assert!(!finish_read(&mut shell, &second, Err("late failure")));
    assert_eq!(presentation(&shell).unwrap().error, None);
}
#[test]
fn close_and_navigation_retire_loads_including_restore_to_the_same_file() {
    let mut shell = shell();
    open(&mut shell, "FONE");
    let read = begin_read(&mut shell).unwrap();
    shell.dismiss_overlay();
    assert!(shell.canvas_checklist.view.is_none());
    assert!(!read.current(&shell));
    open(&mut shell, "FTWO");
    open(&mut shell, "FONE");
    assert!(!read.current(&shell));
    assert!(!finish_read(&mut shell, &read, Err("old visit")));
    assert!(presentation(&shell).unwrap().stale);
}
#[test]
fn account_viewer_and_deleted_file_are_part_of_read_ownership() {
    let mut shell = shell();
    open(&mut shell, "FCANVAS");
    let read = begin_read(&mut shell).unwrap();
    shell.core.account_epoch += 1;
    assert!(!read.current(&shell));
    shell.core.account_epoch -= 1;
    let account = shell.core.active_account.clone();
    shell.core.active_account = Some("different-account".into());
    assert!(!read.current(&shell));
    shell.core.active_account = account;
    let user = shell.self_account.user_id.clone();
    shell.self_account.user_id = "different-viewer".into();
    assert!(!read.current(&shell));
    shell.self_account.user_id = user;
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .deleted_files
        .insert("FCANVAS".into());
    assert!(!read.current(&shell));
}
#[test]
fn unrelated_events_do_not_retire_the_current_load() {
    let mut shell = shell();
    open(&mut shell, "FCANVAS");
    let read = begin_read(&mut shell).unwrap();
    assert!(!invalidate(&mut shell, "TOTHER", "FCANVAS"));
    assert!(!invalidate(&mut shell, "T1", "FOTHER"));
    assert!(read.current(&shell));
}

#[path = "test_snapshot.rs"]
pub(super) mod fixtures;
fn loaded(shell: &mut ShellState, checked: bool, sequence: u64) -> Read {
    let read = begin_read(shell).unwrap();
    recovery::tests::load_empty_isolated(shell, &read);
    assert!(finish_read(
        shell,
        &read,
        Ok(fixtures::snapshot_for_file(&read.file, checked, sequence))
    ));
    read
}
fn attempt(shell: &mut ShellState) -> Attempt {
    let view = presentation(shell).unwrap();
    begin(shell, view.visit, view.read, "item", true).unwrap()
}
#[test]
fn unsent_intent_cancels_on_close_and_cannot_dispatch_after_read_replacement() {
    let mut shell = shell();
    open(&mut shell, "FCANVAS");
    loaded(&mut shell, false, 1000);
    let old = attempt(&mut shell);
    assert!(dispatchable(&shell, &old));
    loaded(&mut shell, false, 1000);
    assert!(!dispatchable(&shell, &old));
    assert!(cancel_preflight(&mut shell, &old));
    let old = attempt(&mut shell);
    shell.dismiss_overlay();
    assert!(!dispatchable(&shell, &old));
    open(&mut shell, "FCANVAS");
    loaded(&mut shell, false, 1000);
    assert!(presentation(&shell).unwrap().operation.is_none());
    assert!(!cancel_preflight(&mut shell, &old));
}
#[test]
fn lost_sent_response_survives_close_and_does_not_replay_or_resolve_from_a_boolean() {
    let mut shell = shell();
    open(&mut shell, "FCANVAS");
    loaded(&mut shell, false, 1000);
    let old = attempt(&mut shell);
    let prepared = fixtures::prepare(&mut shell, &old);
    assert!(mark_dispatched(&mut shell, &old, &prepared));
    assert!(!mark_dispatched(&mut shell, &old, &prepared));
    assert!(!cancel_preflight(&mut shell, &old));
    shell.dismiss_overlay();
    assert!(finish(&mut shell, &old, Outcome::Uncertain));
    shell.core.transport = Some(Arc::new(
        super_platinum_core::slack::Transport::new("rotated").unwrap(),
    ));
    shell.core.account_epoch += 1;
    open(&mut shell, "FCANVAS");
    let read = loaded(&mut shell, true, 2000);
    let view = presentation(&shell).unwrap();
    assert_eq!(view.operation.unwrap().2, Stage::Uncertain);
    assert!(begin(&mut shell, view.visit, view.read, "item", false).is_err());
    assert!(!confirm_readback(&mut shell, &old, &read));
    assert!(!dispatchable(&shell, &old));
    assert!(!finish(
        &mut shell,
        &old,
        Outcome::Rejected("late rejection")
    ));
}
#[test]
fn accepted_receipt_needs_matching_session_and_fresh_canonical_sequence() {
    let mut shell = shell();
    open(&mut shell, "FCANVAS");
    let before = loaded(&mut shell, false, 1000);
    let first = attempt(&mut shell);
    let prepared = fixtures::prepare(&mut shell, &first);
    assert!(mark_dispatched(&mut shell, &first, &prepared));
    assert!(finish(
        &mut shell,
        &first,
        Outcome::Accepted(fixtures::accepted(&prepared))
    ));
    assert!(!confirm_readback(&mut shell, &first, &before));
    let stale = loaded(&mut shell, false, 1000);
    assert!(!confirm_readback(&mut shell, &first, &stale));
    // The accepted edit is proved even if a later collaborator restores false;
    // canonical repaint owns the current state, not our desired checkbox value.
    let newer = loaded(&mut shell, false, 3000);
    assert!(confirm_readback(&mut shell, &first, &newer));
    let second = attempt(&mut shell);
    let prepared2 = fixtures::prepare(&mut shell, &second);
    assert!(mark_dispatched(&mut shell, &second, &prepared2));
    assert!(finish(
        &mut shell,
        &second,
        Outcome::Accepted(fixtures::accepted(&prepared))
    ));
    assert_eq!(
        presentation(&shell).unwrap().operation.unwrap().2,
        Stage::Uncertain
    );
}
#[test]
fn edit_session_survives_visits_but_does_not_cross_account_identity() {
    let mut shell = shell();
    open(&mut shell, "FCANVAS");
    loaded(&mut shell, false, 1000);
    let first = attempt(&mut shell);
    let one = fixtures::prepare(&mut shell, &first);
    shell.dismiss_overlay();
    open(&mut shell, "FCANVAS");
    loaded(&mut shell, false, 1000);
    let second = attempt(&mut shell);
    let two = fixtures::prepare(&mut shell, &second);
    assert_eq!(one.session_id(), two.session_id());
    assert_eq!(two.sequence(), one.sequence() + 1);
    assert!(mark_dispatched(&mut shell, &second, &two));
    shell.core.account_epoch += 1;
    shell.core.active_account = Some("another-account".into());
    // A response may settle its original retained receipt without showing state
    // in the new account or making that receipt available for dispatch there.
    assert!(finish(&mut shell, &second, Outcome::Uncertain));
    open(&mut shell, "FCANVAS");
    loaded(&mut shell, false, 1000);
    assert!(presentation(&shell).unwrap().operation.is_none());
    let third = attempt(&mut shell);
    let three = fixtures::prepare(&mut shell, &third);
    assert_ne!(two.session_id(), three.session_id());
    assert_eq!(three.sequence(), 1);
    assert!(!dispatchable(&shell, &second));
}
#[test]
fn known_not_sent_or_definitive_rejection_unlocks_only_its_operation() {
    let mut shell = shell();
    open(&mut shell, "FCANVAS");
    loaded(&mut shell, false, 1000);
    let first = attempt(&mut shell);
    assert!(cancel_preflight(&mut shell, &first));
    assert!(presentation(&shell).unwrap().operation.is_none());
    let next = attempt(&mut shell);
    let prepared = fixtures::prepare(&mut shell, &next);
    assert!(mark_dispatched(&mut shell, &next, &prepared));
    assert!(finish(
        &mut shell,
        &next,
        Outcome::Rejected("Permission denied")
    ));
    assert!(presentation(&shell).unwrap().operation.is_none());
    assert_eq!(
        presentation(&shell).unwrap().error,
        Some("Permission denied")
    );
    assert!(
        presentation(&shell).unwrap().stale,
        "still requires a fresh canonical read"
    );
}

#[test]
fn credential_rotation_keeps_duplicate_lock_but_old_ack_can_settle_via_current_read() {
    let mut shell = shell();
    shell.core.transport = Some(Arc::new(
        super_platinum_core::slack::Transport::new("synthetic").unwrap(),
    ));
    open(&mut shell, "FCANVAS");
    loaded(&mut shell, false, 1000);
    let old = attempt(&mut shell);
    let prepared = fixtures::prepare(&mut shell, &old);
    assert!(mark_dispatched(&mut shell, &old, &prepared));
    shell.core.transport = Some(Arc::new(
        super_platinum_core::slack::Transport::new("rotated").unwrap(),
    ));
    shell.core.account_epoch += 1;
    open(&mut shell, "FCANVAS");
    let read = loaded(&mut shell, false, 1000);
    let view = presentation(&shell).unwrap();
    assert_eq!(view.operation.unwrap().2, Stage::Saving);
    assert!(begin(&mut shell, view.visit, view.read, "item", true).is_err());
    assert!(!dispatchable(&shell, &old));
    assert!(old.request(&shell).is_none());
    assert!(finish(
        &mut shell,
        &old,
        Outcome::Accepted(fixtures::accepted(&prepared))
    ));
    assert_eq!(
        presentation(&shell).unwrap().operation.unwrap().2,
        Stage::AwaitingRead
    );
    assert!(!confirm_readback(&mut shell, &old, &read));
    let fresh = loaded(&mut shell, true, 2000);
    assert!(confirm_readback(&mut shell, &old, &fresh));
    assert!(presentation(&shell).unwrap().operation.is_none());
}
#[test]
fn stable_owner_capacity_survives_rotation_and_other_accounts_have_separate_capacity() {
    let mut shell = shell();
    for owner in 0..4 {
        shell.core.active_account = Some(format!("00000000-0000-4000-8000-{owner:012}"));
        shell.core.account_epoch += 1;
        for item in 0..MAX_RECEIPTS {
            shell.core.transport = Some(Arc::new(
                super_platinum_core::slack::Transport::new("synthetic").unwrap(),
            ));
            open(&mut shell, &format!("FCANVAS{item}"));
            loaded(&mut shell, false, 1000);
            let pending = attempt(&mut shell);
            let prepared = fixtures::prepare(&mut shell, &pending);
            assert!(mark_dispatched(&mut shell, &pending, &prepared));
            assert!(finish(&mut shell, &pending, Outcome::Uncertain));
        }
        open(&mut shell, "FEXTRA");
        loaded(&mut shell, false, 1000);
        let view = presentation(&shell).unwrap();
        assert!(view.capacity_reason.unwrap().starts_with("Eight"));
        assert!(begin(&mut shell, view.visit, view.read, "item", true).is_err());
        assert_eq!(view.outstanding_total, (owner + 1) * MAX_RECEIPTS);
    }
    shell.core.active_account = Some("00000000-0000-4000-8000-000000000004".into());
    shell.core.account_epoch += 1;
    open(&mut shell, "FEXTRA");
    loaded(&mut shell, false, 1000);
    let view = presentation(&shell).unwrap();
    assert!(view.capacity_reason.unwrap().contains("across accounts"));
    assert!(begin(&mut shell, view.visit, view.read, "item", true).is_err());
    assert_eq!(shell.canvas_checklist.operations.len(), MAX_TOTAL_RECEIPTS);
    assert!(
        shell
            .canvas_checklist
            .operations
            .iter()
            .all(|operation| operation.stage == Stage::Uncertain)
    );
}
#[test]
fn workspace_navigation_preserves_uncertainty_and_owner_capacity() {
    let mut shell = shell();
    open(&mut shell, "FCANVAS");
    loaded(&mut shell, false, 1000);
    let old = attempt(&mut shell);
    let prepared = fixtures::prepare(&mut shell, &old);
    assert!(mark_dispatched(&mut shell, &old, &prepared));
    assert!(finish(&mut shell, &old, Outcome::Uncertain));
    shell.dismiss_overlay();
    shell.core.active_team = Some("OTHER".into());
    assert!(
        old.scope.account_current(&shell),
        "changing active workspace does not replace its authenticated owner"
    );
    assert!(!old.scope.current(&shell));
    shell.core.active_team = Some("T1".into());
    open(&mut shell, "FCANVAS");
    loaded(&mut shell, false, 1000);
    let view = presentation(&shell).unwrap();
    assert_eq!(view.operation.unwrap().2, Stage::Uncertain);
    assert!(begin(&mut shell, view.visit, view.read, "item", false).is_err());
}

#[test]
fn consecutive_local_edits_reuse_acknowledged_crdt_when_full_read_omits_it() {
    let mut shell = shell();
    open(&mut shell, "FCANVAS");
    let initial = fixtures::local_snapshot(false, 1000);
    let read = begin_read(&mut shell).unwrap();
    assert!(finish_read(&mut shell, &read, Ok(initial.clone())));
    let first = attempt(&mut shell);
    let prepared = fixtures::prepare(&mut shell, &first);
    let first_crdt = prepared
        .document_data()
        .message(1)
        .unwrap()
        .unwrap()
        .message(38)
        .unwrap()
        .unwrap();
    assert!(mark_dispatched(&mut shell, &first, &prepared));
    let canonical = fixtures::local_snapshot(true, 2000);
    let accepted = fixtures::accepted_with_snapshot(&prepared, &canonical, 1_000_000_000_000);
    assert!(finish(&mut shell, &first, Outcome::Accepted(accepted)));
    let read = begin_read(&mut shell).unwrap();
    assert!(finish_read(&mut shell, &read, Ok(canonical.clone())));
    assert!(confirm_readback(&mut shell, &first, &read));
    let view = presentation(&shell).unwrap();
    let second = begin(&mut shell, view.visit, view.read, "item", false).unwrap();
    let intent = super_platinum_core::canvas::edit::CheckboxIntent::capture(
        &canonical, "item", false, 2_000_010,
    )
    .unwrap();
    let restored = with_edit_session(&mut shell, &second, |session| {
        intent.prepare(&canonical, session, 2_000_010).unwrap()
    })
    .unwrap();
    let second_crdt = restored
        .document_data()
        .message(1)
        .unwrap()
        .unwrap()
        .message(38)
        .unwrap()
        .unwrap();
    assert_eq!(prepared.session_id(), restored.session_id());
    assert_eq!(restored.sequence(), prepared.sequence() + 1);
    assert_eq!(
        first_crdt.bytes(5).unwrap(),
        second_crdt.bytes(5).unwrap(),
        "later clock must not regenerate actor sequence one"
    );
    assert_eq!(first_crdt.bytes(6).unwrap(), second_crdt.bytes(6).unwrap());
    assert_eq!(second_crdt.uint(3).unwrap(), Some(2000));
    assert!(
        !second_crdt.has(4),
        "server expiry is not echoed in the request"
    );
}

#[test]
fn late_ack_after_auth_rotation_settles_receipt_without_adopting_old_crdt() {
    let mut shell = shell();
    open(&mut shell, "FCANVAS");
    let read = begin_read(&mut shell).unwrap();
    finish_read(&mut shell, &read, Ok(fixtures::local_snapshot(false, 1000)));
    let first = attempt(&mut shell);
    let prepared = fixtures::prepare(&mut shell, &first);
    assert!(mark_dispatched(&mut shell, &first, &prepared));
    shell.core.account_epoch += 1;
    shell.core.transport = Some(Arc::new(
        super_platinum_core::slack::Transport::new("replacement").unwrap(),
    ));
    open(&mut shell, "FCANVAS");
    let canonical = fixtures::local_snapshot(true, 2000);
    assert!(finish(
        &mut shell,
        &first,
        Outcome::Accepted(fixtures::accepted_with_snapshot(
            &prepared,
            &canonical,
            1_000_000_000_000
        ))
    ));
    assert_eq!(
        presentation(&shell).unwrap().operation.unwrap().2,
        Stage::AwaitingRead
    );
    let read = begin_read(&mut shell).unwrap();
    recovery::tests::load_empty_isolated(&mut shell, &read);
    finish_read(&mut shell, &read, Ok(canonical.clone()));
    assert!(confirm_readback(&mut shell, &first, &read));
    let view = presentation(&shell).unwrap();
    let next = begin(&mut shell, view.visit, view.read, "item", false).unwrap();
    let intent = super_platinum_core::canvas::edit::CheckboxIntent::capture(
        &canonical, "item", false, 2_000_010,
    )
    .unwrap();
    let fresh = with_edit_session(&mut shell, &next, |session| {
        intent.prepare(&canonical, session, 2_000_010).unwrap()
    })
    .unwrap();
    assert_ne!(fresh.session_id(), prepared.session_id());
    assert_eq!(fresh.sequence(), 1);
    let old_bytes = prepared
        .document_data()
        .message(1)
        .unwrap()
        .unwrap()
        .message(38)
        .unwrap()
        .unwrap();
    let fresh_bytes = fresh
        .document_data()
        .message(1)
        .unwrap()
        .unwrap()
        .message(38)
        .unwrap()
        .unwrap();
    assert_ne!(old_bytes.bytes(5).unwrap(), fresh_bytes.bytes(5).unwrap());
    assert_eq!(
        shell.canvas_checklist.sessions.len(),
        1,
        "only the replacement full scope remains"
    );
}

#[test]
fn validated_file_events_retire_preflight_and_reads_but_refresh_recovers() {
    use super_platinum_core::{
        config::{Session, WorkspaceSession},
        slack::{Transport, realtime::parse_event},
    };
    let mut shell = shell();
    shell.core.transport = Some(Arc::new(Transport::new("fixture").unwrap()));
    shell.core.session = Some(Session {
        unified_user_client_enabled: Default::default(),
        d_cookie: "fixture".into(),
        workspaces: [(
            "T1".into(),
            WorkspaceSession {
                team_id: "T1".into(),
                user_id: shell.self_account.user_id.clone(),
                enterprise_id: None,
                name: "Fixture".into(),
                url: "https://example.invalid".into(),
                token: String::new(),
            },
        )]
        .into(),
    });
    shell.core.workspaces.get_mut("T1").unwrap().rt_generation = 4;
    open(&mut shell, "FCANVAS");
    let read = loaded(&mut shell, false, 1000);
    let pending = attempt(&mut shell);
    for (generation, raw) in [
        (3, r#"{"type":"file_change","file_id":"FCANVAS"}"#),
        (4, r#"{"type":"file_change","file_id":"FOTHER"}"#),
        (
            4,
            r#"{"type":"file_change","file_id":"FCANVAS","file":{"id":"FOTHER"}}"#,
        ),
    ] {
        crate::file_events::prepare(&mut shell, "T1", generation, &parse_event(raw).unwrap());
        assert!(read.current(&shell));
        assert!(dispatchable(&shell, &pending));
    }
    let event = parse_event(r#"{"type":"file_change","file_id":"FCANVAS"}"#).unwrap();
    crate::file_events::prepare(&mut shell, "T1", 4, &event);
    assert!(!read.current(&shell));
    assert!(!dispatchable(&shell, &pending));
    assert!(presentation(&shell).unwrap().stale);
    assert!(!finish_read(&mut shell, &read, Err("late")));
    loaded(&mut shell, false, 1001);
    assert!(!presentation(&shell).unwrap().stale);
    let event = parse_event(r#"{"type":"file_deleted","file_id":"FCANVAS"}"#).unwrap();
    crate::file_events::prepare(&mut shell, "T1", 4, &event);
    assert!(presentation(&shell).unwrap().stale);
}

#[test]
fn retired_export_releases_only_its_own_loading_and_preserves_current_content() {
    let mut reader = crate::canvas::Reader {
        loading: true,
        generation: 7,
        title: Some("Retained".into()),
        ..Default::default()
    };
    assert!(!reader.finish_export(6, true));
    assert!(
        reader.loading,
        "an older export cannot release a newer load"
    );
    assert!(!reader.finish_export(7, false));
    assert!(
        !reader.loading,
        "canonical invalidation must leave Refresh usable"
    );
    assert_eq!(reader.title.as_deref(), Some("Retained"));
    reader.loading = true;
    assert!(reader.finish_export(7, true));
    assert!(!reader.loading);
}

#[test]
fn refresh_waits_for_dispatch_but_remains_available_for_receipt_recovery() {
    assert!(Stage::Preparing.blocks_refresh());
    assert!(Stage::Saving.blocks_refresh());
    assert!(!Stage::AwaitingRead.blocks_refresh());
    assert!(!Stage::Uncertain.blocks_refresh());
}

#[test]
fn reopen_during_saving_loads_once_after_settlement_without_a_file_event() {
    for outcome in 0..3 {
        let mut shell = shell();
        open(&mut shell, "FCANVAS");
        loaded(&mut shell, false, 1000);
        let old = attempt(&mut shell);
        let prepared = fixtures::prepare(&mut shell, &old);
        assert!(mark_dispatched(&mut shell, &old, &prepared));
        shell.dismiss_overlay();
        open(&mut shell, "FCANVAS");
        let view = presentation(&shell).unwrap();
        assert_eq!(view.read, 0);
        assert!(view.snapshot.is_none());
        assert!(
            event_reads::needed(&shell).is_none(),
            "never read during Saving"
        );
        let result = match outcome {
            0 => Outcome::Accepted(fixtures::accepted(&prepared)),
            1 => Outcome::Uncertain,
            _ => Outcome::Rejected("Not sent"),
        };
        assert!(finish(&mut shell, &old, result));
        assert_eq!(event_reads::needed(&shell), Some(view.visit));
        let read = event_reads::claim(&mut shell, view.visit).unwrap();
        assert!(event_reads::claim(&mut shell, view.visit).is_none());
        assert!(finish_read(
            &mut shell,
            &read,
            Ok(fixtures::snapshot(true, 2000))
        ));
        if outcome == 0 {
            assert!(confirm_readback(&mut shell, &old, &read));
            assert!(presentation(&shell).unwrap().operation.is_none());
        } else if outcome == 1 {
            assert!(!confirm_readback(&mut shell, &old, &read));
            assert_eq!(
                presentation(&shell).unwrap().operation.unwrap().2,
                Stage::Uncertain
            );
        }
        assert!(
            event_reads::needed(&shell).is_none(),
            "no repeated initial loading"
        );
        assert!(!dispatchable(&shell, &old));
    }
}

#[test]
fn own_file_event_readback_replacement_is_bounded_and_visit_owned() {
    let mut shell = shell();
    open(&mut shell, "FCANVAS");
    loaded(&mut shell, false, 1000);
    let _ = attempt(&mut shell);
    shell.canvas_checklist.operations[0].stage = Stage::AwaitingRead;
    let old = begin_read(&mut shell).unwrap();
    let mut retries = 1;
    assert!(retry_invalidated_read(&mut shell, &old, &mut retries).is_none());
    invalidate(&mut shell, "T1", "FCANVAS");
    let next = retry_invalidated_read(&mut shell, &old, &mut retries).unwrap();
    assert_eq!(retries, 0);
    assert!(next.current(&shell));
    assert!(!old.current(&shell));
    invalidate(&mut shell, "T1", "FCANVAS");
    assert!(retry_invalidated_read(&mut shell, &next, &mut retries).is_none());
    assert!(!presentation(&shell).unwrap().loading);
    let manual = begin_read(&mut shell).unwrap();
    assert!(retry_invalidated_read(&mut shell, &next, &mut 1).is_none());
    assert!(manual.current(&shell));
    invalidate(&mut shell, "T1", "FCANVAS");
    shell.core.account_epoch += 1;
    assert!(retry_invalidated_read(&mut shell, &manual, &mut 1).is_none());
    shell.core.account_epoch -= 1;
    open(&mut shell, "FCANVAS");
    assert!(retry_invalidated_read(&mut shell, &manual, &mut 1).is_none());
}

#[test]
fn accepted_notice_requests_manual_refresh_only_when_no_read_is_running() {
    let mut shell = shell();
    open(&mut shell, "FCANVAS");
    loaded(&mut shell, false, 1000);
    let _ = attempt(&mut shell);
    shell.canvas_checklist.operations[0].stage = Stage::AwaitingRead;
    assert_eq!(
        reader::notice(&presentation(&shell).unwrap()),
        Some("Change accepted. Refresh to check canvas state.")
    );
    begin_read(&mut shell).unwrap();
    assert_eq!(
        reader::notice(&presentation(&shell).unwrap()),
        Some("Change accepted. Refreshing canvas state…")
    );
}

#[test]
fn synthetic_transport_requires_actual_empty_journal_completion_before_editing() {
    let mut shell = shell();
    shell.core.transport = Some(Arc::new(
        super_platinum_core::slack::Transport::new("synthetic").unwrap(),
    ));
    open(&mut shell, "FCANVAS");
    let read = begin_read(&mut shell).unwrap();
    assert!(finish_read(
        &mut shell,
        &read,
        Ok(fixtures::snapshot_for_file("FCANVAS", false, 1000))
    ));
    let view = presentation(&shell).unwrap();
    assert!(begin(&mut shell, view.visit, view.read, "item", true).is_err());
    assert!(shell.canvas_checklist.operations.is_empty());
    recovery::tests::load_empty_isolated(&mut shell, &read);
    let pending = attempt(&mut shell);
    assert!(dispatchable(&shell, &pending));
}
