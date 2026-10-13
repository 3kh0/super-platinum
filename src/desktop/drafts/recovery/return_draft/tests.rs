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
