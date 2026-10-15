use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{SlackClient, api, client::RequestBody},
};
use serde_json::json;
fn document() -> Document {
    Document::parse(json!({"id":"F1","mode":"list","created":1,"user":"U1","access":"write","list_limits":{"over_view_maximum":false},"list_metadata":{"schema":[{"id":"C1","key":"title","name":"Title","type":"text","options":{"format":"text"}}],"views":[{"id":"V1","name":"Old","type":"table","position":9,"opaque":{"keep":1}},{"id":"V2","name":"Other","type":"board","position":1}],"default_view":"V1","opaque":{"keep":2}}})).unwrap()
}
fn ordinary() -> PersonalTaskLists {
    PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap()
}
fn capture(d: &Document) -> ViewCreate {
    ViewCreate::capture(
        d,
        " Delivery ",
        ViewLayout::Table,
        Some("full"),
        Some(&ordinary()),
    )
    .unwrap()
}
fn fresh() -> Document {
    let mut raw = document().raw;
    raw["list_metadata"]["views"].as_array_mut().unwrap().insert(1,json!({"id":"SERVER_NEW","name":" Delivery ","type":"table","columns":[{"id":"C1","visible":true,"position":0}],"filters":[],"sorts":[],"opaque":{"server":4}}));
    Document::parse(raw).unwrap()
}
#[test]
fn actual_builder_has_only_five_fields_for_both_layouts() {
    for layout in [ViewLayout::Table, ViewLayout::Board] {
        let d = document();
        let i =
            ViewCreate::capture(&d, " Delivery ", layout, Some("full"), Some(&ordinary())).unwrap();
        let mut attempt = ViewCreateAttempt::new(i);
        let p = attempt.admit(&d, Some("full"), Some(&ordinary())).unwrap();
        let session = WorkspaceSession {
            team_id: "T1".into(),
            user_id: "U1".into(),
            enterprise_id: None,
            name: "Fixture".into(),
            url: "https://example.slack.com".into(),
            token: "fixture-token".into(),
        };
        let request = api::list_view_create(&SlackClient::default(), &session, p);
        assert_eq!(
            url::Url::parse(&request.url).unwrap().path(),
            "/api/lists.views.create"
        );
        let RequestBody::Form(fields) = request.body else {
            panic!()
        };
        assert_eq!(
            fields,
            vec![
                ("token".into(), "fixture-token".into()),
                ("list_id".into(), "F1".into()),
                ("name".into(), " Delivery ".into()),
                ("type".into(), layout.as_str().into()),
                ("_x_reason".into(), "create-view".into())
            ]
        );
        assert!(attempt.admit(&d, Some("full"), Some(&ordinary())).is_err());
    }
}
#[test]
fn conservative_limits_policy_name_and_typed_raw_checks() {
    let d = document();
    for limit in [
        None,
        Some(json!(true)),
        Some(Value::Null),
        Some(json!("false")),
    ] {
        let mut raw = d.raw.clone();
        raw["list_limits"]
            .as_object_mut()
            .unwrap()
            .remove("over_view_maximum");
        if let Some(v) = limit {
            raw["list_limits"]["over_view_maximum"] = v;
        }
        let changed = Document::parse(raw).unwrap();
        assert!(
            ViewCreate::capture(
                &changed,
                "New",
                ViewLayout::Table,
                Some("full"),
                Some(&ordinary())
            )
            .is_err()
        );
        assert!(
            capture(&d)
                .revalidate(&changed, Some("full"), Some(&ordinary()))
                .is_err()
        );
    }
    for name in [
        "".into(),
        "  ".into(),
        "x\ny".into(),
        "x".repeat(61),
        "\u{1f98a}".repeat(31),
    ] {
        assert!(
            ViewCreate::capture(
                &d,
                &name,
                ViewLayout::Table,
                Some("full"),
                Some(&ordinary())
            )
            .is_err()
        );
    }
    assert!(
        ViewCreate::capture(
            &d,
            &"\u{1f98a}".repeat(30),
            ViewLayout::Board,
            Some("full"),
            Some(&ordinary())
        )
        .is_ok()
    );
    assert!(ViewCreate::capture(&d, "N", ViewLayout::Table, None, None).is_err());
    let mut forged = d.clone();
    forged.title = "forged".into();
    assert!(
        ViewCreate::capture(
            &forged,
            "N",
            ViewLayout::Table,
            Some("full"),
            Some(&ordinary())
        )
        .is_err()
    );
}
#[test]
fn isolated_addition_preserves_raw_order_not_sorted_catalog_and_unknowns() {
    let i = capture(&document());
    let f = fresh();
    assert_eq!(i.matching_addition(&f), Some("SERVER_NEW"));
    assert_eq!(f.views[1]["opaque"], json!({"server":4}));
    let mut raw = f.raw.clone();
    raw["updated"] = json!(999);
    assert_eq!(
        i.matching_addition(&Document::parse(raw).unwrap()),
        Some("SERVER_NEW")
    );
}
#[test]
fn metadata_changes_ambiguous_additions_and_incarnation_never_claim_id() {
    let i = capture(&document());
    for change in [
        "reorder",
        "sibling",
        "default",
        "schema",
        "opaque",
        "many",
        "wrongname",
        "wrongtype",
        "parent",
        "incarnation",
        "locked",
    ] {
        let mut raw = fresh().raw;
        match change {
            "reorder" => raw["list_metadata"]["views"]
                .as_array_mut()
                .unwrap()
                .swap(0, 2),
            "sibling" => raw["list_metadata"]["views"][0]["name"] = json!("changed"),
            "default" => raw["list_metadata"]["default_view"] = json!("SERVER_NEW"),
            "schema" => raw["list_metadata"]["schema"][0]["name"] = json!("changed"),
            "opaque" => raw["list_metadata"]["opaque"] = Value::Null,
            "many" => raw["list_metadata"]["views"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"SECOND","name":" Delivery ","type":"table"})),
            "wrongname" => raw["list_metadata"]["views"][1]["name"] = json!("Other"),
            "wrongtype" => raw["list_metadata"]["views"][1]["type"] = json!("board"),
            "parent" => raw["id"] = json!("FOREIGN"),
            "incarnation" => raw["created"] = json!(2),
            _ => raw["list_metadata"]["views"][1]["is_locked"] = json!(true),
        }
        assert!(
            i.matching_addition(&Document::parse(raw).unwrap())
                .is_none(),
            "{change}"
        );
    }
    assert!(i.matching_addition(&document()).is_none());
    let mut reused = fresh().raw;
    reused["list_metadata"]["views"][1]["id"] = json!("V1");
    assert!(Document::parse(reused).is_err());
}
#[test]
fn every_completion_is_terminal_for_admission() {
    for unsent in [true, false] {
        let d = document();
        let mut a = ViewCreateAttempt::new(capture(&d));
        a.admit(&d, Some("full"), Some(&ordinary())).unwrap();
        assert!(a.finish(unsent));
        assert!(!a.finish(!unsent));
        assert_eq!(
            a.phase(),
            if unsent {
                ViewCreatePhase::NotRequested
            } else {
                ViewCreatePhase::Unconfirmed
            }
        );
        assert!(a.admit(&d, Some("full"), Some(&ordinary())).is_err());
    }
}
#[test]
fn full_preflight_except_existing_last_read_exception() {
    let d = document();
    let i = capture(&d);
    let mut raw = d.raw.clone();
    raw["last_read"] = json!(55);
    assert!(
        i.revalidate(
            &Document::parse(raw).unwrap(),
            Some("full"),
            Some(&ordinary())
        )
        .is_ok()
    );
    let mut raw = d.raw.clone();
    raw["list_metadata"]["opaque"] = json!({"changed":true});
    assert!(
        i.revalidate(
            &Document::parse(raw).unwrap(),
            Some("full"),
            Some(&ordinary())
        )
        .is_err()
    );
}
