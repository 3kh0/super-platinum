use super::*;
use serde_json::json;

fn envelope(draft: Value) -> Value {
    json!({"ok":true,"draft":draft})
}

fn raw() -> Value {
    json!({
        "id":"D1", "client_msg_id":"client-1", "date_scheduled":1900000000,
        "last_updated_ts":"1700.01", "is_deleted":false, "is_sent":false,
        "blocks":[{"type":"rich_text","block_id":"original", "elements":[
            {"type":"rich_text_section","elements":[
                {"type":"text","text":"Keep \u{65e5}\u{672c}\u{1f680}","style":{"bold":true}},
                {"type":"user","user_id":"U1"}
            ]}, {"type":"future_block","opaque":{"preserve":[1,2,3]}}
        ]}],
        "destinations":[{"channel_id":"C1","thread_ts":"123.456","broadcast":true,
                         "future_destination":{"opaque":true}}, {"channel_id":"C2"}],
        "file_ids":["F1","F2","F3"]
    })
}

fn editor() -> AttachmentEdit {
    AttachmentEdit::new(serde_json::from_value(raw()).unwrap()).unwrap()
}

fn prepared() -> (AttachmentEdit, FrozenAttachmentEdit) {
    let mut edit = editor();
    edit.remove("F2").unwrap();
    let attempt = edit.freeze().unwrap();
    (edit, attempt)
}

fn submitted() -> (AttachmentEdit, FrozenAttachmentEdit) {
    let (mut edit, attempt) = prepared();
    edit.admit_dispatch(&attempt, &envelope(raw())).unwrap();
    (edit, attempt)
}

fn desired(attempt: &FrozenAttachmentEdit) -> Value {
    let mut draft = raw();
    draft["file_ids"] = json!(attempt.request().file_ids);
    draft["last_updated_ts"] = json!("1700.02");
    draft
}

#[test]
fn staging_cancel_and_restore_preserve_original_order_and_full_content() {
    let original = raw();
    let mut edit = editor();
    edit.remove("F1").unwrap();
    edit.remove("F2").unwrap();
    edit.restore("F1").unwrap();
    assert_eq!(edit.file_ids(), ["F1", "F3"]);
    edit.restore("F2").unwrap();
    assert_eq!(edit.file_ids(), ["F1", "F2", "F3"]);
    assert_eq!(edit.freeze().unwrap_err(), EditError::Unchanged);
    edit.remove("F2").unwrap();
    let attempt = edit.freeze().unwrap();
    let payload = attempt.request();
    assert_eq!(payload.file_ids, ["F1", "F3"]);
    assert_eq!(json!(payload.blocks), original["blocks"]);
    assert_eq!(json!(payload.destinations), original["destinations"]);
    assert_eq!(payload.post_at, 1900000000);
    assert_eq!(payload.client_msg_id, "client-1");
    drop(edit); // Cancel before dispatch has no external effect.
    assert_eq!(original, raw());
}

#[test]
fn last_removal_requires_content_and_preserves_explicit_empty_array() {
    let mut edit = editor();
    for id in ["F1", "F2", "F3"] {
        edit.remove(id).unwrap();
    }
    let attempt = edit.freeze().unwrap();
    assert!(attempt.request().file_ids.is_empty());
    edit.admit_dispatch(&attempt, &envelope(raw())).unwrap();
    let mut partial = desired(&attempt);
    partial.as_object_mut().unwrap().remove("file_ids");
    assert!(
        !edit
            .acknowledge(&attempt, &json!({"ok":true,"draft":partial}))
            .unwrap()
    );
    assert!(
        edit.review(&attempt, Some(&envelope(desired(&attempt))))
            .unwrap()
    );
}

#[test]
fn empty_whitespace_or_opaque_only_content_cannot_lose_last_file() {
    for blocks in [
        json!([]),
        json!([{"type":"rich_text","elements":[{"type":"rich_text_section",
            "elements":[{"type":"text","text":" \n\t"}]}]}]),
        json!([{"type":"future_block","text":"not known sendable"}]),
    ] {
        let mut original = raw();
        original["blocks"] = blocks;
        original["file_ids"] = json!(["F1"]);
        let mut edit = AttachmentEdit::new(serde_json::from_value(original).unwrap()).unwrap();
        edit.remove("F1").unwrap();
        assert_eq!(
            edit.freeze().unwrap_err(),
            EditError::EmptyOrUnprovenContent
        );
        assert_eq!(edit.phase(), EditPhase::Editing);
        edit.restore("F1").unwrap();
    }
}

