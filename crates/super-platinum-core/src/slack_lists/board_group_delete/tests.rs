use super::*;
fn personal() -> PersonalTaskLists {
    PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap()
}
fn doc() -> Document {
    Document::parse(json!({"id":"F1","mode":"list","access":"write","created":1,"user":"U1","opaque_file":[true,null],"list_metadata":{"opaque":{"keep":7},"schema":[{"id":"C1","key":"status","name":"Status","type":"select","opaque_column":7,"options":{"format":"multi_select","default_value":"legacy","default_value_typed":{"select":["legacy"]},"opaque":{"preserve":true},"choices":[{"value":"legacy","label":"Old","color":"blue","opaque":7},{"value":"other","label":"Other","color":"green"}]}}],"views":[{"id":"V1","name":"Board","type":"board","grouping":{"group_by":"status","order":[{"select":["legacy"]},{"select":["other"]}],"opaque":true},"columns":[],"filters":[],"sorts":[]},{"id":"R1","type":"record","opaque":7},{"id":"I1","type":"canvas_inline"}]}})).unwrap()
}
fn page() -> RecordsPage {
    RecordsPage::parse(json!({"ok":true,"opaque":[null,true],"records":[
 {"id":"Rec1","list_id":"F1","position":"0000000001","fields":[{"key":"status","select":["legacy"],"opaque":1}]},
 {"id":"Rec2","list_id":"F1","position":"0000000002","fields":[{"key":"status","select":["legacy","other"],"opaque":2}]},
 {"id":"Rec3","list_id":"F1","position":"0000000003","fields":[{"key":"status","select":[],"opaque":3}]},
 {"id":"Rec4","list_id":"F1","position":"0000000004","fields":[{"key":"status","select":["other"],"opaque":4}]}],"response_metadata":{"next_cursor":"opaque+/=","unknown":true}})).unwrap()
}
fn context() -> ProjectionContext {
    ProjectionContext {
        editable: true,
        ..Default::default()
    }
}
fn plan() -> BoardGroupDelete {
    BoardGroupDelete::capture(
        &doc(),
        &page(),
        "V1",
        "legacy",
        &context(),
        Some("full"),
        Some(&personal()),
    )
    .unwrap()
}
fn schema(p: &BoardGroupDelete) -> Document {
    let mut raw = p.original().raw.clone();
    raw["list_metadata"]["schema"][0]["options"] = p.options().clone();
    Document::parse(raw).unwrap()
}
fn cells(p: &BoardGroupDelete) -> RecordsPage {
    RecordsPage::parse(p.expected_page.clone()).unwrap()
}
#[test]
fn source_scalar_equality_excludes_multi_value_membership_and_keeps_stale_defaults() {
    let p = plan();
    assert_eq!(
        p.cells(),
        &[json!({"row_id":"Rec1","column_id":"C1","select":[]})]
    );
    assert_eq!(p.options()["default_value"], "legacy");
    assert_eq!(
        p.options()["default_value_typed"],
        json!({"select":["legacy"]})
    );
    let mut expected = doc().raw;
    expected["list_metadata"]["schema"][0]["options"]["choices"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    assert_eq!(schema(&p).raw, expected);
    let mut expected = page().raw;
    expected["records"][0]["fields"][0]["select"] = json!([]);
    assert_eq!(cells(&p).raw, expected);
}
#[test]
fn independent_observations_expose_partial_effects_without_causal_claim() {
    let p = plan();
    assert!(!p.observes_schema(&doc()));
    assert!(!p.observes_cells(&doc(), &page()));
    assert!(p.observes_schema(&schema(&p)));
    assert!(!p.observes_cells(&schema(&p), &page()));
    assert!(p.observes_cells(&doc(), &cells(&p)));
    assert!(!p.observes_schema(&doc()));
}
#[test]
fn empty_projected_matches_have_no_cell_permit_or_empty_http_request() {
    let mut raw = page().raw;
    raw["records"].as_array_mut().unwrap().remove(0);
    let rows = RecordsPage::parse(raw).unwrap();
    let p = BoardGroupDelete::capture(
        &doc(),
        &rows,
        "V1",
        "legacy",
        &context(),
        Some("full"),
        Some(&personal()),
    )
    .unwrap();
    assert!(p.cells().is_empty());
    let mut a = BoardGroupDeleteAttempt::new(p);
    let (_, c) = a
        .admit(&doc(), &rows, &context(), Some("full"), Some(&personal()))
        .unwrap();
    assert!(c.is_none());
    assert_eq!(a.cells_phase(), BoardGroupDeletePhase::NotRequired);
    assert!(!a.finish_cells(false));
}
#[test]
fn both_permits_are_admitted_once_and_finish_independently() {
    let mut a = BoardGroupDeleteAttempt::new(plan());
    let (_, c) = a
        .admit(&doc(), &page(), &context(), Some("full"), Some(&personal()))
        .unwrap();
    assert!(c.is_some());
    assert!(
        a.admit(&doc(), &page(), &context(), Some("full"), Some(&personal()))
            .is_err()
    );
    assert!(a.finish_cells(false));
    assert_eq!(a.schema_phase(), BoardGroupDeletePhase::InFlight);
    assert!(a.finish_schema(true));
    let p = plan();
    a.observe(&schema(&p), &cells(&p));
    assert_eq!(a.cells_phase(), BoardGroupDeletePhase::Observed);
    assert_eq!(a.schema_phase(), BoardGroupDeletePhase::NotRequested);
    assert!(!a.finish_cells(false));
}
#[test]
fn stale_unknown_raw_and_current_permission_changes_refuse_admission() {
    for case in 0..3 {
        let p = plan();
        let mut d = doc();
        let mut rows = page();
        let allow = if case == 2 {
            Some("disabled")
        } else {
            Some("full")
        };
        if case == 0 {
            let mut raw = d.raw;
            raw["opaque_file"] = json!(8);
            d = Document::parse(raw).unwrap();
        }
        if case == 1 {
            let mut raw = rows.raw;
            raw["records"][3]["unknown"] = json!(true);
            rows = RecordsPage::parse(raw).unwrap();
        }
        assert!(
            p.revalidate(&d, &rows, &context(), allow, Some(&personal()))
                .is_err()
        );
    }
}
#[test]
fn malformed_lossy_options_and_hidden_or_missing_groups_refuse() {
    for case in 0..3 {
        let mut raw = doc().raw;
        match case {
            0 => raw["list_metadata"]["schema"][0]["options"]["emojiUrl"] = json!("output-only"),
            1 => {
                raw["list_metadata"]["schema"][0]["options"]["default_value_typed"] =
                    json!({"text":"drop"})
            }
            _ => raw["list_metadata"]["views"][0]["grouping"]["group_by"] = json!("missing"),
        };
        let d = Document::parse(raw).unwrap();
        assert!(
            BoardGroupDelete::capture(
                &d,
                &page(),
                "V1",
                "legacy",
                &context(),
                Some("full"),
                Some(&personal())
            )
            .is_err()
        );
    }
    assert!(
        BoardGroupDelete::capture(
            &doc(),
            &page(),
            "V1",
            "",
            &context(),
            Some("full"),
            Some(&personal())
        )
        .is_err()
    );
}
#[test]
fn normalized_or_foreign_page_is_not_exact_clear_observation() {
    let p = plan();
    for case in 0..3 {
        let mut raw = cells(&p).raw;
        match case {
            0 => raw["records"][0]["date_updated"] = json!(9),
            1 => raw["response_metadata"]["next_cursor"] = json!("changed"),
            _ => raw["records"].as_array_mut().unwrap().reverse(),
        };
        assert!(!p.observes_cells(&schema(&p), &RecordsPage::parse(raw).unwrap()));
    }
    let mut raw = schema(&p).raw;
    raw["user"] = json!("U_OTHER");
    assert!(!p.observes_cells(&Document::parse(raw).unwrap(), &cells(&p)));
}
#[test]
fn builders_emit_exact_separate_source_forms() {
    use crate::slack::{SlackClient, api, client::RequestBody};
    let mut a = BoardGroupDeleteAttempt::new(plan());
    let (s, c) = a
        .admit(&doc(), &page(), &context(), Some("full"), Some(&personal()))
        .unwrap();
    let session = crate::config::WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-token".into(),
    };
    let client = SlackClient::default();
    let schema = api::list_board_group_delete_schema(&client, &session, s);
    let RequestBody::Form(fields) = schema.body else {
        panic!()
    };
    assert_eq!(
        fields.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
        [
            "token",
            "list_id",
            "key",
            "options",
            "backfill_missing_cells_with_default",
            "_x_reason"
        ]
    );
    assert_eq!(fields[4].1, "true");
    assert_eq!(fields[5].1, "update-column");
    let cells = api::list_board_group_delete_cells(&client, &session, c.unwrap());
    let RequestBody::Form(fields) = cells.body else {
        panic!()
    };
    assert_eq!(
        fields,
        vec![
            ("token".into(), "fixture-token".into()),
            ("list_id".into(), "F1".into()),
            (
                "cells".into(),
                json!([{"row_id":"Rec1","column_id":"C1","select":[]}]).to_string()
            ),
            ("_x_reason".into(), "bulk-update-cells".into())
        ]
    );
}

