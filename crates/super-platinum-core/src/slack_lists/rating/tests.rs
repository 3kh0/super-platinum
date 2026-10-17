use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{
        api,
        client::{RequestBody, SlackClient},
    },
    slack_lists::{
        CellEdit, CellEditValue, ColumnCreate, ColumnCreateAttempt, Document, PersonalTaskLists,
        RecordsPage,
    },
};

fn ordinary() -> PersonalTaskLists {
    PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap()
}
fn doc(options: Value) -> Document {
    Document::parse(json!({"id":"F1","mode":"list","user":"U1","access":"write","list_limits":{"over_column_maximum":false},"list_metadata":{"schema":[
        {"id":"C1","key":"NAME","name":"Title","type":"text","is_primary_column":true},
        {"id":"C2","key":"quality","name":"Quality","type":"rating","options":options,"opaque":{"keep":7}}
    ],"views":[{"id":"V1","name":"Custom","type":"table","columns":[{"id":"C1","key":"NAME","visible":true},{"id":"C2","key":"quality","visible":true}],"filters":[],"sorts":[]}],"opaque":{"keep":8}}})).unwrap()
}
fn page(fields: Value) -> RecordsPage {
    RecordsPage::parse(json!({"ok":true,"records":[{"id":"R1","fields":fields,"opaque":{"keep":9}}],"response_metadata":{"next_cursor":"NEXT"}})).unwrap()
}
fn cell(value: Value) -> RecordsPage {
    page(json!([{"key":"quality","column_id":"C2","row_id":"R1","rating":value}]))
}
fn edit(d: &Document, p: &RecordsPage, desired: Option<u8>) -> Result<CellEdit, ListError> {
    CellEdit::capture(
        d,
        p,
        "R1",
        "C2",
        Some("full"),
        Some(&ordinary()),
        CellEditValue::Rating(desired),
    )
}
fn session() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-token".into(),
    }
}