#[test]
fn unknown_or_duplicate_file_identity_is_rejected() {
    let mut edit = editor();
    assert_eq!(edit.remove("F4"), Err(EditError::UnknownFile));
    assert_eq!(edit.restore("F4"), Err(EditError::UnknownFile));
    edit.remove("F2").unwrap();
    assert_eq!(edit.remove("F2"), Err(EditError::UnknownFile));
    for ids in [json!(["F1", "F1"]), json!([""])] {
        let mut original = raw();
        original["file_ids"] = ids;
        assert_eq!(
            AttachmentEdit::new(serde_json::from_value(original).unwrap()).unwrap_err(),
            EditError::InvalidBaseline
        );
    }
}

#[test]
fn immutable_attempt_and_foreign_or_abandoned_receipt_cannot_dispatch() {
    let (mut edit, attempt) = prepared();
    let (_, foreign) = prepared();
    assert_eq!(
        edit.admit_dispatch(&foreign, &envelope(raw())),
        Err(EditError::ForeignAttempt)
    );
    let mut changed = attempt.request();
    changed.blocks.clear();
    changed.destinations.clear();
    changed.file_ids.clear();
    changed.post_at = 0;
    assert!(!attempt.request().blocks.is_empty());
    assert_eq!(attempt.request().post_at, 1900000000);
    assert_eq!(edit.remove("F1"), Err(EditError::InvalidPhase));
    edit.abandon_preflight(&attempt).unwrap();
    let newer = edit.freeze().unwrap();
    assert_eq!(
        edit.admit_dispatch(&attempt, &envelope(raw())),
        Err(EditError::ForeignAttempt)
    );
    edit.admit_dispatch(&newer, &envelope(raw())).unwrap();
    assert_eq!(
        edit.admit_dispatch(&newer, &envelope(raw())),
        Err(EditError::InvalidPhase)
    );
    assert_eq!(edit.abandon_preflight(&newer), Err(EditError::InvalidPhase));
}

#[test]
fn same_clock_full_payload_mismatch_is_conflict_and_keeps_staged_removal() {
    for (key, value) in [
        ("blocks", json!([])),
        ("destinations", json!([{"channel_id":"C9"}])),
        ("file_ids", json!(["F3", "F2", "F1"])),
        ("date_scheduled", json!(0)),
        ("client_msg_id", json!("another")),
        ("id", json!("D2")),
        ("is_deleted", json!(true)),
        ("is_sent", json!(true)),
        ("last_updated_ts", json!("1700.02")),
    ] {
        let (mut edit, attempt) = prepared();
        let mut current = raw();
        current[key] = value;
        assert_eq!(
            edit.admit_dispatch(&attempt, &envelope(current)),
            Err(EditError::PreflightMismatch),
            "{key}"
        );
        assert_eq!(edit.phase(), EditPhase::Conflict);
        assert_eq!(edit.file_ids(), ["F1", "F3"]);
        assert_eq!(edit.freeze().unwrap_err(), EditError::InvalidPhase);
    }
}

#[test]
fn preflight_missing_defaultable_fields_fails_closed() {
    for key in [
        "file_ids",
        "blocks",
        "destinations",
        "date_scheduled",
        "is_sent",
        "is_deleted",
        "last_updated_ts",
    ] {
        let (mut edit, attempt) = prepared();
        let mut current = raw();
        current.as_object_mut().unwrap().remove(key);
        assert_eq!(
            edit.admit_dispatch(&attempt, &envelope(current)),
            Err(EditError::PreflightMismatch),
            "{key}"
        );
    }
}

#[test]
fn exact_acknowledgement_confirms_once_with_string_epoch() {
    let (mut edit, attempt) = submitted();
    let mut draft = desired(&attempt);
    draft["date_scheduled"] = json!("1900000000");
    assert!(
        edit.acknowledge(&attempt, &json!({"ok":true,"draft":draft}))
            .unwrap()
    );
    assert_eq!(edit.phase(), EditPhase::Confirmed);
    assert_eq!(edit.freeze().unwrap_err(), EditError::InvalidPhase);
    assert_eq!(edit.mark_uncertain(&attempt), Err(EditError::InvalidPhase));
}

