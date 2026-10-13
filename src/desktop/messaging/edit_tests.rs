use super::*;
use super_platinum_core::slack::{
    Error, Transport,
    models::{Message, SentMessage},
};

fn captured() -> (ShellState, EditVisit) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.core.active_account = Some("edit-account".into());
    shell.core.account_epoch = 4;
    let transport = Arc::new(Transport::new("offline-test-cookie").unwrap());
    shell.core.transport = Some(transport.clone());
    let team = shell.core.active_team.clone().unwrap();
    let channel = shell.core.active_channel.clone().unwrap();
    let ts = "1800000000.000001".to_owned();
    let message = Message {
        ts: Some(ts.clone()),
        text: Some("Original cached text".into()),
        ..Default::default()
    };
    shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .messages
        .entry(channel.clone())
        .or_default()
        .upsert(message.clone());
    shell
        .core
        .threads
        .entry((team, channel.clone(), ts.clone()))
        .or_default()
        .upsert(message);
    shell.start_edit(channel, ts, " Submitted edit ".into());
    let visit = EditVisit::capture(&shell, &transport).unwrap();
    (shell, visit)
}
fn receipt(visit: &EditVisit) -> SentMessage {
    SentMessage {
        channel: visit.channel.clone(),
        ts: visit.ts.clone(),
        message: Message {
            text: Some("Submitted edit".into()),
            ..Default::default()
        },
    }
}
fn text(shell: &ShellState, visit: &EditVisit) -> String {
    shell.core.workspaces[&visit.team].messages[&visit.channel]
        .messages
        .iter()
        .find(|m| m.ts.as_ref() == Some(&visit.ts))
        .unwrap()
        .text
        .clone()
        .unwrap()
}

#[test]
fn edit_receipt_updates_original_channel_and_thread_and_closes_only_exact_editor() {
    let (mut shell, visit) = captured();
    assert!(apply_edit_result(&mut shell, &visit, Ok(receipt(&visit))));
    assert_eq!(text(&shell, &visit), "Submitted edit");
    assert_eq!(
        shell.core.threads[&(visit.team.clone(), visit.channel.clone(), visit.ts.clone())].messages
            [0]
        .text
        .as_deref(),
        Some("Submitted edit")
    );
    assert!(shell.core.editing.is_none());
}

#[test]
fn account_visit_and_transport_changes_reject_edit_success_and_failure() {
    for change in 0..3 {
        for success in [false, true] {
            let (mut shell, visit) = captured();
            match change {
                0 => shell.core.active_account = Some("other-account".into()),
                1 => shell.core.account_epoch += 1,
                _ => {
                    shell.core.transport =
                        Some(Arc::new(Transport::new("replacement-cookie").unwrap()))
                }
            }
            shell.toast = None;
            let result = if success {
                Ok(receipt(&visit))
            } else {
                Err(Error::Api("invalid_auth".into()))
            };
            assert!(!apply_edit_result(&mut shell, &visit, result));
            assert_eq!(text(&shell, &visit), "Original cached text");
            assert_eq!(shell.core.edit_composer.text, " Submitted edit ");
            assert!(shell.core.editing.is_some());
            assert!(shell.toast.is_none());
        }
    }
}

#[test]
fn newer_text_other_message_and_same_message_reopen_survive_old_receipt() {
    for change in 0..4 {
        let (mut shell, visit) = captured();
        match change {
            0 => shell.core.edit_composer.text = "New unsaved edit".into(),
            1 => shell.start_edit(
                visit.channel.clone(),
                "1800000000.000002".into(),
                "Other message".into(),
            ),
            2 => {
                shell.cancel_edit();
                shell.start_edit(
                    visit.channel.clone(),
                    visit.ts.clone(),
                    visit.raw_text.clone(),
                );
            }
            _ => shell.core.edit_composer.text = "Submitted edit".into(), // Raw whitespace changed.
        }
        let editing = shell.core.editing.clone();
        let composer = shell.core.edit_composer.clone();
        assert!(apply_edit_result(&mut shell, &visit, Ok(receipt(&visit))));
        assert_eq!(text(&shell, &visit), "Submitted edit");
        assert_eq!(shell.core.editing, editing);
        assert_eq!(shell.core.edit_composer, composer);
        shell.toast = None;
        assert!(!apply_edit_result(
            &mut shell,
            &visit,
            Err(Error::Api("cant_update_message".into()))
        ));
        assert!(shell.toast.is_none());
    }
}

