use super::*;
use crate::media::MediaRegistry;
use super_platinum_core::{
    ComposerState,
    drafts::{
        store::sync::Stored,
        sync::{DraftSync, Scope},
    },
};

fn local(text: &str) -> LocalDraft {
    LocalDraft {
        composer: ComposerState::with_text(text),
        attachments: vec![],
    }
}
fn shell() -> (ShellState, DraftKey) {
    let mut shell = ShellState::fixture(MediaRegistry::default());
    // These pure projection tests cannot write an account store.
    shell.core.active_account = None;
    let key = shell.composer_key(ComposerTarget::Channel).unwrap();
    shell.core.drafts.channel = Some(key.clone());
    shell.core.composer = ComposerState::with_text("Original");
    shell
        .core
        .drafts
        .items
        .insert(key.clone(), local("Original"));
    (shell, key)
}
fn intent(key: DraftKey) -> ConsumeIntent {
    let scope = Scope {
        account_id: "00000000-0000-4000-8000-000000000097".into(),
        key,
    };
    let mut state = DraftSync::new(scope.clone());
    state.edit(
        &scope,
        Some(super::super::codec::encode(&scope.key, "Original").unwrap()),
    );
    state
        .request_consume(&scope, state.local_revision(), ConsumeKind::Send)
        .unwrap()
}

#[test]
fn durable_clear_removes_only_original_composer_and_book_content() {
    let (mut shell, key) = shell();
    assert!(clear_consumed(&mut shell, &key, &local("Original")));
    assert!(shell.core.composer.text.is_empty());
    assert!(!shell.core.drafts.items.contains_key(&key));
    shell.core.composer = ComposerState::with_text("Newer typing");
    shell
        .core
        .drafts
        .items
        .insert(key.clone(), local("Newer typing"));
    assert!(!clear_consumed(&mut shell, &key, &local("Original")));
    assert_eq!(shell.core.composer.text, "Newer typing");
    assert_eq!(shell.core.drafts.items[&key].composer.text, "Newer typing");
}

#[test]
fn navigation_during_consume_clears_original_book_without_touching_current_field() {
    let (mut shell, key) = shell();
    shell.core.active_channel = Some("CAnother".into());
    shell.core.composer = ComposerState::with_text("Other channel");
    assert!(!clear_consumed(&mut shell, &key, &local("Original")));
    assert!(!shell.core.drafts.items.contains_key(&key));
    assert_eq!(shell.core.composer.text, "Other channel");
}

#[test]
fn positive_receipt_does_not_replace_realtime_edit_or_resurrect_deletion() {
    for deleted in [false, true] {
        let (mut shell, key) = shell();
        let intent = intent(key.clone());
        let pending = append_pending(&mut shell, &intent, &local("Original")).unwrap();
        let ts = "1800000010.000001".to_owned();
        let messages = shell
            .core
            .workspaces
            .get_mut(&key.team)
            .unwrap()
            .messages
            .get_mut(&key.channel)
            .unwrap();
        if deleted {
            messages.record_remote_deletion(&ts);
        } else {
            messages.upsert(Message {
                ts: Some(ts.clone()),
                text: Some("Edited in Slack".into()),
                ..Default::default()
            });
        }
        apply_outcome(
            &mut shell,
            &pending,
            &intent,
            &local("Original"),
            &Outcome::Sent {
                client_msg_id: intent.client_msg_id.clone(),
                draft_id: None,
                ts: ts.clone(),
            },
            true,
        );
        let messages = &shell.core.workspaces[&key.team].messages[&key.channel];
        assert!(
            !messages
                .messages
                .iter()
                .any(|message| message.ts.as_ref() == Some(&pending.optimistic_ts))
        );
        let confirmed = messages
            .messages
            .iter()
            .find(|message| message.ts.as_ref() == Some(&ts));
        if deleted {
            assert!(confirmed.is_none());
        } else {
            assert_eq!(confirmed.unwrap().text.as_deref(), Some("Edited in Slack"));
        }
    }
}