#[test]
fn partial_acknowledgements_never_exploit_deserializer_defaults() {
    for key in [
        "id",
        "client_msg_id",
        "file_ids",
        "blocks",
        "destinations",
        "date_scheduled",
        "is_sent",
        "is_deleted",
    ] {
        let (mut edit, attempt) = submitted();
        let mut draft = desired(&attempt);
        draft.as_object_mut().unwrap().remove(key);
        assert!(
            !edit
                .acknowledge(&attempt, &json!({"ok":true,"draft":draft}))
                .unwrap(),
            "{key}"
        );
        assert_eq!(edit.phase(), EditPhase::Uncertain);
        assert_eq!(edit.freeze().unwrap_err(), EditError::InvalidPhase);
    }
}

#[test]
fn contradictory_or_error_acknowledgements_latch_uncertainty() {
    let (mut edit, attempt) = submitted();
    assert!(
        !edit
            .acknowledge(&attempt, &json!({"ok":false,"error":"unknown"}))
            .unwrap()
    );
    assert_eq!(edit.phase(), EditPhase::Uncertain);
    for (key, value) in [
        ("id", json!("D2")),
        ("client_msg_id", json!("other")),
        ("blocks", json!([])),
        ("destinations", json!([{"channel_id":"C1"}])),
        ("file_ids", json!(["F3", "F1"])),
        ("date_scheduled", json!(0)),
        ("is_sent", json!(true)),
        ("is_deleted", json!(true)),
    ] {
        let (mut edit, attempt) = submitted();
        let mut draft = desired(&attempt);
        draft[key] = value;
        assert!(
            !edit
                .acknowledge(&attempt, &json!({"ok":true,"draft":draft}))
                .unwrap(),
            "{key}"
        );
        assert!(!edit.review(&attempt, Some(&envelope(draft))).unwrap());
        assert_eq!(edit.phase(), EditPhase::Uncertain);
    }
}

#[test]
fn uncertain_read_only_review_requires_positive_exact_full_payload() {
    let (mut edit, attempt) = submitted();
    edit.mark_uncertain(&attempt).unwrap();
    assert!(!edit.review(&attempt, None).unwrap());
    assert!(!edit.review(&attempt, Some(&envelope(raw()))).unwrap());
    assert_eq!(edit.remove("F1"), Err(EditError::InvalidPhase));
    assert_eq!(edit.restore("F2"), Err(EditError::InvalidPhase));
    assert_eq!(
        edit.abandon_preflight(&attempt),
        Err(EditError::InvalidPhase)
    );
    assert_eq!(
        edit.admit_dispatch(&attempt, &envelope(raw())),
        Err(EditError::InvalidPhase)
    );
    assert!(
        edit.review(&attempt, Some(&envelope(desired(&attempt))))
            .unwrap()
    );
    assert_eq!(edit.phase(), EditPhase::Confirmed);
}

#[test]
fn unscheduled_draft_remains_unscheduled_with_exact_rich_payload() {
    let mut original = raw();
    original["date_scheduled"] = json!(0);
    let mut edit = AttachmentEdit::new(serde_json::from_value(original.clone()).unwrap()).unwrap();
    edit.remove("F2").unwrap();
    let attempt = edit.freeze().unwrap();
    assert_eq!(attempt.request().post_at, 0);
    edit.admit_dispatch(&attempt, &envelope(original.clone()))
        .unwrap();
    original["file_ids"] = json!(["F1", "F3"]);
    assert!(
        edit.acknowledge(&attempt, &json!({"ok":true,"draft":original}))
            .unwrap()
    );
}

#[test]
fn contradictory_or_malformed_envelopes_never_admit_or_confirm() {
    for error in [
        json!("conflict"),
        json!(false),
        json!({}),
        json!([]),
        json!(0),
    ] {
        let (mut edit, attempt) = prepared();
        let mut response = envelope(raw());
        response["error"] = error.clone();
        assert_eq!(
            edit.admit_dispatch(&attempt, &response),
            Err(EditError::PreflightMismatch)
        );

        let (mut edit, attempt) = submitted();
        let mut response = envelope(desired(&attempt));
        response["error"] = error;
        assert!(!edit.acknowledge(&attempt, &response).unwrap());
        assert!(!edit.review(&attempt, Some(&response)).unwrap());
        assert_eq!(edit.phase(), EditPhase::Uncertain);
    }

    for response in [
        json!({"ok":true}),
        json!({"ok":true,"draft":null}),
        json!({"ok":true,"draft":[]}),
        json!({"ok":true,"draft":"draft"}),
        json!({"ok":"true","draft":raw()}),
        json!({"draft":raw()}),
        raw(),
    ] {
        let (mut edit, attempt) = prepared();
        assert_eq!(
            edit.admit_dispatch(&attempt, &response),
            Err(EditError::PreflightMismatch)
        );
        let (mut edit, attempt) = submitted();
        assert!(!edit.acknowledge(&attempt, &response).unwrap());
        assert!(!edit.review(&attempt, Some(&response)).unwrap());
        assert_eq!(edit.phase(), EditPhase::Uncertain);
    }
}

