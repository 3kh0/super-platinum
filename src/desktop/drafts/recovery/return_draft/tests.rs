use super::*;
use crate::{media::MediaRegistry, state::PendingDeliveryStatus};
use super_platinum_core::{ComposerState, slack::models::Message};
#[test]
fn restored_draft_removes_only_unchanged_failed_optimistic_copy() {
    for (pending, failed, changed) in [
        (true, true, false),
        (false, true, false),
        (true, false, false),
        (true, true, true),
    ] {
        let mut shell = ShellState::fixture(MediaRegistry::default());
        let key = shell.composer_key(ComposerTarget::Channel).unwrap();
        let account = "00000000-0000-4000-8000-000000000099".to_owned();
        shell.core.active_account = Some(account.clone());
        let scope = Scope {
            account_id: account,
            key: key.clone(),
        };
        let mut state = DraftSync::new(scope.clone());
        let snapshot = codec::encode(&key, "Unsent").unwrap();
        state.edit(&scope, Some(snapshot.clone()));
        let intent = state
            .request_consume(&scope, state.local_revision(), ConsumeKind::Send)
            .unwrap();
        let workspace = shell.core.workspaces.get_mut(&key.team).unwrap();
        let message = Message {
            ts: Some("99999.000001".into()),
            channel: Some(key.channel.clone()),
            user: Some(workspace.self_user_id.clone()),
            client_msg_id: Some(intent.client_msg_id.clone()),
            text: Some(
                if changed {
                    "Newer edited text"
                } else {
                    "Unsent"
                }
                .into(),
            ),
            blocks: snapshot.blocks,
            ..Default::default()
        };
        let messages = workspace.messages.entry(key.channel.clone()).or_default();
        messages.upsert(message);
        if pending {
            messages.pending.push("99999.000001".into());
        }
        shell.pending_delivery.insert(
            "99999.000001".into(),
            if failed {
                PendingDeliveryStatus::Failed
            } else {
                PendingDeliveryStatus::Unconfirmed
            },
        );
        shell.core.pending_scroll_to = None;
        shell.stick_to_bottom = false;
        let removed = remove_failed_row(
            &mut shell,
            &intent,
            &LocalDraft {
                composer: ComposerState::with_text("Unsent"),
                attachments: vec![],
            },
        );
        if removed {
            // Production projects the restored local before refreshing rows.
            // Disable account persistence in this pure projection regression.
            shell.core.active_account = None;
            shell.core.composer = ComposerState::with_text("Unsent");
            shell.core.composer_attachments.clear();
            shell.core.drafts.items.insert(
                key.clone(),
                LocalDraft {
                    composer: shell.core.composer.clone(),
                    attachments: vec![],
                },
            );
            shell.refresh_from_core();
            assert_eq!(shell.core.drafts.items[&key].composer.text, "Unsent");
            assert_eq!(shell.core.composer.text, "Unsent");
        }
        let retained = shell.core.workspaces[&key.team].messages[&key.channel]
            .messages
            .iter()
            .any(|m| m.ts.as_deref() == Some("99999.000001"));
        assert_eq!(retained, !(pending && failed && !changed));
        assert!(shell.core.pending_scroll_to.is_none());
        assert!(!shell.stick_to_bottom);
    }
}

fn thread_fixture(broadcast: bool) -> (ShellState, DraftKey, ConsumeIntent, LocalDraft, Message) {
    let mut shell = ShellState::fixture(MediaRegistry::default());
    shell.core.active_account = Some("00000000-0000-4000-8000-000000000099".into());
    shell.thread_root = Some("100.000001".into());
    let key = shell.composer_key(ComposerTarget::Thread).unwrap();
    let restored = LocalDraft {
        composer: ComposerState {
            reply_broadcast: broadcast,
            ..ComposerState::with_text("Unsent 日本語 🚀")
        },
        attachments: vec![],
    };
    let scope = Scope {
        account_id: shell.core.active_account.clone().unwrap(),
        key: key.clone(),
    };
    let mut state = DraftSync::new(scope.clone());
    let snapshot = codec::encode_composer(&key, &restored.composer).unwrap();
    state.edit(&scope, Some(snapshot.clone()));
    let intent = state
        .request_consume(&scope, state.local_revision(), ConsumeKind::Send)
        .unwrap();
    let message = Message {
        ts: Some("99999.000001".into()),
        user: Some(shell.core.workspaces[&key.team].self_user_id.clone()),
        channel: Some(key.channel.clone()),
        thread_ts: key.thread.clone(),
        subtype: broadcast.then(|| "thread_broadcast".into()),
        client_msg_id: Some(intent.client_msg_id.clone()),
        text: Some(restored.composer.text.clone()),
        blocks: snapshot.blocks,
        ..Default::default()
    };
    (shell, key, intent, restored, message)
}

