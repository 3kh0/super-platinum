use super::*;
use crate::media::MediaRegistry;
use super_platinum_core::slack::models::{HistoryPage, Message};

fn queued(in_thread: bool) -> (ShellState, SendVisit, PendingSend) {
    let mut shell = ShellState::fixture(MediaRegistry::default());
    shell.core.active_account = Some("original-account".into());
    shell.core.account_epoch = 17;
    let transport = Arc::new(Transport::new("offline-test-cookie").unwrap());
    shell.core.transport = Some(transport.clone());
    let pending = if in_thread {
        shell.begin_thread("1.0".into(), None);
        shell.core.thread_composer = super_platinum_core::ComposerState::with_text("private reply");
        shell.queue_thread_composer().unwrap()
    } else {
        shell.core.composer = super_platinum_core::ComposerState::with_text("private message");
        shell.queue_composer().unwrap()
    };
    let visit = SendVisit::capture(&shell, &transport);
    (shell, visit, pending)
}

fn bag<'a>(
    shell: &'a ShellState,
    pending: &PendingSend,
) -> &'a super_platinum_core::state::ChannelMessages {
    if let Some(root) = &pending.thread_ts {
        &shell.core.threads[&(pending.team.clone(), pending.channel.clone(), root.clone())]
    } else {
        &shell.core.workspaces[&pending.team].messages[&pending.channel]
    }
}

