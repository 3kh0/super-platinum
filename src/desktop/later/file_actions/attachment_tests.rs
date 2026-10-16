use super::*;
use crate::file_actions::attachment::tests::{message, sample};

fn captured(shell: &ShellState, attachment: crate::file_actions::Target, path: PathBuf) -> Target {
    let team = shell.core.active_team.clone().unwrap();
    Target {
        id: attachment.id,
        title: attachment.title,
        account: shell.core.active_account.clone().unwrap(),
        user: shell.core.workspaces[&team].self_user_id.clone(),
        team,
        epoch: shell.core.account_epoch,
        visit: shell.draft_writes.view_id,
        transport: shell.core.transport.clone(),
        path: Some(path),
        attachment: None,
    }
    .with_attachment(attachment.attachment)
}

#[test]
fn retained_save_target_refuses_canonical_changes_without_revoking_recovery_owner() {
    for change in 0..7 {
        let (mut shell, _, original) = sample();
        let target = captured(&shell, original, PathBuf::from("unused.sqlite"));
        assert!(target.admit_new_write(&shell).is_ok());
        match change {
            0 => message(&mut shell).files.clear(),
            1 => {
                let file = message(&mut shell).files[0].clone();
                message(&mut shell).files.push(file);
            }
            2 => message(&mut shell).files[0].title = Some("Changed".into()),
            3 => {
                message(&mut shell).files[0]
                    .extra
                    .insert("user".into(), serde_json::json!("UOTHER"));
            }
            4 => {
                message(&mut shell).files[0].extra.insert(
                    "permalink".into(),
                    serde_json::json!("https://fixture.slack.com/files/UOTHER/OTHER"),
                );
            }
            5 => shell.channel_generation += 1,
            _ => shell.core.active_channel = Some("C_OTHER".into()),
        }
        assert!(
            target.current(&shell),
            "existing journal owner remains current"
        );
        assert_eq!(
            target.admit_new_write(&shell).unwrap_err(),
            "This message attachment changed. Reopen its file actions."
        );
        let unguarded = target.clone().with_attachment(None);
        assert!(
            unguarded.admit_new_write(&shell).is_ok(),
            "existing nonattachment entry behavior unchanged"
        );
        assert!(
            target != unguarded,
            "retained component props carry the proof"
        );
    }
}

#[tokio::test]
async fn pre_admission_recheck_and_admitted_journal_custody_have_distinct_lifetimes() {
    let (mut shell, _, original) = sample();
    let path = std::env::temp_dir().join(format!(
        "saved-attachment-{}.sqlite",
        super_platinum_core::slack::api::ForwardMessage::new(
            "C1".into(),
            "1.000001".into(),
            "C2".into(),
            vec![]
        )
        .client_msg_id
    ));
    let target = captured(&shell, original, path.clone());
    assert!(target.admit_new_write(&shell).is_ok());
    tokio::task::yield_now().await;
    let file = message(&mut shell).files.remove(0);
    assert!(target.admit_new_write(&shell).is_err());
    assert!(
        !path.exists(),
        "refused admission has not reserved a journal write"
    );
    message(&mut shell).files.push(file);
    target.admit_new_write(&shell).unwrap();
    let Begin::Started(receipt) = target
        .run(|j, s| {
            j.begin(
                s.clone(),
                Operation {
                    title: "Report".into(),
                    action: Action::Save,
                },
                100,
            )
        })
        .await
        .unwrap()
    else {
        panic!("fresh journal")
    };
    message(&mut shell).files.clear();
    assert!(target.admit_new_write(&shell).is_err());
    assert!(target.current(&shell));
    let id = receipt.operation_id.clone();
    assert!(
        target
            .run(move |j, s| j.resolve(s, &id, Outcome::Confirmed, 101, 0))
            .await
            .is_ok()
    );
    let observed = load(&target).await.unwrap().receipt.unwrap();
    assert_eq!(observed.operation_id, receipt.operation_id);
    assert_eq!(observed.status, Status::Confirmed);
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn actual_change_refuses_retained_save_handler_before_journal_creation() {
    use std::{cell::Cell, rc::Rc};
    type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
    let slot = Slot::default();
    let mut dom = VirtualDom::new_with_props(
        |slot: Slot| {
            let state = use_signal(|| sample().0);
            slot.set(Some(state));
            VNode::empty()
        },
        slot.clone(),
    );
    dom.rebuild_to_vec();
    let mut state = slot.get().unwrap();
    for change_kind in 0..3 {
        let (shell, _, original) = sample();
        let path = std::env::temp_dir().join(format!(
            "retained-save-{}.sqlite",
            super_platinum_core::slack::api::ForwardMessage::new(
                "C1".into(),
                "1.000001".into(),
                "C2".into(),
                vec![]
            )
            .client_msg_id
        ));
        let target = captured(&shell, original, path.clone());
        dom.in_runtime(|| {
            *state.write() = shell;
            let mut shell = state.write();
            match change_kind {
                0 => message(&mut shell).files.clear(),
                1 => {
                    let file = message(&mut shell).files[0].clone();
                    message(&mut shell).files.push(file);
                }
                _ => shell.channel_generation += 1,
            }
        });
        assert!(target.current(&state.peek()));
        let result = change(state, target, SavedAction::Save).await;
        assert_eq!(
            result.err().as_deref(),
            Some("This message attachment changed. Reopen its file actions.")
        );
        assert!(
            !path.exists(),
            "actual change must not begin a new journal operation"
        );
    }
}