#[test]
fn explicit_null_error_is_accepted() {
    let (mut edit, attempt) = prepared();
    let mut response = envelope(raw());
    response["error"] = Value::Null;
    edit.admit_dispatch(&attempt, &response).unwrap();
    let mut response = envelope(desired(&attempt));
    response["error"] = Value::Null;
    assert!(edit.acknowledge(&attempt, &response).unwrap());

    let (mut edit, attempt) = submitted();
    edit.mark_uncertain(&attempt).unwrap();
    assert!(edit.review(&attempt, Some(&response)).unwrap());
}

#[test]
fn proven_not_dispatched_preserves_staging_and_retires_attempt() {
    let (mut edit, attempt) = submitted();
    edit.not_dispatched(&attempt).unwrap();
    assert_eq!(edit.phase(), EditPhase::Editing);
    assert_eq!(edit.file_ids(), ["F1", "F3"]);
    assert_eq!(
        edit.not_dispatched(&attempt),
        Err(EditError::ForeignAttempt)
    );
    let replacement = edit.freeze().unwrap();
    assert_eq!(
        edit.admit_dispatch(&attempt, &envelope(raw())),
        Err(EditError::ForeignAttempt)
    );
    edit.admit_dispatch(&replacement, &envelope(raw())).unwrap();
    edit.mark_uncertain(&replacement).unwrap();
    assert_eq!(
        edit.not_dispatched(&replacement),
        Err(EditError::InvalidPhase)
    );
}

#[test]
fn remote_error_matching_unsent_marker_is_still_uncertain() {
    let (mut edit, attempt) = submitted();
    assert!(
        !edit
            .acknowledge(
                &attempt,
                &json!({"ok":false,"error":"draft_mutation_not_sent_offline"})
            )
            .unwrap()
    );
    assert_eq!(edit.phase(), EditPhase::Uncertain);
    assert_eq!(edit.not_dispatched(&attempt), Err(EditError::InvalidPhase));
}

#[test]
fn explicit_rejection_preserves_staging_and_retires_only_exact_inflight_attempt() {
    for code in [
        "time_in_past",
        "time_too_far",
        "invalid_auth",
        "not_authed",
        "token_revoked",
        "token_expired",
        "not_allowed_token_type",
        "account_inactive",
    ] {
        let (mut edit, attempt) = submitted();
        let response = json!({"ok":false,"error":code});
        let (_, foreign) = submitted();
        assert_eq!(
            edit.rejected(&foreign, &response),
            Err(EditError::ForeignAttempt)
        );
        assert!(edit.rejected(&attempt, &response).unwrap());
        assert_eq!(edit.phase(), EditPhase::Editing);
        assert_eq!(edit.file_ids(), ["F1", "F3"]);
        assert_eq!(
            edit.rejected(&attempt, &response),
            Err(EditError::ForeignAttempt)
        );
        let next = edit.freeze().unwrap();
        edit.admit_dispatch(&next, &envelope(raw())).unwrap();
        edit.mark_uncertain(&next).unwrap();
        assert_eq!(
            edit.rejected(&next, &response),
            Err(EditError::InvalidPhase)
        );
    }
}

#[test]
fn ambiguous_or_contradictory_rejection_never_unlocks_attempt() {
    let mut responses = vec![
        json!({"ok":true,"error":"invalid_auth"}),
        json!({"error":"invalid_auth"}),
        json!({"ok":false,"error":null}),
    ];
    for code in [
        "internal_error",
        "fatal_error",
        "ratelimited",
        "draft_mutation_not_sent_offline",
        "unknown",
    ] {
        responses.push(json!({"ok":false,"error":code}));
    }
    for key in [
        "draft",
        "ts",
        "file_msg_ts",
        "scheduled_message_id",
        "message",
        "new_receipt",
    ] {
        let mut response = json!({"ok":false,"error":"time_in_past"});
        response[key] = Value::Null;
        responses.push(response);
    }
    for response in responses {
        let (mut edit, attempt) = submitted();
        assert!(!edit.rejected(&attempt, &response).unwrap());
        assert_eq!(edit.phase(), EditPhase::InFlight);
        assert!(!edit.acknowledge(&attempt, &response).unwrap());
        assert_eq!(edit.phase(), EditPhase::Uncertain);
    }
}

