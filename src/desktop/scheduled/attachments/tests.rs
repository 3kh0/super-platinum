use super::*;
use serde_json::json;

fn owner(account: &str) -> Owner {
    Owner {
        account: account.into(),
        team: "T1".into(),
        user: "U1".into(),
        draft: "D1".into(),
    }
}

fn raw() -> Value {
    json!({"id":"D1","client_msg_id":"uuid","date_scheduled":0,"last_updated_ts":"1",
        "is_deleted":false,"is_sent":false,"file_ids":["F1","F2"],
        "blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section",
            "elements":[{"type":"text","text":"Original rich message","style":{"bold":true}}]}]}],
        "destinations":[{"channel_id":"C1","thread_ts":"1.2","broadcast":true}]})
}

fn draft() -> ScheduledMessage {
    serde_json::from_value(raw()).unwrap()
}
fn envelope(draft: Value) -> Value {
    json!({"ok":true,"draft":draft})
}

fn prepared() -> (Operations, Lease, Attempt) {
    let mut operations = Operations::default();
    let lease = operations.open(owner("A1"), draft()).unwrap();
    operations.remove(&lease, "F1").unwrap();
    let attempt = operations.freeze(&lease).unwrap();
    (operations, lease, attempt)
}

fn uncertain() -> (Operations, Lease, Attempt) {
    let (mut operations, lease, attempt) = prepared();
    operations.admit(&attempt, &envelope(raw())).unwrap();
    operations.uncertain(&attempt).unwrap();
    (operations, lease, attempt)
}

fn desired() -> Value {
    let mut value = raw();
    value["file_ids"] = json!(["F2"]);
    envelope(value)
}

#[test]
fn text_editor_and_mutations_share_reservation_with_attachment_editor() {
    let mut operations = Operations::default();
    let text_editor = operations.reserve_other(owner("A1")).unwrap();
    assert_eq!(
        operations.open(owner("A1"), draft()).unwrap_err(),
        Fault::Busy
    );
    assert_eq!(
        operations.reserve_other(owner("A1")).unwrap_err(),
        Fault::Busy
    );
    operations.release_other(&text_editor).unwrap();
    let attachments = operations.open(owner("A1"), draft()).unwrap();
    assert_eq!(
        operations.reserve_other(owner("A1")).unwrap_err(),
        Fault::Busy
    );
    assert_eq!(operations.release_other(&attachments), Err(Fault::Retained));
    operations.cancel(&attachments).unwrap();
    assert!(operations.reserve_other(owner("A1")).is_ok());
}

#[test]
fn close_and_reopen_retains_staging_and_never_reloads_new_body() {
    let mut operations = Operations::default();
    let lease = operations.open(owner("A1"), draft()).unwrap();
    operations.remove(&lease, "F1").unwrap();
    let mut newer = draft();
    newer.blocks.clear();
    newer.file_ids.clear();
    let reopened = operations.open(owner("A1"), newer).unwrap();
    assert_eq!(lease, reopened);
    let snapshot = operations.snapshot(&reopened).unwrap();
    assert!(snapshot.rows[0].removed);
    assert!(!snapshot.blocks.is_empty());
    operations.restore(&reopened, "F1").unwrap();
    assert!(!operations.snapshot(&reopened).unwrap().changed);
}

#[test]
fn safe_cancel_invalidates_preflight_and_stale_metadata_without_touching_reopened_editor() {
    let (mut operations, lease, attempt) = prepared();
    operations.cancel(&lease).unwrap();
    let reopened = operations.open(owner("A1"), draft()).unwrap();
    assert_ne!(lease, reopened);
    assert_eq!(
        operations.admit(&attempt, &envelope(raw())),
        Err(Fault::StaleLease)
    );
    assert_eq!(
        operations.metadata(
            &lease,
            "F1",
            File {
                id: Some("F1".into()),
                ..Default::default()
            }
        ),
        Err(Fault::StaleLease)
    );
    assert_eq!(operations.cancel(&lease), Err(Fault::StaleLease));
    assert!(!operations.snapshot(&reopened).unwrap().changed);
}

