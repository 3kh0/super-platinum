use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{SlackClient, api, client::RequestBody},
};
fn document() -> Document {
    Document::parse(json!({"id":"F1","mode":"list","title":"Plan","created":1,"user":"U1","access":"write","list_metadata":{"schema":[{"id":"C1","key":"title","name":"Title","type":"text","options":{"format":"text"}}],"views":[{"id":"V1","name":"Custom","type":"table","grouping":{"column_id":"C1","future":{"keep":1}},"columns":["C1"],"filters":[],"sorts":[],"opaque":{"keep":2}},{"id":"V2","name":"Other","type":"board"}],"default_view":"V1","opaque":{"keep":3}}})).unwrap()
}
fn ordinary() -> PersonalTaskLists {
    PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap()
}
fn capture(doc: &Document) -> Result<ViewRename, ListError> {
    ViewRename::capture(doc, "V1", " Delivery ", Some("full"), Some(&ordinary()))
}
fn fresh() -> Document {
    let mut raw = document().raw;
    raw["list_metadata"]["views"][0]["name"] = json!(" Delivery ");
    Document::parse(raw).unwrap()
}
#[test]
fn exact_view_id_type_grouping_form_preserves_name_spaces() {
    let doc = document();
    let intent = capture(&doc).unwrap();
    let mut attempt = ViewRenameAttempt::new(intent);
    let session = WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://example.slack.com".into(),
        token: "fixture-token".into(),
    };
    let request = api::list_view_rename(
        &SlackClient::default(),
        &session,
        attempt
            .admit(&doc, Some("full"), Some(&ordinary()))
            .unwrap(),
    );
    assert_eq!(request.method, "POST");
    assert_eq!(
        url::Url::parse(&request.url).unwrap().path(),
        "/api/lists.views.update"
    );
    let RequestBody::Form(fields) = request.body else {
        panic!()
    };
    assert_eq!(
        fields,
        vec![
            ("token".into(), "fixture-token".into()),
            ("list_id".into(), "F1".into()),
            ("id".into(), "V1".into()),
            ("name".into(), " Delivery ".into()),
            ("type".into(), "table".into()),
            ("grouping".into(), doc.views[0]["grouping"].to_string()),
            ("_x_reason".into(), "update-view".into())
        ]
    );
    assert!(
        attempt
            .admit(&doc, Some("full"), Some(&ordinary()))
            .is_err()
    );
    assert!(attempt.finish(false));
    assert!(!attempt.finish(true));
    assert!(attempt.observe(&fresh()));
}
#[test]
fn absent_null_and_empty_grouping_and_both_layouts_are_preserved() {
    for kind in ["table", "board"] {
        for grouping in [None, Some(Value::Null), Some(json!({}))] {
            let mut raw = document().raw;
            raw["list_metadata"]["views"][0]["type"] = json!(kind);
            let target = raw["list_metadata"]["views"][0].as_object_mut().unwrap();
            target.remove("grouping");
            if let Some(g) = grouping.clone() {
                target.insert("grouping".into(), g);
            }
            let d = Document::parse(raw).unwrap();
            let i = capture(&d).unwrap();
            assert_eq!(i.kind(), kind);
            assert_eq!(i.grouping(), grouping.as_ref().filter(|v| !v.is_null()));
        }
    }
}
#[test]
fn builtins_locked_unknown_types_and_ambiguous_ids_refuse() {
    for (key, value) in [
        ("default_view_key", json!("all_items")),
        ("default_view_key", json!("future")),
        ("is_all_items_view", json!(true)),
        ("is_locked", json!(true)),
        ("is_locked", Value::Null),
        ("type", json!("calendar")),
        ("grouping", json!([])),
    ] {
        let mut raw = document().raw;
        raw["list_metadata"]["views"][0][key] = value;
        assert!(capture(&Document::parse(raw).unwrap()).is_err(), "{key}");
    }
    let mut raw = document().raw;
    raw["list_metadata"]["views"][1]["id"] = json!("V1");
    assert!(Document::parse(raw).is_err());
}
#[test]
fn conservative_name_bound_and_missing_policy_never_admit() {
    let d = document();
    assert!(
        ViewRename::capture(
            &d,
            "V1",
            &"\u{1f98a}".repeat(30),
            Some("full"),
            Some(&ordinary())
        )
        .is_ok()
    );
    for name in [
        "".to_owned(),
        "  ".into(),
        "line\nname".into(),
        "x".repeat(61),
        "\u{1f98a}".repeat(31),
        "Custom".into(),
    ] {
        assert!(ViewRename::capture(&d, "V1", &name, Some("full"), Some(&ordinary())).is_err());
    }
    assert!(ViewRename::capture(&d, "V1", "New", Some("full"), None).is_err());
    assert!(ViewRename::capture(&d, "V1", "New", Some("disabled"), Some(&ordinary())).is_err());
}
#[test]
fn full_raw_baseline_and_unknown_metadata_are_not_name_only_checks() {
    let d = document();
    let i = capture(&d).unwrap();
    for field in ["grouping", "filters", "opaque"] {
        let mut raw = d.raw.clone();
        raw["list_metadata"]["views"][0][field] = json!({"changed":true});
        let changed = Document::parse(raw).unwrap();
        assert!(
            i.revalidate(&changed, Some("full"), Some(&ordinary()))
                .is_err()
        );
    }
    let mut mismatch = d.clone();
    mismatch.title = "forged".into();
    assert!(capture(&mismatch).is_err());
    let mut same = d.raw.clone();
    same["last_read"] = json!(999);
    assert!(
        i.revalidate(
            &Document::parse(same).unwrap(),
            Some("full"),
            Some(&ordinary())
        )
        .is_ok()
    );
}
#[test]
fn readback_requires_complete_metadata_identity_and_only_desired_name_change() {
    let i = capture(&document()).unwrap();
    assert!(i.observes_desired(&fresh()));
    for change in [
        "sibling",
        "schema",
        "grouping",
        "opaque",
        "removed",
        "incarnation",
    ] {
        let mut raw = fresh().raw;
        match change {
            "sibling" => raw["list_metadata"]["views"][1]["name"] = json!("Other change"),
            "schema" => raw["list_metadata"]["schema"][0]["name"] = json!("Changed"),
            "grouping" => raw["list_metadata"]["views"][0]["grouping"] = json!({}),
            "opaque" => {
                raw["list_metadata"]
                    .as_object_mut()
                    .unwrap()
                    .remove("opaque");
            }
            "removed" => {
                raw["list_metadata"]["views"]
                    .as_array_mut()
                    .unwrap()
                    .remove(0);
            }
            _ => raw["created"] = json!(2),
        }
        assert!(
            !i.observes_desired(&Document::parse(raw).unwrap()),
            "{change}"
        );
    }
}
#[test]
fn received_error_stays_uncertain_and_not_requested_never_rearms() {
    for not_requested in [true, false] {
        let d = document();
        let mut a = ViewRenameAttempt::new(capture(&d).unwrap());
        a.admit(&d, Some("full"), Some(&ordinary())).unwrap();
        a.finish(not_requested);
        assert!(a.admit(&d, Some("full"), Some(&ordinary())).is_err());
        assert_eq!(
            a.phase(),
            if not_requested {
                ViewRenamePhase::NotRequested
            } else {
                ViewRenamePhase::Unconfirmed
            }
        );
        assert_eq!(a.observe(&fresh()), !not_requested);
    }
}
#[test]
fn column_and_view_intents_cannot_observe_each_others_changes() {
    let d = document();
    let c =
        super::super::ColumnRename::capture(&d, "C1", "New field", Some("full"), Some(&ordinary()))
            .unwrap();
    assert!(!c.observes_desired(&fresh()));
    let mut raw = d.raw.clone();
    raw["list_metadata"]["schema"][0]["name"] = json!("New field");
    assert!(
        !capture(&d)
            .unwrap()
            .observes_desired(&Document::parse(raw).unwrap())
    );
}