#[test]
fn legacy_baseline_accepts_only_absent_or_empty_optional_metadata() {
    let mut original = raw();
    original.as_object_mut().unwrap().remove("client_msg_id");
    original.as_object_mut().unwrap().remove("last_updated_ts");
    let expected: ScheduledMessage = serde_json::from_value(original.clone()).unwrap();
    assert!(matches_reviewed(&envelope(original.clone()), &expected));
    original["client_msg_id"] = json!("");
    original["last_updated_ts"] = json!("");
    assert!(matches_reviewed(&envelope(original.clone()), &expected));
    for key in ["client_msg_id", "last_updated_ts"] {
        for invalid in [Value::Null, json!(3), json!("new-identity-or-revision")] {
            let mut changed = original.clone();
            changed[key] = invalid;
            assert!(!matches_reviewed(&envelope(changed), &expected));
        }
    }
    assert_eq!(
        AttachmentEdit::new(expected).unwrap_err(),
        EditError::InvalidBaseline
    );
}

#[test]
fn complete_legacy_readback_refuses_defaulted_content_but_can_refresh_retired_rows() {
    let original = raw();
    let expected: ScheduledMessage = serde_json::from_value(original.clone()).unwrap();
    for key in [
        "id",
        "blocks",
        "destinations",
        "file_ids",
        "date_scheduled",
        "is_deleted",
        "is_sent",
    ] {
        let mut partial = original.clone();
        partial.as_object_mut().unwrap().remove(key);
        assert!(
            complete_readback(&envelope(partial.clone()), "D1").is_none(),
            "{key}"
        );
        assert!(!matches_reviewed(&envelope(partial), &expected));
    }
    for key in ["is_deleted", "is_sent"] {
        let mut retired = original.clone();
        retired[key] = json!(true);
        assert!(
            !complete_readback(&envelope(retired.clone()), "D1")
                .unwrap()
                .active()
        );
        assert!(!matches_reviewed(&envelope(retired), &expected));
    }
    let mut changed = original.clone();
    changed["blocks"] = json!([]); // Same stamp is not a content proof.
    assert!(!matches_reviewed(&envelope(changed), &expected));
}

#[test]
fn legacy_update_acknowledgement_preserves_absent_identity_without_weakening_known_identity() {
    let mut original = raw();
    original.as_object_mut().unwrap().remove("client_msg_id");
    let expected: ScheduledMessage = serde_json::from_value(original.clone()).unwrap();
    let payload = ScheduleMessage::from_draft(&expected);
    assert!(confirms_submission(
        &envelope(original.clone()),
        "D1",
        &payload
    ));
    for value in [Value::Null, json!(false), json!("unrelated")] {
        let mut malformed = original.clone();
        malformed["client_msg_id"] = value;
        assert!(!confirms_submission(&envelope(malformed), "D1", &payload));
    }
    let known = ScheduleMessage::from_draft(&serde_json::from_value(raw()).unwrap());
    assert!(!confirms_submission(&envelope(original), "D1", &known));
}

#[test]
fn independently_known_stale_preflight_requires_exact_prepared_attempt() {
    let (mut edit, attempt) = prepared();
    let (_, foreign) = prepared();
    assert_eq!(
        edit.reject_stale_preflight(&foreign),
        Err(EditError::ForeignAttempt)
    );
    assert_eq!(edit.phase(), EditPhase::Prepared);
    edit.reject_stale_preflight(&attempt).unwrap();
    assert_eq!(edit.phase(), EditPhase::Conflict);
    assert_eq!(
        edit.admit_dispatch(&attempt, &envelope(raw())),
        Err(EditError::InvalidPhase)
    );
}

#[test]
fn queue_admission_distinguishes_absence_from_explicit_retirement_and_compares_full_revision() {
    let expected: ScheduledMessage = serde_json::from_value(raw()).unwrap();
    let mut queue = crate::scheduled::ScheduledState::default();
    assert!(queue.review_is_current(&expected));
    queue.accept_remote(expected.clone());
    assert!(queue.review_is_current(&expected));
    let mut changed = expected.clone();
    changed.blocks.clear(); // Same stamp is still changed known content.
    queue.accept_remote(changed);
    assert!(!queue.review_is_current(&expected));
    for sent in [true, false] {
        let mut queue = crate::scheduled::ScheduledState::default();
        let mut retired = expected.clone();
        retired.is_sent = sent;
        retired.is_deleted = !sent;
        queue.accept_remote(retired);
        assert!(queue.items.is_empty());
        assert!(!queue.review_is_current(&expected));
    }
}