#[test]
fn in_flight_and_uncertain_cannot_cancel_or_reopen_into_fresh_write() {
    let (mut operations, lease, attempt) = prepared();
    operations.admit(&attempt, &envelope(raw())).unwrap();
    assert_eq!(operations.cancel(&lease), Err(Fault::Retained));
    assert_eq!(
        operations.reserve_other(owner("A1")).unwrap_err(),
        Fault::Busy
    );
    operations.uncertain(&attempt).unwrap();
    let reopened = operations.open(owner("A1"), draft()).unwrap();
    assert_eq!(reopened, lease);
    assert_eq!(
        operations.snapshot(&reopened).unwrap().phase,
        EditPhase::Uncertain
    );
    assert_eq!(operations.cancel(&reopened), Err(Fault::Retained));
    assert_eq!(
        operations.freeze(&reopened).unwrap_err(),
        Fault::Edit(EditError::InvalidPhase)
    );
    assert_eq!(
        operations.remove(&reopened, "F2"),
        Err(Fault::Edit(EditError::InvalidPhase))
    );
    assert_eq!(
        operations.preflight_failed(&attempt, "late error".into()),
        Err(Fault::Edit(EditError::InvalidPhase))
    );
}

#[test]
fn stable_owner_separates_account_team_self_user_without_epoch_reset() {
    let (mut operations, lease, _) = uncertain();
    for other in [
        owner("A2"),
        Owner {
            team: "T2".into(),
            ..owner("A1")
        },
        Owner {
            user: "U2".into(),
            ..owner("A1")
        },
    ] {
        let unrelated = operations.reserve_other(other).unwrap();
        operations.release_other(&unrelated).unwrap();
    }
    // Returning to the original stable owner after any number of account epochs
    // still finds the retained operation; epochs are captured by the async adapter.
    assert_eq!(operations.open(owner("A1"), draft()).unwrap(), lease);
    assert!(operations.blocked(&owner("A1")));
}

#[test]
fn review_is_read_only_single_flight_and_absence_never_releases_reservation() {
    let (mut operations, lease, _) = uncertain();
    let review = operations.review_attempt(&lease).unwrap();
    assert!(operations.snapshot(&lease).unwrap().reviewing);
    assert_eq!(operations.review_attempt(&lease).unwrap_err(), Fault::Busy);
    assert!(!operations.review(&review, None).unwrap());
    assert!(!operations.snapshot(&lease).unwrap().reviewing);
    assert!(operations.blocked(&owner("A1")));
    let review = operations.review_attempt(&lease).unwrap();
    assert!(!operations.review(&review, Some(&envelope(raw()))).unwrap());
    assert_eq!(
        operations.snapshot(&lease).unwrap().phase,
        EditPhase::Uncertain
    );
    let review = operations.review_attempt(&lease).unwrap();
    assert!(operations.review(&review, Some(&desired())).unwrap());
    assert_eq!(
        operations.snapshot(&lease).unwrap().phase,
        EditPhase::Confirmed
    );
    operations.cancel(&lease).unwrap();
    assert!(!operations.blocked(&owner("A1")));
}

#[test]
fn contradictory_ack_and_readback_keep_uncertainty() {
    let (mut operations, lease, attempt) = prepared();
    operations.admit(&attempt, &envelope(raw())).unwrap();
    let mut response = desired();
    response["error"] = json!("conflict");
    assert!(!operations.acknowledge(&attempt, &response).unwrap());
    let review = operations.review_attempt(&lease).unwrap();
    assert!(!operations.review(&review, Some(&response)).unwrap());
    assert_eq!(operations.cancel(&lease), Err(Fault::Retained));
}

