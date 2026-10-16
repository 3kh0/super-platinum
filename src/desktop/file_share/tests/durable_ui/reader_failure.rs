use super::*;

#[tokio::test]
async fn failed_current_reader_never_switches_unsampled_editor_to_older_readonly_capture() {
    for failed_storage in [false, true] {
        let mut f = Fixture::new().await;
        let original = f.draft();
        if failed_storage {
            let key = f.storage().key;
            f.shell.file_share.dialog.as_mut().unwrap().draft_storage =
                Some(recovery::Storage::at(key, f.temp.0.clone()));
        }
        let share = f.capture();
        let close = f.capture();
        let previous_task = close.prior_task.clone().unwrap();
        let ticket = if failed_storage {
            share.persist(note(), None, false).await.unwrap()
        } else {
            share.retain(note(), None).unwrap()
        };
        result(&ticket).await;
        let previous = close.predecessor().await;
        assert!(previous.is_err());
        let observation = close.ticket.clone();
        let visit = close.visit.clone();
        {
            let d = f.shell.file_share.dialog.as_mut().unwrap();
            d.closing = true;
            d.phase = Phase::Preparing;
        }
        drop(close);
        durable::close::reader_failed(
            &mut f.shell,
            &f.id,
            &visit,
            &observation,
            previous,
            Some(previous_task),
        );
        let d = visible(&f.shell).unwrap();
        assert_eq!(d.phase, Phase::Editing);
        assert!(!d.closing && !d.draft_local && !d.saved_draft_stale);
        assert!(d.durable.as_ref() == Some(&original));
        assert!(d.draft_task.as_ref().unwrap().same(&ticket));
        assert_eq!(d.error, Some(Refusal::Note.message()));
        assert!(
            f.queue()
                .retained()
                .unwrap()
                .iter()
                .any(|t| t.same(&ticket))
        );
        if failed_storage {
            let retry = f.capture();
            assert!(retry.predecessor.is_none());
            assert!(retry.prior_task.as_ref().unwrap().same(&ticket));
            let observation = retry.ticket.clone();
            let visit = retry.visit.clone();
            let previous = retry.predecessor().await;
            let prior = retry.prior_task.clone();
            drop(retry);
            durable::close::reader_failed(
                &mut f.shell,
                &f.id,
                &visit,
                &observation,
                previous,
                prior,
            );
            assert!(
                visible(&f.shell)
                    .unwrap()
                    .draft_task
                    .as_ref()
                    .unwrap()
                    .same(&ticket)
            );
            assert_eq!(visible(&f.shell).unwrap().phase, Phase::Editing);
        }
    }
}

#[tokio::test]
async fn unresolved_receipt_load_survives_same_context_source_retirement_but_not_account_rotation()
{
    for rotated in [false, true] {
        let mut f = Fixture::new().await;
        let t = f
            .capture()
            .persist(note(), Some(destination()), false)
            .await
            .unwrap();
        let draft = saved(result(&t).await);
        let expected = draft.clone();
        let storage = f.storage();
        let Begin::Started(receipt) = storage
            .run(move |j| j.begin_from_draft(&expected, 10))
            .await
            .unwrap()
        else {
            panic!("begun")
        };
        close(&mut f.shell, &f.id);
        let source = admission::Source::capture(&f.shell, &f.target).unwrap();
        let load =
            durable::open::prepare_captured(&mut f.shell, source, "source".into(), Ok(storage))
                .unwrap();
        let outcome = load.outcome().await;
        assert!(matches!(&outcome,Outcome::Blocked(r) if r.attempt_id==receipt.attempt_id));
        if rotated {
            f.shell.core.account_epoch += 1;
        } else {
            crate::file_actions::attachment::tests::message(&mut f.shell).files[0].title =
                Some("Changed while loading".into());
        }
        load.publish(&mut f.shell, outcome);
        if rotated {
            assert!(visible(&f.shell).is_none());
        } else {
            let d = visible(&f.shell).unwrap();
            assert_eq!(d.phase, Phase::Outcome);
            assert_eq!(d.receipt.as_ref().unwrap().attempt_id, receipt.attempt_id);
            assert!(d.admission.is_none());
        }
    }
}
