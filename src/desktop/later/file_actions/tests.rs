use super::*;
#[test]
fn positive_readback_never_confirms_removal_or_defaulted_negative_fields() {
    let item = fixture_item("F1", SavedAction::Save);
    assert!(confirms(Action::Save, &item));
    assert!(!confirms(Action::Remove, &item));
    assert!(!confirms(Action::Unarchive, &item));
    assert!(!confirms(Action::Restore, &item));
    assert!(!confirms(Action::Reminder(0), &item));
    assert!(confirms(
        Action::Complete,
        &fixture_item("F1", SavedAction::Complete)
    ));
    assert!(confirms(
        Action::Archive,
        &fixture_item("F1", SavedAction::Archive)
    ));
    assert!(confirms(
        Action::Reminder(42),
        &fixture_item("F1", SavedAction::Reminder(42))
    ));
}
#[test]
fn unknown_network_failures_are_not_rejections() {
    for e in [
        Error::Offline("lost response".into()),
        Error::Transport("bad shape".into()),
        Error::HttpStatus {
            status: 500,
            retry_after_secs: None,
        },
        Error::Api("internal_error".into()),
    ] {
        assert!(!rejected(&e));
    }
    assert!(rejected(&Error::Api("invalid_auth".into())));
    assert!(rejected(&Error::RateLimited {
        retry_after_secs: Some(60)
    }));
    assert!(
        deadline(&Error::RateLimited {
            retry_after_secs: None
        })
        .is_some()
    );
}
#[tokio::test]
async fn controller_load_and_retire_use_captured_storage_and_exact_uuid() {
    let path = std::env::temp_dir().join(format!(
        "saved-file-controller-{}.sqlite",
        super_platinum_core::slack::api::ForwardMessage::new(
            "C1".into(),
            "1.000001".into(),
            "C2".into(),
            vec![]
        )
        .client_msg_id
    ));
    let target = Target {
        id: "F1".into(),
        title: "Report".into(),
        team: "T1".into(),
        account: "00000000-0000-4000-8000-000000000098".into(),
        user: "U0".into(),
        epoch: 1,
        visit: 1,
        transport: None,
        path: Some(path.clone()),
    };
    let Begin::Started(receipt) = target
        .run(|j, s| {
            j.begin(
                s.clone(),
                Operation {
                    title: "Report".into(),
                    action: Action::Remove,
                },
                100,
            )
        })
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(
        load(&target).await.unwrap().receipt.unwrap().operation_id,
        receipt.operation_id
    );
    assert!(retire(&target, "different-operation".into()).await.is_err());
    assert_eq!(
        retire(&target, receipt.operation_id)
            .await
            .unwrap()
            .receipt
            .unwrap()
            .status,
        Status::Retired
    );
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn controls_share_active_guard_and_failed_deadline_survives_remount() {
    let id = super_platinum_core::slack::api::ForwardMessage::new(
        "C1".into(),
        "1.000001".into(),
        "C2".into(),
        vec![],
    )
    .client_msg_id;
    let path = std::env::temp_dir().join(format!("saved-file-guards-{id}.sqlite"));
    let target = Target {
        id: "F1".into(),
        title: "Report".into(),
        team: "T1".into(),
        account: "00000000-0000-4000-8000-000000000098".into(),
        user: "U0".into(),
        epoch: 1,
        visit: 1,
        transport: None,
        path: Some(path.clone()),
    };
    let active = guards::Active::claim(&target).unwrap();
    assert!(guards::Active::claim(&target.clone()).is_err());
    assert!(retire(&target, "any-operation".into()).await.is_err());
    let mut different = target.clone();
    different.user = "U2".into();
    assert!(guards::Active::claim(&different).is_ok());
    drop(active);
    assert!(guards::Active::claim(&target).is_ok());
    let until = now() + 120;
    guards::remember(&target, until);
    guards::remember(&target, until - 60);
    guards::persisted(&target, until - 60);
    assert_eq!(guards::deadline(&target, now()), Some(until));
    // A directory cannot be a SQLite database: retry failure keeps the barrier.
    std::fs::create_dir(&path).unwrap();
    assert!(cooling_down(&target.clone()).await.is_err());
    assert_eq!(guards::deadline(&target, now()), Some(until));
    std::fs::remove_dir(&path).unwrap();
    target
        .run(|j, s| {
            j.begin(
                s.clone(),
                Operation {
                    title: "Report".into(),
                    action: Action::Save,
                },
                now(),
            )
        })
        .await
        .unwrap();
    assert_eq!(
        load(&target).await.unwrap().receipt.unwrap().retry_at,
        until
    );
    assert!(cooling_down(&target.clone()).await.unwrap().is_some());
    assert_eq!(guards::deadline(&target, now()), None);
    assert_eq!(
        target.run(|j, s| j.get(s)).await.unwrap().unwrap().retry_at,
        until
    );
    let _ = std::fs::remove_file(path);
}
