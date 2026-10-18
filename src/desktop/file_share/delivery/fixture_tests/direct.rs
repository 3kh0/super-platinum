use super::*;
use crate::file_share::fixture::Conversation;
fn with_destination(
    temp: &Temp,
    case: Case,
    conversation: Conversation,
) -> (ShellState, crate::file_actions::Target, Capability) {
    let (mut shell, target, _) = seed(temp, case);
    let w = shell.core.workspaces.get_mut(fixture::TEAM).unwrap();
    w.channels.insert(
        conversation.id().into(),
        serde_json::from_value(conversation.raw()).unwrap(),
    );
    w.users.insert(
        "U00000002".into(),
        serde_json::from_value(fixture::peer()).unwrap(),
    );
    let cap = Capability::for_test(temp.0.clone(), case, 7, 11)
        .unwrap()
        .with_conversation(conversation);
    shell.file_share_fixture = Some(cap.clone());
    (shell, target, cap)
}
fn read_receipt(cap: &Capability) -> Receipt {
    let mut scope = scope();
    scope.destination = cap.destination_id().into();
    Journal::open(cap.storage_path(), fixture::ACCOUNT)
        .unwrap()
        .get(&scope)
        .unwrap()
        .unwrap()
}
#[tokio::test]
async fn actual_worker_existing_dm_and_group_dm_preserve_accepted_and_unknown_custody() {
    for conversation in [Conversation::Direct, Conversation::Group] {
        for case in [Case::Accepted, Case::Unconfirmed] {
            let temp = Temp::new(case);
            let (mut shell, target, cap) = with_destination(&temp, case, conversation);
            let id = open_selected(&mut shell, target.clone()).await;
            choose(&mut shell, &id, conversation.id()).unwrap();
            let (mut dom, run) = mount(shell, id);
            until(&mut dom, || {
                let ready = cap.evidence().unwrap().awaiting_release;
                if !ready {
                    assert!(
                        run.done.borrow().is_none(),
                        "worker stopped before awaiting_release: {:?}",
                        run.done.borrow()
                    );
                }
                ready
            })
            .await;
            let before = read_receipt(&cap);
            let evidence = cap.evidence().unwrap();
            assert_eq!(evidence.dispatch_count, 1);
            let expected = if conversation == Conversation::Direct {
                vec!["files.info", "conversations.info", "users/info"]
            } else {
                vec!["files.info", "conversations.info"]
            };
            assert_eq!(evidence.reads, expected);
            assert_eq!(before.scope.destination, conversation.id());
            assert_eq!(before.intent.note(), Some(note().as_slice()));
            let fields = evidence.fields.unwrap();
            assert_eq!(
                fields.iter().find(|(k, _)| k == "channel").unwrap().1,
                conversation.id()
            );
            assert_eq!(
                fields.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
                [
                    "token",
                    "files",
                    "channel",
                    "blocks",
                    "resharing_aware",
                    "skip_dlp_user_warning",
                    "from_share_modal"
                ]
            );
            // Once dispatched, local peer removal cannot discard the result.
            {
                let mut signal = run.signal.get().unwrap();
                signal
                    .write()
                    .core
                    .workspaces
                    .get_mut(fixture::TEAM)
                    .unwrap()
                    .users
                    .remove("U00000002");
            }
            cap.release(&before.attempt_id).unwrap();
            until(&mut dom, || run.done.borrow().is_some()).await;
            assert_eq!(*run.done.borrow(), Some(Ok(())));
            let after = read_receipt(&cap);
            assert_eq!(after.attempt_id, before.attempt_id);
            assert!(
                after.intent == before.intent,
                "Share intent changed after completion"
            );
            if case == Case::Unconfirmed {
                assert_eq!(after.state(), ReceiptState::Unconfirmed);
                // Reopen the actual SQLite journal; no readback can promote an
                // uncertain attempt and Begin must return the same blocked row.
                let mut journal = Journal::open(cap.storage_path(), fixture::ACCOUNT).unwrap();
                assert!(matches!(
                    journal.begin(
                        after.scope.clone(),
                        after.intent.clone(),
                        after.file_title.clone(),
                        after.destination_name.clone(),
                        9000
                    ),
                    Ok(super_platinum_core::file_sharing::store::Begin::Blocked(_))
                ));
            } else {
                assert!(matches!(
                    after.state(),
                    ReceiptState::Server(FileShareDisposition::Accepted { .. })
                ));
            }
            assert_eq!(cap.evidence().unwrap().dispatch_count, 1);
            assert!(
                cap.prepare(
                    &super_platinum_core::file_sharing::Intent::new(
                        fixture::FILE.into(),
                        fixture::DESTINATION.into(),
                        None,
                        None
                    )
                    .unwrap()
                )
                .is_err()
            );
        }
    }
}

#[tokio::test]
async fn recipient_retirement_after_journal_reservation_cancels_before_dispatch() {
    for conversation in [Conversation::Direct, Conversation::Group] {
        let temp = Temp::new(Case::SourceStale);
        let (mut shell, target, cap) = with_destination(&temp, Case::SourceStale, conversation);
        let id = open_selected(&mut shell, target).await;
        choose(&mut shell, &id, conversation.id()).unwrap();
        let (mut dom, run) = mount(shell, id);
        until(&mut dom, || {
            let ready = cap.evidence().unwrap().awaiting_begin_release;
            if !ready {
                assert!(
                    run.done.borrow().is_none(),
                    "worker stopped before awaiting_begin_release: {:?}",
                    run.done.borrow()
                );
            }
            ready
        })
        .await;
        let before = read_receipt(&cap);
        {
            let mut signal = run.signal.get().unwrap();
            let mut shell = signal.write();
            let w = shell.core.workspaces.get_mut(fixture::TEAM).unwrap();
            if conversation == Conversation::Direct {
                w.users.get_mut("U00000002").unwrap().deleted = true;
            } else {
                w.channels
                    .get_mut(conversation.id())
                    .unwrap()
                    .extra
                    .insert("members".into(), json!([fixture::USER, "U00000009"]));
            }
        }
        cap.release_begin(&before.attempt_id).unwrap();
        until(&mut dom, || run.done.borrow().is_some()).await;
        assert_eq!(*run.done.borrow(), Some(Ok(())));
        let after = read_receipt(&cap);
        assert_eq!(after.attempt_id, before.attempt_id);
        assert_eq!(
            after.state(),
            ReceiptState::NotSent(NotSent::AdmissionCanceled)
        );
        assert_eq!(cap.evidence().unwrap().dispatch_count, 0);
    }
}