#[test]
fn failed_receipt_persistence_marks_retained_optimistic_content_unconfirmed() {
    let (mut shell, key) = shell();
    let intent = intent(key.clone());
    let pending = append_pending(&mut shell, &intent, &local("Original")).unwrap();
    apply_outcome(
        &mut shell,
        &pending,
        &intent,
        &local("Original"),
        &Outcome::Sent {
            client_msg_id: intent.client_msg_id.clone(),
            draft_id: None,
            ts: "1800000010.000001".into(),
        },
        false,
    );
    assert_eq!(
        shell.pending_delivery[&pending.optimistic_ts],
        PendingDeliveryStatus::Unconfirmed
    );
    assert!(
        shell.core.workspaces[&key.team].messages[&key.channel]
            .messages
            .iter()
            .any(|message| message.client_msg_id.as_ref() == Some(&intent.client_msg_id))
    );
}

#[test]
fn direct_send_requires_clean_local_ownership_and_no_prior_consumption() {
    let (_, key) = shell();
    let scope = Scope {
        account_id: "00000000-0000-4000-8000-000000000097".into(),
        key,
    };
    let mut state = DraftSync::new(scope.clone());
    state.edit(
        &scope,
        Some(super::super::codec::encode(&scope.key, "Original").unwrap()),
    );
    let mut view = View {
        local: local("Original"),
        stored: Some(Stored { revision: 1, state }),
        blocked: None,
        uncommitted: None,
    };
    assert!(can_consume(&view));
    view.uncommitted = Some(local("Still saving"));
    assert!(!can_consume(&view));
    view.uncommitted = None;
    let stored = view.stored.as_mut().unwrap();
    stored
        .state
        .request_consume(&scope, stored.state.local_revision(), ConsumeKind::Send)
        .unwrap();
    assert!(!can_consume(&view));
}

#[test]
fn thread_consumption_captures_broadcast_and_projects_only_the_original_destination() {
    for broadcast in [false, true] {
        let (mut shell, mut key) = shell();
        key.thread = Some("100.000001".into());
        shell.thread_root = key.thread.clone();
        let mut original = local("Original");
        original.composer.reply_broadcast = broadcast;
        shell.core.thread_composer = original.composer.clone();
        shell
            .core
            .drafts
            .items
            .insert(key.clone(), original.clone());
        let intent = intent(key.clone());
        let pending = append_pending(&mut shell, &intent, &original).unwrap();
        assert_eq!(pending.thread_ts, key.thread);
        assert_eq!(pending.reply_broadcast, broadcast);
        assert!(clear_consumed(&mut shell, &key, &original));
        assert!(!shell.core.thread_composer.reply_broadcast);
        assert_eq!(shell.core.composer.text, "Original");
        shell.begin_thread("100.000002".into(), None);
        shell.core.thread_composer = super_platinum_core::ComposerState::with_text("New thread");
        let outcome = Outcome::Sent {
            ts: "200.000001".into(),
            client_msg_id: pending.client_msg_id.clone(),
            draft_id: None,
        };
        apply_outcome(&mut shell, &pending, &intent, &original, &outcome, true);
        let thread = &shell.core.threads[&(
            key.team.clone(),
            key.channel.clone(),
            key.thread.clone().unwrap(),
        )];
        assert!(
            thread
                .messages
                .iter()
                .any(|m| m.ts.as_deref() == Some("200.000001"))
        );
        let channel = &shell.core.workspaces[&key.team].messages[&key.channel];
        assert_eq!(
            channel
                .messages
                .iter()
                .any(|m| m.ts.as_deref() == Some("200.000001")),
            broadcast
        );
        assert_eq!(shell.core.thread_composer.text, "New thread");
    }
}

#[test]
fn changing_only_broadcast_during_consumption_keeps_the_new_draft() {
    let (mut shell, mut key) = shell();
    key.thread = Some("100.000001".into());
    shell.thread_root = key.thread.clone();
    let original = local("Original");
    shell.core.thread_composer = original.composer.clone();
    shell.core.thread_composer.reply_broadcast = true;
    assert!(!clear_consumed(&mut shell, &key, &original));
    assert!(shell.core.thread_composer.reply_broadcast);
    assert_eq!(shell.core.thread_composer.text, "Original");
}
