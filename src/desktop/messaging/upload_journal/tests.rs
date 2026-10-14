use super::*;
use crate::media::MediaRegistry;
use super_platinum_core::slack::models::Message;

const ACCOUNT: &str = "00000000-0000-4000-8000-000000000001";
fn shell() -> ShellState {
    let mut s = ShellState::fixture(MediaRegistry::default());
    s.core.active_account = Some(ACCOUNT.into());
    s.upload_journal = State::isolated();
    s
}
fn receipt(s: &ShellState, thread: bool, broadcast: bool) -> Stored {
    let team = s.core.active_team.clone().unwrap();
    let channel = s.core.active_channel.clone().unwrap();
    let value = Draft {
        account_id: ACCOUNT.into(),
        team_id: team.clone(),
        user_id: s.core.workspaces[&team].self_user_id.clone(),
        channel_id: channel,
        thread_ts: thread.then(|| "1700000000.000001".into()),
        reply_broadcast: broadcast,
        client_msg_id: "super-platinum-file-test".into(),
        optimistic_ts: "1900000000.000001".into(),
        caption: "Synthetic caption 🦀".into(),
        attachments: vec![uploads::Attachment {
            name: "example.txt".into(),
            bytes: 17,
        }],
    };
    let r = s.upload_journal.backend.reserve(value).unwrap();
    s.upload_journal
        .backend
        .finalize(r.stored.ticket, vec!["F1".into()])
        .unwrap()
        .unwrap()
}
#[test]
fn restores_channel_thread_and_broadcast_as_inert_unconfirmed_rows() {
    for (thread, broadcast) in [(false, false), (true, false), (true, true)] {
        let mut s = shell();
        let row = receipt(&s, thread, broadcast);
        restore(&mut s, ACCOUNT, vec![row.clone()]);
        let p = s
            .core
            .pending_file_messages
            .iter()
            .find(|p| p.client_msg_id == row.draft.client_msg_id)
            .unwrap();
        assert_eq!(p.thread_ts, row.draft.thread_ts);
        assert_eq!(p.reply_broadcast, broadcast);
        assert_eq!(p.text, row.draft.caption);
        assert!(p.attachments.iter().all(|a| a.path.as_os_str().is_empty()
            && !a.uploading
            && a.upload_cancel.is_none()
            && a.upload_progress.is_none()));
        assert_eq!(p.finalized.as_ref().unwrap().file_ids, vec!["F1"]);
        assert_eq!(
            s.pending_delivery[&row.draft.optimistic_ts],
            crate::state::PendingDeliveryStatus::Unconfirmed
        );
        let count = s.core.pending_file_messages.len();
        restore(&mut s, ACCOUNT, vec![row]);
        assert_eq!(s.core.pending_file_messages.len(), count);
        assert!(s.core.composer_attachments.is_empty());
        assert!(s.core.thread_composer_attachments.is_empty());
    }
}
#[test]
fn restores_only_exact_account_team_and_self_user_and_never_overwrites_active_upload() {
    let mut s = shell();
    let row = receipt(&s, false, false);
    let before = s.core.pending_file_messages.len();
    let mut wrong = row.clone();
    wrong.draft.user_id = "WOTHER".into();
    restore(&mut s, ACCOUNT, vec![wrong]);
    assert_eq!(s.core.pending_file_messages.len(), before);
    restore(
        &mut s,
        "00000000-0000-4000-8000-000000000002",
        vec![row.clone()],
    );
    assert_eq!(s.core.pending_file_messages.len(), before);
    restore(&mut s, ACCOUNT, vec![row.clone()]);
    let p = s
        .core
        .pending_file_messages
        .iter_mut()
        .find(|p| p.client_msg_id == row.draft.client_msg_id)
        .unwrap();
    p.attachments[0].uploading = true;
    p.text = "active unchanged".into();
    restore(&mut s, ACCOUNT, vec![row]);
    assert!(
        s.core
            .pending_file_messages
            .iter()
            .any(|p| p.text == "active unchanged" && p.attachments[0].uploading)
    );
}
#[test]
fn canonical_ingest_retires_exact_journal_and_stale_load_revision_is_rejected() {
    let mut s = shell();
    let row = receipt(&s, true, true);
    restore(&mut s, ACCOUNT, vec![row.clone()]);
    let revision = s.upload_journal.revision;
    let message:Message=serde_json::from_value(serde_json::json!({"type":"message","ts":"1900000001.000001","user":row.draft.user_id,"channel":row.draft.channel_id,"thread_ts":row.draft.thread_ts,"subtype":"thread_broadcast","files":[{"id":"F1"}]})).unwrap();
    let bag = s
        .core
        .workspaces
        .get_mut(&row.draft.team_id)
        .unwrap()
        .messages
        .entry(row.draft.channel_id.clone())
        .or_default();
    bag.upsert(message);
    upload_recovery::reconcile(
        &mut s,
        &row.draft.team_id,
        &row.draft.channel_id,
        Some(&["1900000001.000001".into()]),
    );
    assert!(
        !s.core
            .pending_file_messages
            .iter()
            .any(|p| p.client_msg_id == row.draft.client_msg_id)
    );
    assert!(s.upload_journal.backend.load(ACCOUNT).unwrap().is_empty());
    assert_ne!(s.upload_journal.revision, revision);
    // Even an explicitly replayed stale snapshot encounters canonical evidence,
    // retires without duplicating the delivered row, and never schedules a send.
    restore(&mut s, ACCOUNT, vec![row.clone()]);
    assert!(
        !s.core
            .pending_file_messages
            .iter()
            .any(|p| p.client_msg_id == row.draft.client_msg_id)
    );
}
#[test]
fn canonical_tombstone_and_wrong_sender_never_retire_a_receipt() {
    let mut s = shell();
    let row = receipt(&s, false, false);
    restore(&mut s, ACCOUNT, vec![row.clone()]);
    let message: Message = serde_json::from_value(
        serde_json::json!({"ts":"1900000001.000001","user":"UOTHER","files":[{"id":"F1"}]}),
    )
    .unwrap();
    let bag = s
        .core
        .workspaces
        .get_mut(&row.draft.team_id)
        .unwrap()
        .messages
        .entry(row.draft.channel_id.clone())
        .or_default();
    bag.upsert(message);
    upload_recovery::reconcile(&mut s, &row.draft.team_id, &row.draft.channel_id, None);
    assert_eq!(
        s.upload_journal.backend.load(ACCOUNT).unwrap(),
        vec![row.clone()]
    );
    let bag = s
        .core
        .workspaces
        .get_mut(&row.draft.team_id)
        .unwrap()
        .messages
        .get_mut(&row.draft.channel_id)
        .unwrap();
    bag.record_remote_deletion("1900000001.000001");
    assert!(bag.was_deleted("1900000001.000001"));
    let message:Message=serde_json::from_value(serde_json::json!({"ts":"1900000001.000001","user":row.draft.user_id,"files":[{"id":"F1"}]})).unwrap();
    // A stale response carrying the right sender must still lose to deletion.
    bag.upsert(message);
    upload_recovery::reconcile(&mut s, &row.draft.team_id, &row.draft.channel_id, None);
    assert_eq!(s.upload_journal.backend.load(ACCOUNT).unwrap(), vec![row]);
}
#[test]
fn send_visit_rejects_credential_rotation_but_allows_navigation() {
    let mut s = shell();
    let transport = Arc::new(super_platinum_core::slack::Transport::new("synthetic").unwrap());
    s.core.transport = Some(transport.clone());
    let team = s.core.active_team.clone().unwrap();
    s.core.session = Some(super_platinum_core::config::Session {
        d_cookie: "synthetic".into(),
        workspaces: std::collections::BTreeMap::from([(
            team.clone(),
            super_platinum_core::config::WorkspaceSession {
                team_id: team.clone(),
                enterprise_id: None,
                user_id: s.core.workspaces[&team].self_user_id.clone(),
                name: "Fixture".into(),
                url: "https://example.slack.com".into(),
                token: "synthetic-token".into(),
            },
        )]),
    });
    let visit = super::super::ownership::SendVisit::capture(&s, &transport);
    s.channel_generation = s.channel_generation.wrapping_add(1);
    assert!(visit.current(&s));
    s.core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut(&team)
        .unwrap()
        .token
        .push_str("-rotated");
    assert!(!visit.current(&s));
}