#[test]
fn separator_bearing_legacy_choice_does_not_confuse_scalar_and_lane_membership() {
    let mut raw = doc().raw;
    raw["list_metadata"]["schema"][0]["options"]["choices"]
        .as_array_mut()
        .unwrap()
        .push(json!({"value":"legacy|other","label":"Ambiguous scalar"}));
    let d = Document::parse(raw).unwrap();
    assert!(
        BoardGroupDelete::capture(
            &d,
            &page(),
            "V1",
            "legacy|other",
            &context(),
            Some("full"),
            Some(&personal())
        )
        .is_err()
    );
}

#[test]
fn exact_target_membership_and_aliases_refuse_capture_and_stale_admission() {
    let mut cases = vec![
        ("list_id", json!("F_OTHER")),
        ("list_id", json!(7)),
        ("parent_record_id", json!(7)),
        ("suggested", json!("false")),
        ("is_suggested", json!(0)),
        ("archived", json!("false")),
        ("is_deleted", json!([])),
    ];
    cases.push((
        "alias",
        json!({"key":"other_key","column_id":"C1","select":["legacy"]}),
    ));
    for (key, value) in cases {
        let mut raw = page().raw;
        if key == "alias" {
            raw["records"][0]["fields"]
                .as_array_mut()
                .unwrap()
                .push(value);
        } else {
            raw["records"][0][key] = value;
        }
        let rows = RecordsPage::parse(raw).unwrap();
        assert!(
            BoardGroupDelete::capture(
                &doc(),
                &rows,
                "V1",
                "legacy",
                &context(),
                Some("full"),
                Some(&personal())
            )
            .is_err(),
            "{key}"
        );
        let mut attempt = BoardGroupDeleteAttempt::new(plan());
        assert!(
            attempt
                .admit(&doc(), &rows, &context(), Some("full"), Some(&personal()))
                .is_err()
        );
        assert_eq!(attempt.schema_phase(), BoardGroupDeletePhase::Reviewing);
        assert_eq!(attempt.cells_phase(), BoardGroupDeletePhase::Reviewing);
    }
}