#[test]
fn edit_receipt_cannot_resurrect_deleted_rows_or_update_a_wrong_destination() {
    let (mut shell, visit) = captured();
    let mut wrong = receipt(&visit);
    wrong.channel = "different-channel".into();
    assert!(!apply_edit_result(&mut shell, &visit, Ok(wrong)));
    assert_eq!(text(&shell, &visit), "Original cached text");
    shell
        .core
        .workspaces
        .get_mut(&visit.team)
        .unwrap()
        .messages
        .get_mut(&visit.channel)
        .unwrap()
        .record_remote_deletion(&visit.ts);
    shell
        .core
        .threads
        .get_mut(&(visit.team.clone(), visit.channel.clone(), visit.ts.clone()))
        .unwrap()
        .record_remote_deletion(&visit.ts);
    assert!(apply_edit_result(&mut shell, &visit, Ok(receipt(&visit))));
    assert!(
        !shell.core.workspaces[&visit.team].messages[&visit.channel]
            .messages
            .iter()
            .any(|m| m.ts.as_ref() == Some(&visit.ts))
    );
    assert!(
        shell.core.threads[&(visit.team.clone(), visit.channel.clone(), visit.ts.clone())]
            .messages
            .is_empty()
    );
}

#[test]
fn reversed_success_and_realtime_edit_preserve_newer_cached_content() {
    for realtime in [false, true] {
        let (mut shell, old) = captured();
        let newer = Message {
            ts: Some(old.ts.clone()),
            text: Some("Newer confirmed edit".into()),
            blocks: vec![serde_json::json!({"type":"rich_text","newer":true})],
            edited: Some(serde_json::json!({"ts":"1800000010.000001"})),
            ..Default::default()
        };
        if realtime {
            shell
                .core
                .workspaces
                .get_mut(&old.team)
                .unwrap()
                .messages
                .get_mut(&old.channel)
                .unwrap()
                .merge_update(newer.clone());
            shell
                .core
                .threads
                .get_mut(&(old.team.clone(), old.channel.clone(), old.ts.clone()))
                .unwrap()
                .merge_update(newer.clone());
        } else {
            shell.core.edit_composer.text = "Newer confirmed edit".into();
            let visit = EditVisit::capture(&shell, shell.core.transport.as_ref().unwrap()).unwrap();
            let result = SentMessage {
                message: newer.clone(),
                ..receipt(&visit)
            };
            assert!(apply_edit_result(&mut shell, &visit, Ok(result)));
        }
        assert!(apply_edit_result(&mut shell, &old, Ok(receipt(&old))));
        assert_eq!(text(&shell, &old), "Newer confirmed edit");
        assert_eq!(
            shell.core.threads[&(old.team.clone(), old.channel.clone(), old.ts.clone())].messages
                [0]
            .text
            .as_deref(),
            Some("Newer confirmed edit")
        );
    }
}

#[test]
fn edit_success_preserves_later_reactions_and_never_inserts_into_another_thread() {
    let (mut shell, visit) = captured();
    let reaction = super_platinum_core::slack::models::Reaction {
        name: "thumbsup".into(),
        count: 3,
        ..Default::default()
    };
    shell
        .core
        .workspaces
        .get_mut(&visit.team)
        .unwrap()
        .messages
        .get_mut(&visit.channel)
        .unwrap()
        .messages
        .iter_mut()
        .find(|m| m.ts.as_ref() == Some(&visit.ts))
        .unwrap()
        .reactions = vec![reaction];
    let unrelated = (
        visit.team.clone(),
        visit.channel.clone(),
        "1800000009.000001".into(),
    );
    shell.core.threads.entry(unrelated.clone()).or_default();
    assert!(apply_edit_result(&mut shell, &visit, Ok(receipt(&visit))));
    let message = shell.core.workspaces[&visit.team].messages[&visit.channel]
        .messages
        .iter()
        .find(|m| m.ts.as_ref() == Some(&visit.ts))
        .unwrap();
    assert_eq!(message.reactions[0].count, 3);
    assert_eq!(message.text.as_deref(), Some("Submitted edit"));
    assert!(shell.core.threads[&unrelated].messages.is_empty());
}