#[test]
fn load_admission_rejects_retirement_newer_load_and_account_replacement() {
    let mut s = shell();
    let owner = Owner::capture(&s, None).unwrap();
    let revision = s.upload_journal.revision;
    let serial = s.upload_journal.load_serial;
    assert!(accepts_load(&s, &owner, revision, serial));
    s.channel_generation += 1;
    assert!(accepts_load(&s, &owner, revision, serial));
    s.upload_journal.revision += 1;
    assert!(!accepts_load(&s, &owner, revision, serial));
    s.upload_journal.revision = revision;
    s.upload_journal.load_serial += 1;
    assert!(!accepts_load(&s, &owner, revision, serial));
    s.upload_journal.load_serial = serial;
    s.core.active_account = Some("00000000-0000-4000-8000-000000000002".into());
    assert!(!accepts_load(&s, &owner, revision, serial));
}
#[test]
fn reservation_without_ids_stays_unconfirmed_and_never_reenters_composer() {
    let mut s = shell();
    let full = receipt(&s, false, false);
    let mut draft = full.draft.clone();
    draft.client_msg_id.push_str("-interrupted");
    draft.optimistic_ts = "1900000002.000001".into();
    let reserved = s.upload_journal.backend.reserve(draft).unwrap().stored;
    restore(&mut s, ACCOUNT, vec![reserved.clone()]);
    let p = s
        .core
        .pending_file_messages
        .iter()
        .find(|p| p.client_msg_id == reserved.draft.client_msg_id)
        .unwrap();
    assert!(p.finalized.is_none());
    assert!(!p.attachments[0].uploading);
    assert!(p.attachments[0].path.as_os_str().is_empty());
    assert!(s.core.composer_attachments.is_empty());
    assert_eq!(s.upload_journal.backend.load(ACCOUNT).unwrap().len(), 2);
}