#[test]
fn every_quantity_and_emoji_provenance_uses_actual_permit_and_exact_seven_field_form() {
    let d = doc(RatingOptions::default().wire());
    let p = page(json!([]));
    for max in 1..=5 {
        for (emoji, team) in [
            (":star:", None),
            (":heart:", Some("T1")),
            (":fixture_custom:", Some("T1")),
        ] {
            let options = RatingOptions::capture(emoji, max, team).unwrap();
            let intent = ColumnCreate::capture_rating(
                &d,
                &p,
                "V1",
                " Quality Équipe 東京 ",
                options.clone(),
                "U1",
                Some("full"),
                Some(&ordinary()),
            )
            .unwrap();
            let mut a = ColumnCreateAttempt::new(intent);
            let permit = a
                .admit(&d, &p, "U1", Some("full"), Some(&ordinary()))
                .unwrap();
            assert!(
                a.admit(&d, &p, "U1", Some("full"), Some(&ordinary()))
                    .is_err()
            );
            let request = api::list_column_create(&SlackClient::default(), &session(), permit);
            assert!(!request.retry_safe());
            let RequestBody::Form(fields) = request.body else {
                panic!()
            };
            assert_eq!(
                fields.iter().map(|f| f.0.as_str()).collect::<Vec<_>>(),
                [
                    "token",
                    "list_id",
                    "view_id",
                    "name",
                    "type",
                    "options",
                    "_x_reason"
                ]
            );
            assert_eq!(fields[3].1, "Quality Équipe 東京");
            assert_eq!(fields[4].1, "rating");
            assert_eq!(
                serde_json::from_str::<Value>(&fields[5].1).unwrap(),
                options.wire()
            );
            assert_eq!(
                options.wire().get("emoji_team_id").is_some(),
                team.is_some()
            );
            assert!(a.finish(false));
            assert!(!a.finish(false));
        }
    }
}
#[test]
fn configuration_refuses_unknown_null_noninteger_and_invalid_bounded_drafts() {
    for raw in [
        json!(null),
        json!({}),
        json!({"emoji":":star:","max":0}),
        json!({"emoji":":star:","max":6}),
        json!({"emoji":":star:","max":2.5}),
        json!({"emoji":":star:","max":true}),
        json!({"emoji":"star","max":5}),
        json!({"emoji":":bad name:","max":5}),
        json!({"emoji":":star:","max":5,"emoji_team_id":null}),
        json!({"emoji":":star:","max":5,"emojiTeamId":"T1"}),
        json!({"emoji":":star:","max":5,"emoji_url":"https://fixture.invalid/emoji"}),
    ] {
        assert!(RatingOptions::from_wire(&raw).is_err(), "{raw}");
    }
    assert!(RatingOptions::capture(&format!(":{}:", "a".repeat(255)), 5, None).is_err());
    assert!(RatingOptions::capture(":star:", 5, Some(" T1 ")).is_err());
}
#[test]
fn observation_is_exact_and_preserves_originals_not_server_normalization() {
    let d = doc(RatingOptions::default().wire());
    let p = page(json!([]));
    let options = RatingOptions::capture(":fixture_custom:", 3, Some("T1")).unwrap();
    let intent = ColumnCreate::capture_rating(
        &d,
        &p,
        "V1",
        "Review",
        options.clone(),
        "U1",
        Some("full"),
        Some(&ordinary()),
    )
    .unwrap();
    let mut raw = d.raw.clone();
    raw["list_metadata"]["schema"].as_array_mut().unwrap().push(json!({"id":"SERVER_NEW","key":"server_new","name":"Review","type":"rating","options":options.wire(),"opaque":{"server":1}}));
    raw["list_metadata"]["views"][0]["columns"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"SERVER_NEW","key":"server_new","visible":true}));
    let fresh = Document::parse(raw.clone()).unwrap();
    assert_eq!(
        intent.matching_addition(&fresh, &p).unwrap().id,
        "SERVER_NEW"
    );
    assert_eq!(intent.original(), &d);
    assert_eq!(intent.original_records(), &p);
    raw["list_metadata"]["schema"][2]["options"]["emoji_url"] =
        json!("https://fixture.invalid/custom");
    assert!(
        intent
            .matching_addition(&Document::parse(raw).unwrap(), &p)
            .is_none()
    );
    assert!(
        intent
            .matching_addition(&fresh, &cell(json!([1])))
            .is_none()
    );
    assert!(
        intent
            .revalidate(&d, &p, "U2", Some("full"), Some(&ordinary()))
            .is_err()
    );
}
#[test]
fn missing_and_empty_are_known_empty_without_raw_backfill_and_clear_is_noop() {
    let d = doc(RatingOptions::default().wire());
    for p in [page(json!([])), cell(json!([]))] {
        let original = p.clone();
        assert_eq!(read(d.column("C2").unwrap(), &p.records[0]).unwrap(), None);
        assert!(edit(&d, &p, None).is_err());
        let e = edit(&d, &p, Some(3)).unwrap();
        assert_eq!(
            e.cells(),
            json!([{"row_id":"R1","column_id":"C2","rating":[3]}])
        );
        let request = api::list_cell_update(&SlackClient::default(), &session(), &e);
        assert!(!request.retry_safe());
        let RequestBody::Form(form) = request.body else {
            panic!("exact cell form")
        };
        assert_eq!(
            form.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
            ["token", "list_id", "cells", "_x_reason"]
        );
        assert_eq!(form[2].1, e.cells().to_string());
        assert_eq!(form[3].1, "bulk-update-cells");
        assert_eq!(e.readback_value(&d, &p), Some(CellEditValue::Rating(None)));
        assert!(!e.readback_matches(&d, &p));
        assert!(e.readback_matches(&d, &cell(json!([3]))));
        assert_eq!(p, original);
    }
}
#[test]
fn set_change_and_clear_respect_each_captured_max_and_immutable_schema() {
    for max in 1..=5 {
        let d = doc(RatingOptions::capture(":heart:", max, Some("T1"))
            .unwrap()
            .wire());
        let p = page(json!([]));
        let e = edit(&d, &p, Some(max)).unwrap();
        assert!(edit(&d, &p, Some(max + 1)).is_err());
        assert!(edit(&d, &p, Some(0)).is_err());
        let stored = cell(json!([max]));
        assert!(e.readback_matches(&d, &stored));
        let clear = edit(&d, &stored, None).unwrap();
        assert_eq!(clear.cells()[0]["rating"], json!([]));
        assert!(clear.readback_matches(&d, &cell(json!([]))));
        let changed = doc(RatingOptions::capture(":star:", max, None).unwrap().wire());
        assert!(
            e.revalidate(&changed, &p, Some("full"), Some(&ordinary()))
                .is_err()
        );
    }
}
#[test]
fn malformed_legacy_zero_wrong_member_and_identity_are_not_editable_defaults() {
    let d = doc(RatingOptions::default().wire());
    for raw in [
        json!(null),
        json!(0),
        json!([0]),
        json!([6]),
        json!([-1]),
        json!([1.5]),
        json!([true]),
        json!(["2"]),
        json!([1, 2]),
    ] {
        let p = cell(raw);
        assert!(edit(&d, &p, Some(2)).is_err());
    }
    for fields in [
        json!([{"key":"quality"}]),
        json!([{"key":"other","column_id":"C2","rating":[1]}]),
        json!([{"key":"quality","row_id":"WRONG","rating":[]}]),
        json!([{"key":"quality","number":[2]}]),
    ] {
        assert!(edit(&d, &page(fields), Some(2)).is_err());
    }
    assert!(
        CellEdit::capture(
            &d,
            &page(json!([])),
            "R1",
            "C2",
            Some("full"),
            Some(&ordinary()),
            CellEditValue::Number(Some(2.into()))
        )
        .is_err()
    );
}