#[test]
fn absent_target_list_identity_uses_page_provenance_and_preserves_unrelated_raw() {
    let mut raw = page().raw;
    raw["records"][0].as_object_mut().unwrap().remove("list_id");
    raw["records"][0]["parent_record_id"] = json!(null);
    raw["records"][0]["suggested"] = json!(false);
    raw["records"][0]["fields"]
        .as_array_mut()
        .unwrap()
        .push(json!({"key":"other_key","column_id":"C_OTHER","opaque":[true,null]}));
    raw["records"][3]["list_id"] = json!("unrelated raw provenance");
    let rows = RecordsPage::parse(raw.clone()).unwrap();
    let p = BoardGroupDelete::capture(
        &doc(),
        &rows,
        "V1",
        "legacy",
        &context(),
        Some("full"),
        Some(&personal()),
    )
    .unwrap();
    let mut attempt = BoardGroupDeleteAttempt::new(p.clone());
    assert!(
        attempt
            .admit(&doc(), &rows, &context(), Some("full"), Some(&personal()))
            .unwrap()
            .1
            .is_some()
    );
    raw["records"][0]["fields"][0]["select"] = json!([]);
    assert_eq!(p.expected_page, raw);
    assert!(p.observes_cells(&schema(&p), &RecordsPage::parse(raw).unwrap()));
}
