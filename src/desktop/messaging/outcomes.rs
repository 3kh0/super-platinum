//! Transient presentation for retained sends. This is not a retry queue.
use super::{SendVisit, ShellState};
use crate::state::PendingDeliveryStatus;

pub(super) fn mark_upload_status(
    shell: &mut ShellState,
    visit: &SendVisit,
    client_msg_id: &str,
    status: PendingDeliveryStatus,
) {
    if !visit.current(shell) {
        return;
    }
    let Some(pending) = shell
        .core
        .pending_file_messages
        .iter_mut()
        .find(|pending| pending.client_msg_id == client_msg_id)
    else {
        return;
    };
    shell
        .pending_delivery
        .insert(pending.message_ts.clone(), status);
    for attachment in &mut pending.attachments {
        attachment.uploading = false;
        attachment.upload_started = None;
        attachment.upload_cancel = None;
        attachment.upload_progress = None;
    }
}

/// Named offline fixture: use the same text-result and upload-preservation
/// helpers as production, with transport construction but no network calls.
pub(crate) fn fixture(shell: &mut ShellState, name: &str) {
    use std::sync::Arc;
    use super_platinum_core::{
        ComposerState,
        domain::{ComposerAttachment, PendingFileMessage},
        slack::{Error, Transport},
    };
    let transport = Arc::new(Transport::new("offline-fixture").expect("fixture transport"));
    shell.core.transport = Some(transport.clone());
    shell.core.active_team = Some("T1".into());
    shell.core.active_channel = Some("C2".into());
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .insert("C2".into(), Default::default());
    shell.sync_composer_drafts();
    let visit = SendVisit::capture(shell, &transport);
    for (text, error) in [
        (
            "This message was rejected. Its text is still available here.",
            Error::Api("not_in_channel".into()),
        ),
        (
            "The connection ended before delivery could be confirmed.",
            Error::Offline("fixture timeout".into()),
        ),
    ] {
        shell.core.composer = ComposerState::with_text(text);
        let pending = shell.queue_composer().expect("fixture text queued");
        super::ownership::apply_text_result(shell, &visit, &pending, "Message", Err(error));
    }
    for (index, text, filename, status) in [
        (
            1,
            "This upload was rejected. A newer draft stays in the composer.",
            "release-notes.txt",
            PendingDeliveryStatus::Failed,
        ),
        (
            2,
            "These files may have arrived. Check the conversation before uploading again.",
            "roadmap.pdf",
            PendingDeliveryStatus::Unconfirmed,
        ),
    ] {
        shell.core.composer = ComposerState::with_text(text);
        let pending = shell.queue_composer().expect("fixture upload row queued");
        shell.core.pending_file_messages.push(PendingFileMessage {
            team: pending.team,
            channel: pending.channel,
            thread_ts: None,
            message_ts: pending.optimistic_ts,
            client_msg_id: pending.client_msg_id.clone(),
            text: text.into(),
            attachments: vec![ComposerAttachment {
                id: index,
                path: format!("/offline-fixture/{filename}").into(),
                name: filename.into(),
                bytes: 2048,
                uploading: true,
                upload_started: Some(std::time::Instant::now()),
                upload_cancel: None,
                upload_progress: None,
                preview_path: None,
            }],
        });
        if status == PendingDeliveryStatus::Failed {
            shell.core.composer = ComposerState::with_text("A newer draft stays here.");
            assert!(super::restore_upload_to_shell(
                shell,
                &visit,
                &pending.client_msg_id,
                None
            ));
        } else {
            mark_upload_status(shell, &visit, &pending.client_msg_id, status);
        }
    }
    shell.core.composer = ComposerState::with_text("A newer draft stays here.");
    shell.toast = None;
    if name == "send-outcomes-thread" {
        let root = "1719800000.000001".to_owned();
        let mut replies = shell
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .messages
            .remove("C2")
            .unwrap();
        for message in &mut replies.messages {
            message.thread_ts = Some(root.clone());
        }
        for pending in &mut shell.core.pending_file_messages {
            pending.thread_ts = Some(root.clone());
        }
        let root_message = super_platinum_core::slack::models::Message {
            ts: Some(root.clone()),
            user: Some("U0".into()),
            text: Some("Delivery status review".into()),
            channel: Some("C2".into()),
            ..Default::default()
        };
        replies.upsert(root_message.clone());
        shell
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .messages
            .entry("C2".into())
            .or_default()
            .upsert(root_message);
        shell
            .core
            .threads
            .insert(("T1".into(), "C2".into(), root.clone()), replies);
        shell.core.composer = ComposerState::default();
        shell.begin_thread(root, None);
        shell.core.thread_composer = ComposerState::with_text("A newer draft stays here.");
    }
    shell.refresh_from_core();
    shell.core.transport = None;
}

#[cfg(test)]
#[path = "outcomes_tests.rs"]
mod tests;
