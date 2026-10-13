use super::*;
use crate::{media::MediaRegistry, state::PendingDeliveryStatus};
use super_platinum_core::domain::PendingFileMessage;

fn fixture(thread: bool, broadcast: bool) -> (ShellState, PendingFileMessage, Message) {
    let mut shell = ShellState::fixture(MediaRegistry::default());
    shell.core.active_account = Some("account-one".into());
    let team = shell.core.active_team.clone().unwrap();
    let channel = shell.core.active_channel.clone().unwrap();
    let sender = shell.core.workspaces[&team].self_user_id.clone();
    let pending = PendingFileMessage {
        finalized: None,
        team,
        channel,
        thread_ts: thread.then(|| "100.000001".into()),
        reply_broadcast: broadcast,
        message_ts: "pending-upload".into(),
        client_msg_id: "upload-one".into(),
        text: "preserved caption".into(),
        attachments: vec![],
    };
    let optimistic = Message {
        ts: Some(pending.message_ts.clone()),
        text: Some(pending.text.clone()),
        ..Default::default()
    };
    if let Some(root) = &pending.thread_ts {
        let bag = shell
            .core
            .threads
            .entry((pending.team.clone(), pending.channel.clone(), root.clone()))
            .or_default();
        bag.upsert(optimistic);
        bag.pending.push(pending.message_ts.clone());
    } else {
        let bag = shell
            .core
            .workspaces
            .get_mut(&pending.team)
            .unwrap()
            .messages
            .entry(pending.channel.clone())
            .or_default();
        bag.upsert(optimistic);
        bag.pending.push(pending.message_ts.clone());
    }
    shell.core.pending_file_messages.push(pending.clone());
    shell.pending_delivery.insert(
        pending.message_ts.clone(),
        PendingDeliveryStatus::Unconfirmed,
    );
    let canonical = serde_json::from_value(serde_json::json!({
        "ts":"9999999999.123456", "user":sender,
        "thread_ts":pending.thread_ts, "files":[{"id":"F1"}],
        "subtype":if broadcast {Some("thread_broadcast")} else {None},
        "text":"canonical caption"
    }))
    .unwrap();
    (shell, pending, canonical)
}
fn ingest(shell: &mut ShellState, pending: &PendingFileMessage, message: Message) {
    if let Some(root) = &pending.thread_ts {
        shell
            .core
            .threads
            .get_mut(&(pending.team.clone(), pending.channel.clone(), root.clone()))
            .unwrap()
            .upsert(message);
    } else {
        shell
            .core
            .workspaces
            .get_mut(&pending.team)
            .unwrap()
            .messages
            .get_mut(&pending.channel)
            .unwrap()
            .upsert(message);
    }
}
fn attach_receipt(shell: &mut ShellState, pending: &PendingFileMessage) -> bool {
    let sender = shell.core.workspaces[&pending.team].self_user_id.clone();
    attach(shell, &pending.client_msg_id, &sender, vec!["F1".into()])
}
fn recover(shell: &mut ShellState, pending: &PendingFileMessage, message: &Message) {
    let ids = incoming(
        shell,
        &pending.team,
        &pending.channel,
        std::slice::from_ref(message),
    );
    reconcile(shell, &pending.team, &pending.channel, Some(&ids));
}
#[test]
fn delayed_canonical_ingest_and_realtime_before_receipt_retire_only_exact_upload() {
    for thread in [false, true] {
        for before in [false, true] {
            let (mut shell, pending, message) = fixture(thread, thread);
            // A second unproven upload is never inferred to have arrived.
            let mut unrelated = pending.clone();
            unrelated.client_msg_id = "upload-two".into();
            unrelated.message_ts = "pending-two".into();
            shell.core.pending_file_messages.push(unrelated);
            if before {
                ingest(&mut shell, &pending, message.clone());
            }
            assert_eq!(attach_receipt(&mut shell, &pending), before);
            if !before {
                assert_eq!(shell.core.pending_file_messages.len(), 2);
                shell.core.active_channel = Some("navigated-away".into());
                ingest(&mut shell, &pending, message.clone());
                recover(&mut shell, &pending, &message);
            }
            assert_eq!(shell.core.pending_file_messages.len(), 1);
            assert_eq!(
                shell.core.pending_file_messages[0].client_msg_id,
                "upload-two"
            );
            assert!(!shell.pending_delivery.contains_key(&pending.message_ts));
            let bag = &shell.core.workspaces[&pending.team].messages[&pending.channel];
            assert!(lookup(bag, message.ts.as_deref().unwrap()).is_some());
            assert!(bag.pending.iter().all(|id| id != &pending.message_ts));
        }
    }
}
#[test]
fn account_sender_destination_and_tombstones_cannot_retire_receipt() {
    for case in [
        "account",
        "sender",
        "channel",
        "thread",
        "files",
        "channel-deleted",
        "thread-deleted",
    ] {
        let (mut shell, pending, mut message) = fixture(true, true);
        assert!(!attach_receipt(&mut shell, &pending));
        match case {
            "account" => shell.core.active_account = Some("different-account".into()),
            "sender" => message.user = Some("different-user".into()),
            "channel" => message.channel = Some("different-channel".into()),
            "thread" => message.thread_ts = Some("different-thread".into()),
            "files" => message.files[0].id = Some("different-file".into()),
            "channel-deleted" => shell
                .core
                .workspaces
                .get_mut(&pending.team)
                .unwrap()
                .messages
                .entry(pending.channel.clone())
                .or_default()
                .record_remote_deletion(message.ts.as_deref().unwrap()),
            "thread-deleted" => shell
                .core
                .threads
                .get_mut(&(
                    pending.team.clone(),
                    pending.channel.clone(),
                    pending.thread_ts.clone().unwrap(),
                ))
                .unwrap()
                .record_remote_deletion(message.ts.as_deref().unwrap()),
            _ => unreachable!(),
        }
        ingest(&mut shell, &pending, message.clone());
        recover(&mut shell, &pending, &message);
        assert_eq!(shell.core.pending_file_messages.len(), 1, "{case}");
        assert_eq!(
            shell.core.pending_file_messages[0].text,
            "preserved caption"
        );
        assert!(
            shell.pending_delivery.contains_key(&pending.message_ts),
            "{case}"
        );
    }
}
#[test]
fn indexed_ingest_only_checks_supplied_ids_and_preserves_newer_broadcast_copy() {
    for subtype in ["thread_broadcast", "reply_broadcast"] {
        let (mut shell, pending, mut message) = fixture(true, true);
        message.subtype = Some(subtype.into());
        assert!(!attach_receipt(&mut shell, &pending));
        ingest(&mut shell, &pending, message.clone());
        let mut newer = message.clone();
        newer.text = Some("newer channel edit".into());
        shell
            .core
            .workspaces
            .get_mut(&pending.team)
            .unwrap()
            .messages
            .entry(pending.channel.clone())
            .or_default()
            .upsert(newer);
        reconcile(
            &mut shell,
            &pending.team,
            &pending.channel,
            Some(&["1.000001".into()]),
        );
        assert_eq!(shell.core.pending_file_messages.len(), 1);
        recover(&mut shell, &pending, &message);
        assert!(shell.core.pending_file_messages.is_empty());
        assert_eq!(
            lookup(
                &shell.core.workspaces[&pending.team].messages[&pending.channel],
                message.ts.as_deref().unwrap()
            )
            .unwrap()
            .text
            .as_deref(),
            Some("newer channel edit")
        );
    }
}
#[test]
fn realtime_admission_rejects_retired_socket_and_conflicting_receipt() {
    let (mut shell, pending, mut message) = fixture(false, false);
    assert!(!attach_receipt(&mut shell, &pending));
    let sender = shell.core.workspaces[&pending.team].self_user_id.clone();
    assert!(!attach(
        &mut shell,
        &pending.client_msg_id,
        &sender,
        vec!["F2".into()]
    ));
    assert_eq!(
        shell.core.pending_file_messages[0]
            .finalized
            .as_ref()
            .unwrap()
            .file_ids,
        vec!["F1"]
    );
    shell
        .core
        .workspaces
        .get_mut(&pending.team)
        .unwrap()
        .rt_generation = 10;
    message.channel = Some(pending.channel.clone());
    let update = RtEvent::Message(message.clone());
    assert!(event(&shell, &pending.team, 9, &update).is_none());
    assert_eq!(
        event(&shell, &pending.team, 10, &update),
        Some((pending.channel, message.ts.unwrap()))
    );
}
