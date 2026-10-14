use super::*;
use serde_json::json;

fn owner() -> Owner {
    Owner {
        account: "A1".into(),
        team: "T1".into(),
        user: "U1".into(),
        draft: "D1".into(),
    }
}
fn desired() -> Value {
    json!({"ok":true,"draft":{"id":"D1","client_msg_id":"uuid","date_scheduled":0,
        "is_deleted":false,"is_sent":false,"blocks":[{"type":"rich_text","elements":[]}],
        "file_ids":["F2"],"destinations":[{"channel_id":"C1","thread_ts":"1.2","broadcast":true}]}})
}
fn payload() -> ScheduleMessage {
    ScheduleMessage::from_draft(
        &serde_json::from_value::<ScheduledMessage>(desired()["draft"].clone()).unwrap(),
    )
}
fn running(action: Action) -> (Operations, Lease) {
    let mut operations = Operations::default();
    let lease = operations.reserve_other(owner()).unwrap();
    operations.begin_other(&lease, action).unwrap();
    (operations, lease)
}

#[test]
fn idle_unmount_releases_but_running_unmount_retains_exact_lease() {
    let mut operations = Operations::default();
    let idle = operations.reserve_other(owner()).unwrap();
    operations.detach_other(&idle).unwrap();
    assert!(!operations.blocked(&owner()));
    let running = operations.reserve_other(owner()).unwrap();
    operations.begin_other(&running, Action::Edit).unwrap();
    operations.detach_other(&running).unwrap();
    assert!(operations.blocked(&owner()));
    assert_eq!(operations.release_other(&running), Err(Fault::Retained));
    assert_eq!(
        operations.settle_other(&idle, Outcome::Confirmed),
        Err(Fault::StaleLease)
    );
    operations
        .settle_other(&running, Outcome::NotDispatched)
        .unwrap();
    assert!(!operations.blocked(&owner()));
}

#[test]
fn confirmed_attached_editor_returns_idle_without_replacing_its_local_content() {
    let (mut operations, lease) = running(Action::Edit);
    operations.expect_other(&lease, payload()).unwrap();
    operations.settle_other(&lease, Outcome::Confirmed).unwrap();
    assert_eq!(
        operations.other_snapshot(&lease).unwrap().phase,
        Phase::Idle
    );
    // Registry owns reservation/proof only; the rich editor DOM/request is not
    // assigned or cleared by settlement and can still contain newer local edits.
    assert!(operations.blocked(&owner()));
    operations.begin_other(&lease, Action::Edit).unwrap();
}

#[test]
fn uncertain_cannot_release_or_begin_new_save_and_preserves_expected_payload() {
    let (mut operations, lease) = running(Action::Edit);
    operations.expect_other(&lease, payload()).unwrap();
    assert_eq!(
        operations.expect_other(&lease, payload()),
        Err(Fault::Retained)
    );
    operations.settle_other(&lease, Outcome::Unknown).unwrap();
    operations.detach_other(&lease).unwrap();
    assert_eq!(operations.validate_other(&lease), Err(Fault::Retained));
    assert_eq!(operations.release_other(&lease), Err(Fault::Retained));
    assert_eq!(
        operations.begin_other(&lease, Action::Edit),
        Err(Fault::Retained)
    );
    assert_eq!(
        operations.settle_other(&lease, Outcome::Confirmed),
        Err(Fault::Retained)
    );
    assert!(
        operations
            .other_snapshot(&lease)
            .unwrap()
            .can_confirm_by_readback
    );
}

#[test]
fn exact_positive_edit_readback_releases_detached_reservation() {
    for action in [Action::Edit, Action::Unschedule] {
        let (mut operations, lease) = running(action);
        operations.expect_other(&lease, payload()).unwrap();
        operations.settle_other(&lease, Outcome::Unknown).unwrap();
        operations.detach_other(&lease).unwrap();
        operations.begin_other_review(&lease).unwrap();
        assert_eq!(operations.begin_other_review(&lease), Err(Fault::Retained));
        assert!(
            operations
                .finish_other_review(&lease, Some(&desired()))
                .unwrap()
        );
        assert!(!operations.blocked(&owner()));
    }
}

