use super::*;

#[tokio::test]
async fn failed_empty_close_retains_current_empty_input_separately_from_nonempty_expected_row() {
    for failure in [0, 1, 2] {
        let mut f = Fixture::new().await;
        let capture = f.capture();
        let visit = capture.visit.clone();
        let t = capture
            .persist(note(), Some(destination()), false)
            .await
            .unwrap();
        let outcome = result(&t).await;
        let expected = saved(outcome.clone());
        assert!(durable::apply_capture(
            &mut f.shell,
            &f.id,
            &visit,
            &t,
            &outcome
        ));
        let storage = f.storage();
        let mut stored = expected.clone();
        let mut blocked_attempt = None;
        if failure == 0 {
            let old = expected.clone();
            let mut changed = old.content.clone();
            changed.note[0]["future"] = json!(18);
            let Mutation::Applied(changed) = storage
                .run(move |j| j.save_draft(&old, changed))
                .await
                .unwrap()
            else {
                panic!("external save")
            };
            stored = changed;
        } else if failure == 1 {
            let old = expected.clone();
            let Begin::Started(receipt) = storage
                .run(move |j| j.begin_from_draft(&old, 10))
                .await
                .unwrap()
            else {
                panic!("actual begin")
            };
            blocked_attempt = Some(receipt.attempt_id);
        } else {
            f.shell.file_share.dialog.as_mut().unwrap().draft_storage =
                Some(recovery::Storage::at(storage.key.clone(), f.temp.0.clone()));
        }
        let capture = f.capture();
        let visit = capture.visit.clone();
        let clear = capture.persist(vec![], None, true).await.unwrap();
        let outcome = result(&clear).await;
        match (failure, &outcome) {
            (0, Outcome::Conflict(Some(current))) => assert!(current == &stored),
            (1, Outcome::Blocked(receipt)) => {
                assert_eq!(Some(&receipt.attempt_id), blocked_attempt.as_ref())
            }
            (2, Outcome::Failed(Refusal::Storage)) => {}
            _ => panic!("expected exact refusal for injected case"),
        }
        let scope = expected.scope.clone();
        assert!(storage.run(move |j| j.draft(&scope)).await.unwrap() == Some(stored));
        let Some(Command::Clear {
            expected: old,
            content: current,
        }) = clear.input_ref()
        else {
            panic!("typed empty capture")
        };
        assert!(old == &expected);
        assert_eq!(old.content.note, note());
        assert!(old.content.destination.is_some());
        assert!(current.empty());
        assert!(durable::captured(&clear).unwrap().empty());
        assert!(durable::unsaved(&clear));
        assert!(durable::apply_capture(
            &mut f.shell,
            &f.id,
            &visit,
            &clear,
            &outcome
        ));
        f.recovery(vec![], vec![clear.clone()]);
        let serial = f.shell.file_share.recovery.serial;
        durable::saved::review_local(&mut f.shell, serial, clear.id()).unwrap();
        let d = visible(&f.shell).unwrap();
        assert!(d.draft_local && d.note.is_empty() && d.destination.is_none());
        // A readonly-delete refusal legitimately projects the stored nonempty
        // body. Its exact expected CAS is not replaced by Clear's empty input.
        let readonly = drafts::Visit::review(&storage.key, &expected).unwrap();
        let blocked = submit(
            &f.queue(),
            recovery::Storage::at(storage.key.clone(), f.temp.0.clone()),
            readonly,
            Command::Discard {
                expected: expected.clone(),
            },
        );
        assert!(matches!(
            result(&blocked).await,
            Outcome::Failed(Refusal::Storage)
        ));
        assert!(durable::captured(&blocked).unwrap() == expected.content);
    }
}

#[tokio::test]
async fn clear_submission_refuses_nonempty_or_rebound_content_without_removing_row() {
    let f = Fixture::new().await;
    let expected = f.draft();
    let queue = f.queue();
    let storage = f.storage();
    let visit = visible(&f.shell).unwrap().draft_visit.clone().unwrap();
    for change in 0..4 {
        let mut content = expected.content.clone();
        match change {
            0 => content.note = note(),
            1 => {
                content.destination = Some(
                    super_platinum_core::file_sharing::store::drafts::Destination {
                        id: "C00000001".into(),
                        name: "ship".into(),
                        private: false,
                    },
                )
            }
            2 => content.source["id"] = json!("OTHER"),
            _ => content.title.push('!'),
        }
        let reservation = queue.reserve(storage.clone(), visit.clone()).unwrap();
        let observation = reservation.observe();
        assert!(
            reservation
                .submit(Command::Clear {
                    expected: expected.clone(),
                    content
                })
                .is_err()
        );
        assert!(matches!(result(&observation).await, Outcome::Failed(_)));
        assert!(f.row().await == Some(expected.clone()));
    }
    let readonly = drafts::Visit::review(&storage.key, &expected).unwrap();
    let ticket = submit(
        &queue,
        storage,
        readonly,
        Command::Clear {
            expected: expected.clone(),
            content: expected.content.clone(),
        },
    );
    assert!(matches!(
        result(&ticket).await,
        Outcome::Failed(Refusal::Changed)
    ));
    assert!(f.row().await == Some(expected));
}