fn sent(pending: &PendingSend) -> SentMessage {
    SentMessage {
        channel: pending.channel.clone(),
        ts: "9999999999.000001".into(),
        message: Message {
            text: Some("server response".into()),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[test]
fn queued_owner_rejects_other_account_even_with_identical_conversation_ids() {
    for thread in [false, true] {
        let (mut shell, _, pending) = queued(thread);
        assert_eq!(pending.account.as_deref(), Some("original-account"));
        assert_eq!(pending.account_epoch, 17);
        assert!(SendVisit::queued_is_current(&shell, &pending));
        shell.core.active_account = Some("other-account".into());
        assert!(!SendVisit::queued_is_current(&shell, &pending));
        shell.core.active_account = pending.account.clone();
        shell.core.account_epoch += 1;
        assert!(!SendVisit::queued_is_current(&shell, &pending));
    }
}

#[test]
fn switched_account_success_and_rejection_leave_current_ui_and_pending_untouched() {
    for thread in [false, true] {
        for success in [false, true] {
            let (mut shell, visit, pending) = queued(thread);
            shell.core.active_account = Some("other-account".into());
            shell.toast = None;
            let before = format!("{:?}", bag(&shell, &pending));
            let result = if success {
                Ok(sent(&pending))
            } else {
                Err(Error::Api("not_in_channel".into()))
            };
            assert!(!apply_text_result(
                &mut shell, &visit, &pending, "Message", result
            ));
            assert_eq!(format!("{:?}", bag(&shell, &pending)), before);
            assert!(shell.toast.is_none());
        }
    }
}

#[test]
fn returning_to_same_account_or_replacing_transport_rejects_old_success() {
    for epoch_change in [false, true] {
        let (mut shell, visit, pending) = queued(false);
        if epoch_change {
            shell.core.account_epoch += 1;
        } else {
            shell.core.transport = Some(Arc::new(Transport::new("new-cookie").unwrap()));
        }
        assert!(!apply_text_result(
            &mut shell,
            &visit,
            &pending,
            "Message",
            Ok(sent(&pending))
        ));
        assert!(
            bag(&shell, &pending)
                .pending
                .contains(&pending.optimistic_ts)
        );
    }
}

#[test]
fn current_success_confirms_original_channel_or_thread_after_navigation() {
    for thread in [false, true] {
        let (mut shell, visit, pending) = queued(thread);
        shell.core.active_channel = Some("unrelated-channel".into());
        assert!(apply_text_result(
            &mut shell,
            &visit,
            &pending,
            "Message",
            Ok(sent(&pending))
        ));
        assert!(
            !bag(&shell, &pending)
                .pending
                .contains(&pending.optimistic_ts)
        );
        assert!(
            bag(&shell, &pending)
                .messages
                .iter()
                .any(|message| message.ts.as_deref() == Some("9999999999.000001"))
        );
    }
}

#[test]
fn malformed_or_contradictory_positive_text_receipt_retains_original_payload() {
    for thread in [false, true] {
        for case in 0..13 {
            let (mut shell, visit, mut pending) = queued(thread);
            let mut receipt = sent(&pending);
            match case {
                0 => receipt.ts.clear(),
                1 => receipt.ts = "1".into(),
                2 => receipt.ts = "1.".into(),
                3 => receipt.ts = "1.1234567".into(),
                4 => receipt.ts = "0.123456".into(),
                5 => receipt.ts = "+1.123456".into(),
                6 => receipt.ts = "1.12345x".into(),
                7 => receipt.channel.clear(),
                8 => receipt.channel = "other-channel".into(),
                9 => receipt.message.channel = Some("other-channel".into()),
                10 => receipt.message.ts = Some("1234567890.123456".into()),
                11 => receipt.message.thread_ts = Some("other-thread".into()),
                12 => {
                    if thread {
                        receipt.ts = "1234567890.123456".into();
                        // A reply cannot be its requested parent message.
                        pending.thread_ts = Some(receipt.ts.clone());
                        assert!(!confirms_text(&receipt, &pending));
                        continue;
                    }
                    receipt.message.thread_ts = Some(String::new());
                }
                _ => unreachable!(),
            }
            let before = format!("{:?}", bag(&shell, &pending));
            assert!(
                !apply_text_result(&mut shell, &visit, &pending, "Message", Ok(receipt)),
                "accepted case {case}, thread={thread}"
            );
            assert_eq!(format!("{:?}", bag(&shell, &pending)), before);
            assert_eq!(
                shell.pending_delivery.get(&pending.optimistic_ts),
                Some(&crate::state::PendingDeliveryStatus::Unconfirmed)
            );
        }
    }
}

#[test]
fn matching_explicit_text_receipt_accepts_server_generated_client_id() {
    for thread in [false, true] {
        let (mut shell, visit, pending) = queued(thread);
        let mut receipt = sent(&pending);
        receipt.message.ts = Some(receipt.ts.clone());
        receipt.message.channel = Some(pending.channel.clone());
        receipt.message.thread_ts = Some(
            pending
                .thread_ts
                .clone()
                .unwrap_or_else(|| receipt.ts.clone()),
        );
        // chat.postMessage currently does not transmit the optimistic UUID.
        receipt.message.client_msg_id = Some("server-generated-id".into());
        assert!(apply_text_result(
            &mut shell,
            &visit,
            &pending,
            "Message",
            Ok(receipt)
        ));
        assert!(
            !bag(&shell, &pending)
                .pending
                .contains(&pending.optimistic_ts)
        );
    }
}

#[test]
fn uncertain_and_rejected_send_retain_original_payload_without_replay() {
    for thread in [false, true] {
        for error in [
            Error::Offline("timeout".into()),
            Error::Transport("decode chat.postMessage".into()),
            Error::Api("internal_error".into()),
            Error::Api("fatal_error".into()),
            Error::HttpStatus {
                status: 503,
                retry_after_secs: None,
            },
            Error::HttpStatus {
                status: 408,
                retry_after_secs: None,
            },
        ] {
            let (mut shell, visit, pending) = queued(thread);
            assert!(!apply_text_result(
                &mut shell,
                &visit,
                &pending,
                "Message",
                Err(error)
            ));
            assert!(
                bag(&shell, &pending)
                    .pending
                    .contains(&pending.optimistic_ts)
            );
            assert!(
                bag(&shell, &pending)
                    .messages
                    .iter()
                    .any(|message| message.text.as_deref() == Some(pending.text.as_str()))
            );
        }
        let (mut shell, visit, pending) = queued(thread);
        assert!(!apply_text_result(
            &mut shell,
            &visit,
            &pending,
            "Message",
            Err(Error::Api("not_in_channel".into()))
        ));
        assert!(
            bag(&shell, &pending)
                .pending
                .contains(&pending.optimistic_ts)
        );
        assert!(
            bag(&shell, &pending)
                .messages
                .iter()
                .any(|message| message.text.as_deref() == Some(pending.text.as_str()))
        );
    }
}

#[test]
fn upload_history_cannot_merge_or_report_errors_into_another_account_visit() {
    for epoch_change in [false, true] {
        let (mut shell, visit, pending) = queued(false);
        if epoch_change {
            shell.core.account_epoch += 1;
        } else {
            shell.core.active_account = Some("other-account".into());
        }
        shell.toast = None;
        let before = format!("{:?}", bag(&shell, &pending));
        let page = HistoryPage {
            messages: vec![sent(&pending).message],
            ..Default::default()
        };
        let expected = UploadReadback {
            team: pending.team.clone(),
            channel: pending.channel.clone(),
            thread_ts: None,
            sender: shell.core.workspaces[&pending.team].self_user_id.clone(),
            file_ids: vec!["F1".into()],
        };
        assert!(!apply_upload_history(
            &mut shell,
            &visit,
            &expected,
            Ok(page)
        ));
        assert!(!apply_upload_history(
            &mut shell,
            &visit,
            &expected,
            Err(Error::Api("invalid_auth".into()))
        ));
        assert_eq!(format!("{:?}", bag(&shell, &pending)), before);
        assert!(shell.toast.is_none());
    }
}

#[test]
fn upload_readback_requires_exact_files_sender_and_destination() {
    for thread in [false, true] {
        let (mut shell, visit, pending) = queued(thread);
        let expected = UploadReadback {
            team: pending.team.clone(),
            channel: pending.channel.clone(),
            thread_ts: pending.thread_ts.clone(),
            sender: shell.core.workspaces[&pending.team].self_user_id.clone(),
            file_ids: vec!["F1".into(), "F2".into()],
        };
        let message: Message = serde_json::from_value(serde_json::json!({
            "ts":"9999999999.123456", "user":expected.sender, "channel":expected.channel,
            "thread_ts":expected.thread_ts, "files":[{"id":"F1"},{"id":"F2"}]
        }))
        .unwrap();
        assert!(expected.confirms(&message));
        let mut bad = message.clone();
        bad.user = Some("other-user".into());
        assert!(!expected.confirms(&bad));
        let mut bad = message.clone();
        bad.channel = Some("other-channel".into());
        assert!(!expected.confirms(&bad));
        let mut bad = message.clone();
        bad.thread_ts = Some("other-root".into());
        assert!(!expected.confirms(&bad));
        let mut bad = message.clone();
        bad.ts = Some("invalid".into());
        assert!(!expected.confirms(&bad));
        let mut bad = message.clone();
        bad.files.pop();
        assert!(!expected.confirms(&bad));
        let mut bad = message.clone();
        bad.files[1].id = Some("F3".into());
        assert!(!expected.confirms(&bad));
        assert!(!apply_upload_history(
            &mut shell,
            &visit,
            &expected,
            Ok(HistoryPage::default())
        ));
        assert!(
            bag(&shell, &pending)
                .pending
                .contains(&pending.optimistic_ts)
        );
        assert!(apply_upload_history(
            &mut shell,
            &visit,
            &expected,
            Ok(HistoryPage {
                messages: vec![message],
                ..Default::default()
            })
        ));
        assert!(
            bag(&shell, &pending)
                .messages
                .iter()
                .any(|message| message.ts.as_deref() == Some("9999999999.123456"))
        );
        if thread {
            assert!(
                !shell.core.workspaces[&pending.team].messages[&pending.channel]
                    .messages
                    .iter()
                    .any(|message| message.ts.as_deref() == Some("9999999999.123456"))
            );
        }
        let mut wrong_team = expected;
        wrong_team.team = "other-team".into();
        assert!(!apply_upload_history(
            &mut shell,
            &visit,
            &wrong_team,
            Ok(HistoryPage::default())
        ));
    }
}

#[test]
fn upload_readback_preserves_newer_rows_deletions_and_navigation_paging_state() {
    for thread in [false, true] {
        let (mut shell, visit, pending) = queued(thread);
        let expected = UploadReadback {
            team: pending.team.clone(),
            channel: pending.channel.clone(),
            thread_ts: pending.thread_ts.clone(),
            sender: shell.core.workspaces[&pending.team].self_user_id.clone(),
            file_ids: vec!["F1".into()],
        };
        let uploaded: Message = serde_json::from_value(serde_json::json!({
            "ts":"9999999999.123456", "user":expected.sender, "channel":expected.channel,
            "thread_ts":expected.thread_ts, "files":[{"id":"F1"}], "text":"old caption"
        }))
        .unwrap();
        let mut unrelated_edited = uploaded.clone();
        unrelated_edited.ts = Some("9999999998.123456".into());
        unrelated_edited.files.clear();
        unrelated_edited.text = Some("old unrelated text".into());
        let mut unrelated_deleted = unrelated_edited.clone();
        unrelated_deleted.ts = Some("9999999997.123456".into());
        let messages = if let Some(root) = &pending.thread_ts {
            shell
                .core
                .threads
                .get_mut(&(pending.team.clone(), pending.channel.clone(), root.clone()))
                .unwrap()
        } else {
            shell
                .core
                .workspaces
                .get_mut(&pending.team)
                .unwrap()
                .messages
                .get_mut(&pending.channel)
                .unwrap()
        };
        messages.upsert(unrelated_deleted.clone());
        messages.remove(unrelated_deleted.ts.as_deref().unwrap());
        let mut newer_unrelated = unrelated_edited.clone();
        newer_unrelated.text = Some("newer realtime edit".into());
        messages.upsert(newer_unrelated);
        let mut newer_uploaded = uploaded.clone();
        newer_uploaded.text = Some("newer uploaded caption".into());
        messages.upsert(newer_uploaded);
        messages.loaded = true;
        messages.history_refreshing = true;
        messages.history_loading_older = true;
        messages.has_more_older = true;
        messages.history_failed = true;
        messages.last_read = Some("9999999996.123456".into());
        let before = format!("{:?}", bag(&shell, &pending));

        assert!(apply_upload_history(
            &mut shell,
            &visit,
            &expected,
            Ok(HistoryPage {
                messages: vec![unrelated_edited, unrelated_deleted, uploaded],
                ..Default::default()
            })
        ));
        // The matching upload proves delivery, but none of the older page
        // payloads can replace current rows or resurrect unrelated deletions.
        assert_eq!(format!("{:?}", bag(&shell, &pending)), before);
    }
}

#[test]
fn upload_readback_cannot_confirm_or_resurrect_a_remotely_deleted_matching_row() {
    for thread in [false, true] {
        for row_was_loaded in [false, true] {
            let (mut shell, visit, pending) = queued(thread);
            let expected = UploadReadback {
                team: pending.team.clone(),
                channel: pending.channel.clone(),
                thread_ts: pending.thread_ts.clone(),
                sender: shell.core.workspaces[&pending.team].self_user_id.clone(),
                file_ids: vec!["F1".into()],
            };
            let uploaded: Message = serde_json::from_value(serde_json::json!({
                "ts":"9999999999.123456", "user":expected.sender, "channel":expected.channel,
                "thread_ts":expected.thread_ts, "files":[{"id":"F1"}], "text":"caption"
            }))
            .unwrap();
            let ts = uploaded.ts.as_deref().unwrap();
            // The channel bag holds authoritative deletion evidence even when
            // the deleted row was an unloaded reply and had never appeared.
            let channel = shell
                .core
                .workspaces
                .get_mut(&pending.team)
                .unwrap()
                .messages
                .entry(pending.channel.clone())
                .or_default();
            if row_was_loaded {
                channel.upsert(uploaded.clone());
            }
            channel.record_remote_deletion(ts);
            if let Some(root) = &pending.thread_ts {
                let replies = shell
                    .core
                    .threads
                    .get_mut(&(pending.team.clone(), pending.channel.clone(), root.clone()))
                    .unwrap();
                if row_was_loaded {
                    replies.upsert(uploaded.clone());
                }
                replies.record_remote_deletion(ts);
            }
            let before = format!("{:?}", bag(&shell, &pending));
            assert!(!apply_upload_history(
                &mut shell,
                &visit,
                &expected,
                Ok(HistoryPage {
                    messages: vec![uploaded.clone()],
                    ..Default::default()
                })
            ));
            assert_eq!(format!("{:?}", bag(&shell, &pending)), before);
            assert!(
                bag(&shell, &pending)
                    .pending
                    .contains(&pending.optimistic_ts)
            );

            // The old readback still cannot modify the same IDs after an
            // account visit changes, even when its result carries a tombstone.
            shell.core.account_epoch += 1;
            shell.toast = None;
            assert!(!apply_upload_history(
                &mut shell,
                &visit,
                &expected,
                Ok(HistoryPage {
                    messages: vec![uploaded],
                    ..Default::default()
                })
            ));
            assert_eq!(format!("{:?}", bag(&shell, &pending)), before);
            assert!(shell.toast.is_none());
        }
    }
}

#[test]
fn stale_upload_failure_cannot_restore_private_caption_or_remove_new_account_pending() {
    for thread in [false, true] {
        let (mut shell, visit, pending) = queued(thread);
        shell
            .core
            .pending_file_messages
            .push(super_platinum_core::domain::PendingFileMessage {
                team: pending.team.clone(),
                channel: pending.channel.clone(),
                thread_ts: pending.thread_ts.clone(),
                message_ts: pending.optimistic_ts.clone(),
                client_msg_id: pending.client_msg_id.clone(),
                text: "private caption".into(),
                attachments: Vec::new(),
            });
        shell.core.active_account = Some("other-account".into());
        super::super::restore_upload_to_shell(
            &mut shell,
            &visit,
            &pending.client_msg_id,
            pending.thread_ts.as_ref(),
        );
        assert_eq!(shell.core.pending_file_messages.len(), 1);
        assert!(
            bag(&shell, &pending)
                .pending
                .contains(&pending.optimistic_ts)
        );
        assert!(shell.core.composer.text.is_empty());
        assert!(shell.core.thread_composer.text.is_empty());
    }
}

#[test]
fn failed_upload_preserves_original_payload_when_newer_draft_exists() {
    for thread in [false, true] {
        let (mut shell, visit, pending) = queued(thread);
        let attachment = super_platinum_core::domain::ComposerAttachment {
            id: 77,
            path: "fixture-file.txt".into(),
            name: "fixture-file.txt".into(),
            bytes: 12,
            uploading: true,
            upload_started: Some(std::time::Instant::now()),
            upload_cancel: Some(Arc::new(std::sync::atomic::AtomicBool::new(false))),
            upload_progress: Some(Arc::new(std::sync::atomic::AtomicU64::new(3))),
            preview_path: None,
        };
        shell
            .core
            .pending_file_messages
            .push(super_platinum_core::domain::PendingFileMessage {
                team: pending.team.clone(),
                channel: pending.channel.clone(),
                thread_ts: pending.thread_ts.clone(),
                message_ts: pending.optimistic_ts.clone(),
                client_msg_id: pending.client_msg_id.clone(),
                text: pending.text.clone(),
                attachments: vec![attachment],
            });
        if thread {
            shell.core.thread_composer = super_platinum_core::ComposerState::with_text("New reply");
        } else {
            shell.core.composer = super_platinum_core::ComposerState::with_text("New message");
        }
        assert!(super::super::restore_upload_to_shell(
            &mut shell,
            &visit,
            &pending.client_msg_id,
            pending.thread_ts.as_ref()
        ));
        let saved = &shell.core.pending_file_messages[0];
        assert_eq!(saved.text, pending.text);
        assert_eq!(saved.attachments.len(), 1);
        assert!(!saved.attachments[0].uploading);
        assert!(saved.attachments[0].upload_progress.is_none());
        assert!(
            bag(&shell, &pending)
                .pending
                .contains(&pending.optimistic_ts)
        );
        assert_eq!(
            if thread {
                &shell.core.thread_composer.text
            } else {
                &shell.core.composer.text
            },
            if thread { "New reply" } else { "New message" }
        );
        assert!(shell.core.composer_attachments.is_empty());
        assert!(shell.core.thread_composer_attachments.is_empty());
    }
}

#[test]
fn failed_upload_after_navigation_restores_original_empty_draft() {
    let (mut shell, visit, pending) = queued(false);
    shell
        .core
        .pending_file_messages
        .push(super_platinum_core::domain::PendingFileMessage {
            team: pending.team.clone(),
            channel: pending.channel.clone(),
            thread_ts: None,
            message_ts: pending.optimistic_ts.clone(),
            client_msg_id: pending.client_msg_id.clone(),
            text: pending.text.clone(),
            attachments: vec![],
        });
    shell.core.active_channel = Some("different-channel".into());
    assert!(!super::super::restore_upload_to_shell(
        &mut shell,
        &visit,
        &pending.client_msg_id,
        None
    ));
    let key = super_platinum_core::drafts::DraftKey {
        team: pending.team.clone(),
        channel: pending.channel.clone(),
        thread: None,
    };
    assert_eq!(shell.core.drafts.items[&key].composer.text, pending.text);
    assert!(shell.core.pending_file_messages.is_empty());
    assert!(
        !bag(&shell, &pending)
            .pending
            .contains(&pending.optimistic_ts)
    );
    assert!(shell.core.composer.text.is_empty());
}