#[test]
fn failed_read_only_preflight_returns_to_staged_editor_without_losing_ids() {
    let (mut operations, lease, attempt) = prepared();
    operations
        .preflight_failed(&attempt, "Could not check this draft.".into())
        .unwrap();
    let snapshot = operations.snapshot(&lease).unwrap();
    assert_eq!(snapshot.phase, EditPhase::Editing);
    assert!(snapshot.rows[0].removed);
    assert_eq!(
        snapshot.error.as_deref(),
        Some("Could not check this draft.")
    );
    let fresh = operations.freeze(&lease).unwrap();
    assert_eq!(
        operations.admit(&attempt, &envelope(raw())),
        Err(Fault::Edit(EditError::ForeignAttempt))
    );
    operations.admit(&fresh, &envelope(raw())).unwrap();
}

#[test]
fn metadata_is_exact_id_and_preview_does_not_restore_removed_attachment() {
    let mut operations = Operations::default();
    let lease = operations.open(owner("A1"), draft()).unwrap();
    assert_eq!(operations.snapshot(&lease).unwrap().rows[0].title, "F1");
    assert!(operations.preview(&lease, "F1").unwrap().is_none());
    let file = File {
        id: Some("F1".into()),
        title: Some("Report.pdf".into()),
        size: Some(1024),
        ..Default::default()
    };
    assert_eq!(
        operations.metadata(&lease, "F2", file.clone()),
        Err(Fault::WrongDraft)
    );
    operations.metadata(&lease, "F1", file).unwrap();
    operations.remove(&lease, "F1").unwrap();
    assert_eq!(
        operations
            .preview(&lease, "F1")
            .unwrap()
            .unwrap()
            .id
            .as_deref(),
        Some("F1")
    );
    let snapshot = operations.snapshot(&lease).unwrap();
    assert!(snapshot.rows[0].removed);
    assert_eq!(snapshot.rows[0].title, "Report.pdf");
    assert_eq!(
        snapshot
            .rows
            .iter()
            .map(|row| row.id.as_str())
            .collect::<Vec<_>>(),
        ["F1", "F2"]
    );
}

#[test]
fn admission_requires_exact_credentials_even_for_same_user_and_transport() {
    let captured = super_platinum_core::config::WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: Some("E1".into()),
        user_id: "U1".into(),
        name: "Synthetic workspace".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic-token".into(),
    };
    assert!(same_credentials(&captured, &captured));
    for field in ["team", "user", "token", "url", "enterprise"] {
        let mut changed = captured.clone();
        match field {
            "team" => changed.team_id = "T2".into(),
            "user" => changed.user_id = "U2".into(),
            "token" => changed.token = "synthetic-rotated-token".into(),
            "url" => changed.url = "https://other-fixture.slack.com".into(),
            "enterprise" => changed.enterprise_id = Some("E2".into()),
            _ => unreachable!(),
        }
        assert!(!same_credentials(&captured, &changed), "{field}");
    }
    // Display-only renaming does not change request identity.
    let mut renamed = captured.clone();
    renamed.name = "Renamed fixture".into();
    assert!(same_credentials(&captured, &renamed));
}

#[test]
fn cooldown_blocks_another_read_or_save_without_overflowing_large_retry_after() {
    let (mut operations, lease, _) = uncertain();
    operations.review_cooldown(&lease, u64::MAX).unwrap();
    assert!(operations.snapshot(&lease).unwrap().review_cooling_down);
    assert_eq!(
        operations.review_attempt(&lease).unwrap_err(),
        Fault::CoolingDown
    );
    operations.refresh_cooldown(&lease).unwrap();
    assert_eq!(
        operations.review_attempt(&lease).unwrap_err(),
        Fault::CoolingDown
    );
    operations.review_cooldown(&lease, 0).unwrap();
    operations.refresh_cooldown(&lease).unwrap();
    assert!(!operations.snapshot(&lease).unwrap().review_cooling_down);
    assert!(operations.review_attempt(&lease).is_ok());

    let (mut operations, lease, attempt) = prepared();
    operations
        .preflight_failed(&attempt, "Rate limited before any write.".into())
        .unwrap();
    operations.review_cooldown(&lease, 30).unwrap();
    assert_eq!(operations.freeze(&lease).unwrap_err(), Fault::CoolingDown);
}