#[test]
fn thread_return_projects_matching_composer_and_keeps_channel_and_other_thread() {
    for broadcast in [false, true] {
        let (mut shell, key, _, restored, _) = thread_fixture(broadcast);
        shell.core.active_account = None;
        shell.main_view = MainView::Scheduled;
        shell.core.thread_composer = Default::default();
        shell.core.thread_composer_attachments.clear();
        shell.core.drafts.items.remove(&key);
        shell.core.composer = ComposerState::with_text("Channel writing");
        assert!(project_return(&mut shell, &key, &restored));
        shell.sync_composer_drafts();
        assert_eq!(shell.core.thread_composer.text, restored.composer.text);
        assert_eq!(shell.core.thread_composer.reply_broadcast, broadcast);
        assert_eq!(
            shell.core.drafts.items[&key].composer.reply_broadcast,
            broadcast
        );
        assert_eq!(shell.core.composer.text, "Channel writing");

        shell.core.drafts.items.remove(&key);
        shell.thread_root = Some("100.000002".into());
        shell.core.thread_composer = ComposerState::with_text("Other thread writing");
        assert!(project_return(&mut shell, &key, &restored));
        assert_eq!(shell.core.thread_composer.text, "Other thread writing");
        assert_eq!(
            shell.core.drafts.items[&key].composer.text,
            restored.composer.text
        );
    }
}

#[test]
fn thread_return_refuses_live_or_book_text_files_and_broadcast_only_intent() {
    for in_book in [false, true] {
        for content in ["text", "file", "broadcast"] {
            let (mut shell, key, _, restored, _) = thread_fixture(true);
            shell.core.active_account = None;
            shell.main_view = MainView::Scheduled;
            shell.core.thread_composer = Default::default();
            shell.core.thread_composer_attachments.clear();
            shell.core.drafts.items.remove(&key);
            let mut newer = LocalDraft::default();
            match content {
                "text" => newer.composer = ComposerState::with_text("Newer writing"),
                "broadcast" => newer.composer.reply_broadcast = true,
                _ => newer.attachments.push(
                    serde_json::from_value(serde_json::json!({
                        "id":1,"path":"new.txt","name":"new.txt","bytes":2
                    }))
                    .unwrap(),
                ),
            }
            if in_book {
                shell.core.drafts.items.insert(key.clone(), newer.clone());
            } else {
                shell.core.thread_composer = newer.composer.clone();
                shell.core.thread_composer_attachments = newer.attachments.clone();
            }
            assert!(!project_return(&mut shell, &key, &restored));
            if in_book {
                assert!(crate::drafts::same_local(
                    &shell.core.drafts.items[&key],
                    &newer
                ));
            } else {
                assert_eq!(shell.core.thread_composer, newer.composer);
                assert_eq!(
                    shell.core.thread_composer_attachments.len(),
                    newer.attachments.len()
                );
            }
        }
    }
}

