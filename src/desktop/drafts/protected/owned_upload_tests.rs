//! Actual temporary owned worker; no configured paths or network.
use super::*;
fn ready() -> Arc<Account> {
    let route = o::OwnedRoute::fresh_fixture().unwrap();
    let review = route.inspect().unwrap().wait().unwrap();
    route.commit(&review).unwrap().wait().unwrap();
    let review = route.inspect().unwrap().wait().unwrap();
    route.initialize_slash(&review).unwrap().wait().unwrap();
    Account::from_retained(
        route
            .inspect()
            .unwrap()
            .wait()
            .unwrap()
            .open()
            .unwrap()
            .initialize_cold(&o::FactoryIdentity::default())
            .unwrap()
            .wait()
            .unwrap(),
    )
}
fn draft(account: &Account, n: usize) -> Draft {
    Draft {
        account_id: account.id().into(),
        team_id: "T1".into(),
        user_id: "U1".into(),
        channel_id: "C1".into(),
        thread_ts: Some("1.000001".into()),
        reply_broadcast: true,
        client_msg_id: format!("super-platinum-file-{}", 1000000 + n),
        optimistic_ts: "2.000001".into(),
        caption: "Retained caption \u{65e5}\u{1f680}".into(),
        attachments: vec![uploads::Attachment {
            name: "example.txt".into(),
            bytes: 17,
        }],
    }
}
#[test]
fn owned_upload_backend_completes_96_families_and_cancellations_without_second_worker() {
    let account = ready();
    let ops = &route(&account).unwrap().operations;
    let baseline = ops.retained().len();
    for n in 0..96 {
        let input = draft(&account, n);
        let reserved = reserve_in(&account, input.clone()).unwrap();
        assert!(reserved.created);
        assert!(reserve_in(&account, input).is_err()); // no repeated network permission
        if n % 2 == 0 {
            let row = finalize_in(&account, reserved.stored.ticket, vec!["F1".into()])
                .unwrap()
                .unwrap();
            assert_eq!(load_in(&account).unwrap(), vec![row.clone()]);
            assert!(retire_in(&account, row).unwrap());
        } else {
            assert!(retire_in(&account, reserved.stored).unwrap());
        }
        assert!(load_in(&account).unwrap().is_empty());
        assert_eq!(ops.retained().len(), baseline);
        assert!(
            route(&account)
                .unwrap()
                .uploads
                .0
                .lock()
                .unwrap()
                .is_empty()
        );
    }
}
#[test]
fn original_account_settles_after_ui_replacement_without_touching_replacement_journal() {
    let first = ready();
    let replacement = ready();
    let input = draft(&first, 1);
    let reserved = reserve_in(&first, input.clone()).unwrap();
    let other = reserve_in(&replacement, draft(&replacement, 1)).unwrap();
    assert!(
        finalize_in(
            &replacement,
            reserved.stored.ticket.clone(),
            vec!["F1".into()]
        )
        .is_err()
    );
    // Same durable owner retained through a UI switch; this is not HTTP proof.
    let settled = finalize_in(&first, reserved.stored.ticket, vec!["F1".into()])
        .unwrap()
        .unwrap();
    assert_eq!(settled.draft, input);
    assert_eq!(load_in(&first).unwrap(), vec![settled]);
    assert_eq!(load_in(&replacement).unwrap(), vec![other.stored.clone()]);
    assert!(retire_in(&replacement, other.stored).unwrap());
}
#[test]
fn duplicate_receipt_and_failed_finalize_keep_exact_original_input_and_journal() {
    let account = ready();
    let input = draft(&account, 1);
    let reserved = reserve_in(&account, input.clone()).unwrap();
    assert!(finalize_in(&account, reserved.stored.ticket.clone(), vec![]).is_err());
    assert!(finalize_in(&account, reserved.stored.ticket.clone(), vec!["F1".into()]).is_err());
    let route = route(&account).unwrap();
    let entry = route
        .uploads
        .0
        .lock()
        .unwrap()
        .get(&input.client_msg_id)
        .unwrap()
        .clone();
    let entry = entry.lock().unwrap();
    assert_eq!(*entry.reservation.as_ref().unwrap().input(), input);
    assert!(matches!(&entry.child,Some(Child::Finalize{files,..}) if files.is_empty()));
    assert_eq!(load_in(&account).unwrap(), vec![reserved.stored]);
}