#[test]
fn repeated_revision_races_eventually_restore_unrelated_older_receipt() {
    let mut s = shell();
    let old = receipt(&s, false, false);
    let owner = Owner::capture(&s, None).unwrap();
    let serial = s.upload_journal.load_serial;
    let first_revision = s.upload_journal.revision;
    let first_rows = s.upload_journal.backend.load(ACCOUNT).unwrap();
    let mut fresh_draft = old.draft.clone();
    fresh_draft.client_msg_id.push_str("-new");
    fresh_draft.optimistic_ts = "1900000003.000001".into();
    let reserved = s
        .upload_journal
        .backend
        .reserve(fresh_draft)
        .unwrap()
        .stored;
    remember(&mut s, reserved.clone());
    assert_eq!(
        apply_load(&mut s, &owner, first_revision, serial, first_rows),
        LoadDecision::Recheck
    );
    assert!(
        !s.core
            .pending_file_messages
            .iter()
            .any(|p| p.client_msg_id == old.draft.client_msg_id)
    );
    let second_revision = s.upload_journal.revision;
    let second_rows = s.upload_journal.backend.load(ACCOUNT).unwrap();
    let finalized = s
        .upload_journal
        .backend
        .finalize(reserved.ticket, vec!["F2".into()])
        .unwrap()
        .unwrap();
    remember(&mut s, finalized);
    assert_eq!(
        apply_load(&mut s, &owner, second_revision, serial, second_rows),
        LoadDecision::Recheck
    );
    assert!(
        !s.core
            .pending_file_messages
            .iter()
            .any(|p| p.client_msg_id == old.draft.client_msg_id)
    );
    let stable_revision = s.upload_journal.revision;
    let stable_rows = s.upload_journal.backend.load(ACCOUNT).unwrap();
    assert_eq!(
        apply_load(&mut s, &owner, stable_revision, serial, stable_rows),
        LoadDecision::Applied
    );
    assert!(
        s.core
            .pending_file_messages
            .iter()
            .any(|p| p.client_msg_id == old.draft.client_msg_id)
    );
    assert!(s.core.composer_attachments.is_empty());
    assert_eq!(s.upload_journal.backend.load(ACCOUNT).unwrap().len(), 2);
}
