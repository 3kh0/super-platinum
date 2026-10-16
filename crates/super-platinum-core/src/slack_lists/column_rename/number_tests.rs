use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{SlackClient, api, client::RequestBody},
};
fn ordinary() -> PersonalTaskLists {
    PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap()
}
fn document(precision: u8) -> Document {
    Document::parse(json!({"id":"F_NUMBER","mode":"list","access":"write","created":1,"user":"U1",
        "list_metadata":{"opaque":{"keep":true},"schema":[
            {"id":"C_NUMBER","key":"estimate","name":"Estimate","type":"number","options":{"precision":precision},"opaque":{"keep":2}},
            {"id":"C_TEXT","key":"title","name":"Title","type":"text","options":{"format":"text"}}
        ],"views":[{"id":"V1","columns":[{"key":"estimate","visible":true}],"opaque":true}]}})).unwrap()
}
fn intent(doc: &Document) -> ColumnRename {
    ColumnRename::capture(
        doc,
        "C_NUMBER",
        "  Planned effort  ",
        Some("full"),
        Some(&ordinary()),
    )
    .unwrap()
}
#[test]
fn all_six_number_renames_preserve_exact_options_and_build_key_not_id_once() {
    for precision in 0..=5 {
        let doc = document(precision);
        let rename = intent(&doc);
        assert_eq!(rename.original(), &doc);
        assert_eq!(rename.unchanged_options(), &json!({"precision":precision}));
        let mut attempt = ColumnRenameAttempt::new(rename);
        let permit = attempt
            .admit(&doc, Some("full"), Some(&ordinary()))
            .unwrap();
        assert!(
            attempt
                .admit(&doc, Some("full"), Some(&ordinary()))
                .is_err()
        );
        let session = WorkspaceSession {
            team_id: "T1".into(),
            user_id: "U1".into(),
            enterprise_id: None,
            name: "Fixture".into(),
            url: "https://example.slack.com".into(),
            token: "fixture-token".into(),
        };
        let request = api::list_column_rename(&SlackClient::default(), &session, permit);
        assert_eq!(
            url::Url::parse(&request.url).unwrap().path(),
            "/api/lists.columns.update"
        );
        assert_eq!(request.method, "POST");
        let RequestBody::Form(fields) = request.body else {
            panic!()
        };
        assert_eq!(
            fields,
            vec![
                ("token".into(), "fixture-token".into()),
                ("list_id".into(), "F_NUMBER".into()),
                ("key".into(), "estimate".into()),
                ("name".into(), "Planned effort".into()),
                ("options".into(), format!("{{\"precision\":{precision}}}")),
                ("backfill_missing_cells_with_default".into(), "true".into()),
                ("_x_reason".into(), "update-column".into())
            ]
        );
    }
}
#[test]
fn number_options_are_exact_and_never_coerced_or_defaulted() {
    for options in [
        Value::Null,
        json!({}),
        json!({"precision":6}),
        json!({"precision":-1}),
        json!({"precision":2.0}),
        json!({"precision":"2"}),
        json!({"precision":true}),
        json!({"precision":2,"default_value":3}),
        json!({"precision":2,"future":true}),
    ] {
        let mut raw = document(2).raw;
        raw["list_metadata"]["schema"][0]["options"] = options;
        let doc = Document::parse(raw).unwrap();
        assert!(!ColumnRename::supports(doc.column("C_NUMBER").unwrap()));
        assert!(
            ColumnRename::capture(&doc, "C_NUMBER", "New", Some("full"), Some(&ordinary()))
                .is_err()
        );
    }
    for kind in ["text", "currency", "unknown"] {
        let mut raw = document(2).raw;
        raw["list_metadata"]["schema"][0]["type"] = json!(kind);
        assert!(
            ColumnRename::capture(
                &Document::parse(raw).unwrap(),
                "C_NUMBER",
                "New",
                Some("full"),
                Some(&ordinary())
            )
            .is_err()
        );
    }
}
#[test]
fn only_full_name_only_readback_observes_number_rename() {
    let before = document(2);
    let rename = intent(&before);
    let mut raw = before.raw.clone();
    raw["list_metadata"]["schema"][0]["name"] = json!("Planned effort");
    let expected = Document::parse(raw.clone()).unwrap();
    assert!(rename.observes_desired(&expected));
    assert!(!rename.observes_desired(&before));
    for kind in ["precision", "unknown", "sibling", "views", "incarnation"] {
        let mut changed = raw.clone();
        match kind {
            "precision" => changed["list_metadata"]["schema"][0]["options"]["precision"] = json!(3),
            "unknown" => {
                changed["list_metadata"]
                    .as_object_mut()
                    .unwrap()
                    .remove("opaque");
            }
            "sibling" => changed["list_metadata"]["schema"][1]["name"] = json!("Changed"),
            "views" => changed["list_metadata"]["views"][0]["opaque"] = json!(false),
            "incarnation" => changed["created"] = json!(2),
            _ => unreachable!(),
        }
        assert!(
            !rename.observes_desired(&Document::parse(changed).unwrap()),
            "{kind}"
        );
    }
    assert_eq!(rename.original(), &before);
}
#[test]
fn number_rename_revalidates_shape_policy_and_typed_raw_consistency() {
    let before = document(2);
    let rename = intent(&before);
    for kind in ["precision", "name", "access", "typed"] {
        let mut changed = before.clone();
        match kind {
            "precision" => {
                changed.raw["list_metadata"]["schema"][0]["options"]["precision"] = json!(3)
            }
            "name" => changed.raw["list_metadata"]["schema"][0]["name"] = json!("New"),
            "access" => changed.raw["access"] = json!("read"),
            "typed" => changed.columns[0].name = "Forged".into(),
            _ => unreachable!(),
        }
        if kind != "typed" {
            changed = Document::parse(changed.raw).unwrap();
        }
        assert!(
            rename
                .revalidate(&changed, Some("full"), Some(&ordinary()))
                .is_err(),
            "{kind}"
        );
    }
    assert!(rename.revalidate(&before, None, Some(&ordinary())).is_err());
    assert!(rename.revalidate(&before, Some("full"), None).is_err());
    let personal = PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":"F_NUMBER"}}),
    )
    .unwrap();
    assert!(
        rename
            .revalidate(&before, Some("full"), Some(&personal))
            .is_err()
    );
}