#[test]
fn partial_contradictory_and_different_rich_readbacks_keep_uncertainty() {
    for variant in ["missing", "error", "files", "blocks", "destination", "id"] {
        let (mut operations, lease) = running(Action::Edit);
        operations.expect_other(&lease, payload()).unwrap();
        operations.settle_other(&lease, Outcome::Unknown).unwrap();
        operations.begin_other_review(&lease).unwrap();
        let mut response = desired();
        match variant {
            "missing" => {
                response["draft"]
                    .as_object_mut()
                    .unwrap()
                    .remove("file_ids");
            }
            "error" => response["error"] = json!("conflict"),
            "files" => response["draft"]["file_ids"] = json!([]),
            "blocks" => response["draft"]["blocks"] = json!([]),
            "destination" => response["draft"]["destinations"][0]["broadcast"] = json!(false),
            "id" => response["draft"]["id"] = json!("D2"),
            _ => unreachable!(),
        }
        assert!(
            !operations
                .finish_other_review(&lease, Some(&response))
                .unwrap(),
            "{variant}"
        );
        assert_eq!(
            operations.other_snapshot(&lease).unwrap().phase,
            Phase::Uncertain
        );
        assert!(!operations.other_snapshot(&lease).unwrap().reviewing);
    }
}

#[test]
fn send_delete_absence_or_positive_flags_are_not_confirmation() {
    for action in [Action::Send, Action::Delete] {
        let (mut operations, lease) = running(action);
        assert_eq!(
            operations.expect_other(&lease, payload()),
            Err(Fault::Retained)
        );
        operations.settle_other(&lease, Outcome::Unknown).unwrap();
        operations.begin_other_review(&lease).unwrap();
        assert!(!operations.finish_other_review(&lease, None).unwrap());
        operations.begin_other_review(&lease).unwrap();
        let mut response = desired();
        response["draft"]["is_deleted"] = json!(true);
        response["draft"]["is_sent"] = json!(true);
        assert!(
            !operations
                .finish_other_review(&lease, Some(&response))
                .unwrap()
        );
        assert!(
            !operations
                .other_snapshot(&lease)
                .unwrap()
                .can_confirm_by_readback
        );
        assert!(operations.blocked(&owner()));
    }
}

#[test]
fn failed_or_rotated_scope_review_clears_checking_without_releasing() {
    let (mut operations, lease) = running(Action::Edit);
    operations.expect_other(&lease, payload()).unwrap();
    operations.settle_other(&lease, Outcome::Unknown).unwrap();
    operations.begin_other_review(&lease).unwrap();
    assert!(!operations.finish_other_review(&lease, None).unwrap());
    assert!(!operations.other_snapshot(&lease).unwrap().reviewing);
    assert!(operations.blocked(&owner()));
    operations.begin_other_review(&lease).unwrap();
}

#[test]
fn review_rate_limit_prevents_new_read_without_releasing_uncertainty() {
    let (mut operations, lease) = running(Action::Unschedule);
    operations.expect_other(&lease, payload()).unwrap();
    operations.settle_other(&lease, Outcome::Unknown).unwrap();
    operations.other_review_cooldown(&lease, u64::MAX).unwrap();
    assert_eq!(
        operations.begin_other_review(&lease),
        Err(Fault::CoolingDown)
    );
    operations.refresh_other_cooldown(&lease).unwrap();
    assert!(operations.other_snapshot(&lease).unwrap().cooling_down);
    operations.other_review_cooldown(&lease, 0).unwrap();
    operations.refresh_other_cooldown(&lease).unwrap();
    assert!(!operations.other_snapshot(&lease).unwrap().cooling_down);
    operations.begin_other_review(&lease).unwrap();
    assert!(operations.blocked(&owner()));
}