#[test]
fn thread_return_removes_only_exact_unique_failed_row_in_original_thread() {
    for broadcast in [false, true] {
        for mutation in [
            "none",
            "root",
            "channel",
            "broadcast",
            "text",
            "edited",
            "sender",
            "uuid",
            "blocks",
            "confirmed",
            "uncertain",
            "duplicate",
            "account",
        ] {
            let (mut shell, key, intent, restored, original) = thread_fixture(broadcast);
            let mut candidate = original.clone();
            match mutation {
                "root" => candidate.thread_ts = Some("100.000002".into()),
                "channel" => candidate.channel = Some("OTHER".into()),
                "broadcast" => {
                    candidate.subtype = if broadcast {
                        None
                    } else {
                        Some("thread_broadcast".into())
                    }
                }
                "text" => candidate.text = Some("Edited".into()),
                "edited" => candidate.edited = Some(serde_json::json!({"ts":"99999.000002"})),
                "sender" => candidate.user = Some("OTHER".into()),
                "uuid" => candidate.client_msg_id = Some("OTHER".into()),
                "blocks" => candidate.blocks.clear(),
                "account" => shell.core.active_account = Some("different-account".into()),
                _ => (),
            }
            let bag_key = (
                key.team.clone(),
                key.channel.clone(),
                key.thread.clone().unwrap(),
            );
            let other_key = (key.team.clone(), key.channel.clone(), "100.000002".into());
            shell
                .core
                .threads
                .entry(other_key.clone())
                .or_default()
                .upsert(original.clone());
            shell
                .core
                .workspaces
                .get_mut(&key.team)
                .unwrap()
                .messages
                .entry(key.channel.clone())
                .or_default()
                .upsert(original.clone());
            let bag = shell.core.threads.entry(bag_key.clone()).or_default();
            bag.upsert(candidate);
            if mutation != "confirmed" {
                bag.pending.push("99999.000001".into());
            }
            shell.pending_delivery.insert(
                "99999.000001".into(),
                if mutation == "uncertain" {
                    PendingDeliveryStatus::Unconfirmed
                } else {
                    PendingDeliveryStatus::Failed
                },
            );
            if mutation == "duplicate" {
                let mut duplicate = original.clone();
                duplicate.ts = Some("99999.000002".into());
                bag.upsert(duplicate);
                bag.pending.push("99999.000002".into());
                shell
                    .pending_delivery
                    .insert("99999.000002".into(), PendingDeliveryStatus::Failed);
            }
            assert_eq!(
                remove_failed_row(&mut shell, &intent, &restored),
                mutation == "none",
                "{mutation}"
            );
            assert_eq!(
                shell.core.threads[&bag_key]
                    .messages
                    .iter()
                    .any(|m| m.ts == original.ts),
                mutation != "none"
            );
            assert!(
                shell.core.threads[&other_key]
                    .messages
                    .iter()
                    .any(|m| m.ts == original.ts)
            );
            assert!(
                shell.core.workspaces[&key.team].messages[&key.channel]
                    .messages
                    .iter()
                    .any(|m| m.ts == original.ts)
            );
        }
    }
}

#[test]
fn rejected_thread_with_acknowledged_server_draft_cannot_return_as_direct_send() {
    let (_, key, direct, restored, _) = thread_fixture(true);
    let scope = direct.scope.clone();
    let remote = super_platinum_core::drafts::sync::RemoteDraft {
        scope: scope.clone(),
        id: "D1".into(),
        client_msg_id: direct.client_msg_id.clone(),
        last_updated_ts: "100.000003".into(),
        snapshot: codec::encode_composer(&key, &restored.composer).unwrap(),
    };
    let mut state = DraftSync::adopt_remote(&scope, remote).unwrap();
    state
        .request_consume(&scope, state.local_revision(), ConsumeKind::Send)
        .unwrap();
    let operation = state.begin_operation(&scope).unwrap();
    assert!(state.reject(&scope, &operation.id));
    let view = View {
        local: LocalDraft::default(),
        stored: Some(Stored { revision: 1, state }),
        blocked: None,
        uncommitted: None,
    };
    assert!(view.stored.as_ref().unwrap().state.acknowledged().is_some());
    assert!(rejected(&view).is_none());
}

#[test]
fn interrupted_return_projects_only_after_reentering_drafts_without_new_input() {
    let mut shell = ShellState::fixture(MediaRegistry::default());
    let key = shell.composer_key(ComposerTarget::Channel).unwrap();
    shell.core.active_account = None; // No persistence in this projection test.
    shell.core.composer = ComposerState::default();
    shell.core.composer_attachments.clear();
    shell.core.drafts.items.remove(&key);
    let restored = LocalDraft {
        composer: ComposerState::with_text("Restored unsent message"),
        attachments: vec![],
    };
    shell.main_view = MainView::Home;
    assert!(!project_return(&mut shell, &key, &restored));
    assert!(shell.core.composer.text.is_empty());
    assert!(!shell.core.drafts.items.contains_key(&key));
    shell.main_view = MainView::Scheduled;
    let team = shell.core.active_team.take();
    assert!(!project_return(&mut shell, &key, &restored));
    shell.core.active_team = team;
    assert!(project_return(&mut shell, &key, &restored));
    shell.sync_composer_drafts();
    assert_eq!(
        shell.core.drafts.items[&key].composer.text,
        "Restored unsent message"
    );
    shell.core.composer = ComposerState::with_text("Newer typing");
    assert!(!project_return(&mut shell, &key, &restored));
    assert_eq!(shell.core.composer.text, "Newer typing");
}
