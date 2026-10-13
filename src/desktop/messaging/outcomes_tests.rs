use super::*;
use crate::{media::MediaRegistry, state::PendingSend};
use std::sync::Arc;
use super_platinum_core::slack::{Error, Transport, models::SentMessage};

fn ready() -> (ShellState, SendVisit) {
    let mut shell = ShellState::fixture(MediaRegistry::default());
    fixture(&mut shell, "send-outcomes");
    let transport = Arc::new(Transport::new("offline-test").unwrap());
    shell.core.transport = Some(transport.clone());
    let visit = SendVisit::capture(&shell, &transport);
    (shell, visit)
}

#[test]
fn named_fixture_has_stopped_text_and_upload_labels_and_preserves_newer_input() {
    let (shell, _) = ready();
    assert_eq!(shell.pending_delivery.len(), 4);
    assert_eq!(
        shell
            .pending_delivery
            .values()
            .filter(|status| **status == PendingDeliveryStatus::Failed)
            .count(),
        2
    );
    assert_eq!(
        shell
            .pending_delivery
            .values()
            .filter(|status| **status == PendingDeliveryStatus::Unconfirmed)
            .count(),
        2
    );
    for message in shell.messages.iter().filter(|message| message.pending) {
        assert!(matches!(
            shell.pending_delivery_label(&message.id),
            "Not sent" | "Delivery unconfirmed"
        ));
    }
    assert_eq!(shell.core.pending_file_messages.len(), 2);
    assert!(
        shell
            .core
            .pending_file_messages
            .iter()
            .flat_map(|pending| &pending.attachments)
            .all(|attachment| !attachment.uploading)
    );
    assert_eq!(shell.core.composer.text, "A newer draft stays here.");
}

#[test]
fn stale_account_or_epoch_callbacks_do_not_replace_delivery_labels() {
    for epoch_change in [false, true] {
        let (mut shell, visit) = ready();
        let first = shell.core.workspaces["T1"].messages["C2"].messages[0].clone();
        let pending = PendingSend {
            reply_broadcast: false,
            account: shell.core.active_account.clone(),
            account_epoch: shell.core.account_epoch,
            team: "T1".into(),
            channel: "C2".into(),
            thread_ts: None,
            text: first.text.unwrap(),
            client_msg_id: first.client_msg_id.unwrap(),
            optimistic_ts: first.ts.unwrap(),
        };
        let upload_id = shell.core.pending_file_messages[0].client_msg_id.clone();
        let before = shell.pending_delivery.clone();
        if epoch_change {
            shell.core.account_epoch += 1;
        } else {
            shell.core.active_account = Some("different-account".into());
        }
        super::super::ownership::apply_text_result(
            &mut shell,
            &visit,
            &pending,
            "Message",
            Err(Error::Offline("timeout".into())),
        );
        mark_upload_status(
            &mut shell,
            &visit,
            &upload_id,
            PendingDeliveryStatus::Unconfirmed,
        );
        assert_eq!(shell.pending_delivery, before);
    }
}

#[test]
fn positive_confirmation_and_realtime_removal_clear_transient_status_only() {
    let (mut shell, visit) = ready();
    let first = shell.core.workspaces["T1"].messages["C2"].messages[0].clone();
    let pending = PendingSend {
        reply_broadcast: false,
        account: shell.core.active_account.clone(),
        account_epoch: shell.core.account_epoch,
        team: "T1".into(),
        channel: "C2".into(),
        thread_ts: None,
        text: first.text.unwrap(),
        client_msg_id: first.client_msg_id.unwrap(),
        optimistic_ts: first.ts.unwrap(),
    };
    assert!(super::super::ownership::apply_text_result(
        &mut shell,
        &visit,
        &pending,
        "Message",
        Ok(SentMessage {
            channel: pending.channel.clone(),
            ts: "1900000000.000001".into(),
            ..Default::default()
        })
    ));
    assert!(!shell.pending_delivery.contains_key(&pending.optimistic_ts));
    assert_eq!(shell.pending_delivery.len(), 3);
    let removed = shell.core.pending_file_messages[0].message_ts.clone();
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .get_mut("C2")
        .unwrap()
        .remove(&removed);
    shell.refresh_from_core();
    assert!(!shell.pending_delivery.contains_key(&removed));
    assert_eq!(shell.pending_delivery.len(), 2);
}

#[test]
fn fresh_account_shell_has_no_previous_delivery_status() {
    assert!(
        ShellState::fixture(MediaRegistry::default())
            .pending_delivery
            .is_empty()
    );
    assert!(
        ShellState::opening(MediaRegistry::default())
            .pending_delivery
            .is_empty()
    );
}

#[test]
fn thread_fixture_keeps_all_four_statuses_on_retained_reply_rows() {
    let mut shell = ShellState::fixture(MediaRegistry::default());
    fixture(&mut shell, "send-outcomes-thread");
    assert_eq!(shell.pending_delivery.len(), 4);
    assert_eq!(
        shell
            .thread_messages
            .iter()
            .filter(|message| message.pending)
            .count(),
        4
    );
    assert_eq!(shell.core.thread_composer.text, "A newer draft stays here.");
    assert!(
        shell
            .core
            .pending_file_messages
            .iter()
            .all(|pending| pending.thread_ts == shell.thread_root)
    );
}