#[test]
fn explicit_rejection_keeps_staging_and_requires_a_fresh_attempt() {
    let (mut operations, lease, attempt) = prepared();
    operations.admit(&attempt, &envelope(raw())).unwrap();
    assert!(
        operations
            .rejected(&attempt, &json!({"ok":false,"error":"invalid_auth"}))
            .unwrap()
    );
    let snapshot = operations.snapshot(&lease).unwrap();
    assert_eq!(snapshot.phase, EditPhase::Editing);
    assert!(snapshot.rows[0].removed);
    assert!(snapshot.error.unwrap().contains("authentication"));
    assert!(operations.acknowledge(&attempt, &desired()).is_err());
    let next = operations.freeze(&lease).unwrap();
    operations.admit(&next, &envelope(raw())).unwrap();
    assert!(
        !operations
            .rejected(
                &next,
                &json!({"ok":false,"error":"invalid_auth","draft":null})
            )
            .unwrap()
    );
    assert!(
        !operations
            .acknowledge(
                &next,
                &json!({"ok":false,"error":"invalid_auth","draft":null})
            )
            .unwrap()
    );
    assert_eq!(
        operations.snapshot(&lease).unwrap().phase,
        EditPhase::Uncertain
    );
}

#[test]
fn queued_newer_or_equal_stamp_changed_content_blocks_lagging_preflight_without_rollback() {
    for stamp in ["2", "1", ""] {
        let (mut operations, lease, attempt) = prepared();
        let mut queue = super_platinum_core::scheduled::ScheduledState::default();
        queue.ensure_scope(1, "T1");
        queue.accept_remote(draft());
        let mut newer = draft();
        newer.last_updated_ts = stamp.into();
        newer.blocks = vec![
            json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"New realtime content"}]}]}),
        ];
        let expected_blocks = newer.blocks.clone();
        queue.accept_remote(newer);
        let admission = operations.admit_current(&attempt, &envelope(raw()), &queue);
        assert_eq!(admission, Err(Fault::QueuedRevisionChanged));
        assert_eq!(
            operations.snapshot(&lease).unwrap().phase,
            EditPhase::Conflict
        );
        assert_eq!(queue.items["D1"].blocks, expected_blocks);
        assert_eq!(queue.items["D1"].last_updated_ts, stamp);
        // No mutation may be acknowledged because admission never entered InFlight.
        assert_eq!(
            operations.acknowledge(&attempt, &desired()),
            Err(Fault::Edit(EditError::InvalidPhase))
        );
    }
}

#[test]
fn current_queue_admission_preserves_cancel_and_positive_matching_readback() {
    let (mut operations, lease, attempt) = prepared();
    operations.cancel(&lease).unwrap();
    assert_eq!(
        operations.admit_current(
            &attempt,
            &envelope(raw()),
            &super_platinum_core::scheduled::ScheduledState::default()
        ),
        Err(Fault::StaleLease)
    );
    for present in [true, false] {
        let (mut operations, lease, attempt) = prepared();
        let baseline = draft();
        let mut queue = super_platinum_core::scheduled::ScheduledState::default();
        if present {
            queue.accept_remote(baseline);
        }
        operations
            .admit_current(&attempt, &envelope(raw()), &queue)
            .unwrap();
        assert_eq!(
            operations.snapshot(&lease).unwrap().phase,
            EditPhase::InFlight
        );
    }
}

#[test]
fn explicitly_retired_queue_refuses_old_active_attachment_info() {
    for sent in [true, false] {
        let (mut operations, lease, attempt) = prepared();
        let mut queue = super_platinum_core::scheduled::ScheduledState::default();
        let mut retired = draft();
        retired.is_sent = sent;
        retired.is_deleted = !sent;
        queue.accept_remote(retired);
        assert!(queue.items.is_empty());
        assert_eq!(
            operations.admit_current(&attempt, &envelope(raw()), &queue),
            Err(Fault::QueuedRevisionChanged)
        );
        assert_eq!(
            operations.snapshot(&lease).unwrap().phase,
            EditPhase::Conflict
        );
        assert!(queue.items.is_empty());
    }
}
