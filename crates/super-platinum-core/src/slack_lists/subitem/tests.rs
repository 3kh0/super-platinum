use super::*;
use serde_json::json;
pub(super) fn fixture() -> (Document, RecordsPage, PersonalTaskLists) {
    let doc=Document::parse(json!({"id":"F_LIST","mode":"list","access":"write","created":1,"user":"U1","list_limits":{"over_row_maximum":false},"old_unknown":9,"list_metadata":{"schema":[{"id":"C_NAME","key":"name","name":"Name","type":"text"},{"id":"C_DONE","key":"done","name":"Done","type":"todo_completed"}],"views":[{"id":"V_DONE","default_view_key":"completed_items"}]}})).unwrap();
    let page=RecordsPage::parse(json!({"ok":true,"records":[row("P",None),row("A",Some("P")),row("B",Some("P")),row("GRAND",Some("A")),row("OTHER",None)],"response_metadata":{"next_cursor":" opaque ","extra":7},"population_extra":{"keep":true}})).unwrap();
    let policy = PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap();
    (doc, page, policy)
}
fn row(id: &str, parent: Option<&str>) -> Value {
    json!({"id":id,"parent_record_id":parent.unwrap_or(""),"date_created":1,"created_by":"U1","fields":[{"key":"name","column_id":"C_NAME","rich_text":[]}],"old_unknown":17})
}
fn parent_read(d: &Document, p: &RecordsPage, children: Vec<Value>) -> Value {
    json!({"ok":true,"list":d.raw,"record":p.record("P").unwrap().raw,"subtasks":children})
}
fn intent(d: &Document, p: &RecordsPage, policy: &PersonalTaskLists) -> SubitemCreationIntent {
    SubitemCreationIntent::capture(d, p, "P", Some("full"), Some(policy), true).unwrap()
}
#[test]
fn only_enabled_ordinary_root_with_known_capacity_can_admit() {
    let (d, p, policy) = fixture();
    for parent in ["A", "missing"] {
        assert!(
            SubitemCreationIntent::capture(&d, &p, parent, Some("full"), Some(&policy), true)
                .is_err()
        );
    }
    assert!(
        SubitemCreationIntent::capture(&d, &p, "P", Some("full"), Some(&policy), false).is_err()
    );
    assert!(SubitemCreationIntent::capture(&d, &p, "P", None, Some(&policy), true).is_err());
    assert!(SubitemCreationIntent::capture(&d, &p, "P", Some("full"), None, true).is_err());
    let mut page = p.raw.clone();
    let records = page["records"].as_array_mut().unwrap();
    for n in 0..47 {
        let mut r = row(&format!("KNOWN{n}"), Some("P"));
        r["archived"] = json!(true);
        records.push(r);
    }
    let page49 = RecordsPage::parse(page.clone()).unwrap();
    assert!(
        SubitemCreationIntent::capture(&d, &page49, "P", Some("full"), Some(&policy), true).is_ok()
    );
    page["records"]
        .as_array_mut()
        .unwrap()
        .push(row("FIFTY", Some("P")));
    assert!(
        SubitemCreationIntent::capture(
            &d,
            &RecordsPage::parse(page).unwrap(),
            "P",
            Some("full"),
            Some(&policy),
            true
        )
        .is_err()
    );
}
#[test]
fn exact_parent_preflight_allows_only_document_last_read_exception() {
    let (d, p, policy) = fixture();
    let i = intent(&d, &p, &policy);
    let mut fresh = d.raw.clone();
    fresh["last_read"] = json!(100);
    let fresh = Document::parse(fresh).unwrap();
    let read =
        SubitemReadback::from_response("F_LIST", "P", &parent_read(&fresh, &p, vec![])).unwrap();
    assert!(
        i.revalidate(&fresh, &read, Some("full"), Some(&policy), true)
            .is_ok()
    );
    let read = SubitemReadback::from_response(
        "F_LIST",
        "P",
        &parent_read(&fresh, &p, vec![row("NEW", Some("P"))]),
    )
    .unwrap();
    assert!(
        i.revalidate(&fresh, &read, Some("full"), Some(&policy), true)
            .is_err()
    );
    let mut raw = parent_read(&fresh, &p, vec![]);
    raw["record"]["old_unknown"] = json!(18);
    assert!(
        i.revalidate(
            &fresh,
            &SubitemReadback::from_response("F_LIST", "P", &raw).unwrap(),
            Some("full"),
            Some(&policy),
            true
        )
        .is_err()
    );
}
#[test]
fn actual_builder_uses_only_exact_parent_and_never_completed_or_lane_seeds() {
    use crate::{
        config::WorkspaceSession,
        slack::{SlackClient, api, client::RequestBody},
    };
    let (d, p, policy) = fixture();
    let i = intent(&d, &p, &policy);
    let read = SubitemReadback::from_response("F_LIST", "P", &parent_read(&d, &p, vec![])).unwrap();
    let mut attempt = SubitemCreateAttempt::new(i);
    let permit = attempt
        .admit(&d, &read, Some("full"), Some(&policy), true)
        .unwrap();
    assert!(
        attempt
            .admit(&d, &read, Some("full"), Some(&policy), true)
            .is_err()
    );
    let w = WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    };
    let timestamp = super::super::ListClock::new()
        .tick(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        )
        .unwrap();
    let req = api::list_subitem_create(&SlackClient::default(), &w, permit, &timestamp);
    let RequestBody::Form(fields) = req.body else {
        panic!("form")
    };
    assert_eq!(
        fields,
        vec![
            ("token".into(), "synthetic".into()),
            ("list_id".into(), "F_LIST".into()),
            ("position".into(), "".into()),
            ("updated_timestamp".into(), timestamp.to_string()),
            ("_x_reason".into(), "create-record".into()),
            ("parent_record_id".into(), "P".into())
        ]
    );
}
#[test]
fn ack_parent_identity_and_full_body_are_not_guessed() {
    let (d, p, policy) = fixture();
    let i = intent(&d, &p, &policy);
    let good = json!({"ok":true,"record":row("NEW",Some("P"))});
    assert!(i.validate_response(&good).is_ok());
    for bad in [Value::Null, json!(""), json!("OTHER"), json!(false)] {
        let mut b = good.clone();
        b["record"]["parent_record_id"] = bad;
        assert!(i.validate_response(&b).is_err());
    }
    let mut b = good.clone();
    b["record"]["id"] = json!("B");
    assert!(i.validate_response(&b).is_err());
    let mut b = good.clone();
    b["error"] = json!("denied");
    assert!(i.validate_response(&b).is_err());
    let mut b = good;
    b["record"]["fields"][0]["row_id"] = json!("FOREIGN");
    assert!(i.validate_response(&b).is_err());
}
#[test]
fn positive_merge_replaces_full_objects_but_preserves_absent_subtree_and_cursor() {
    let (d, p, policy) = fixture();
    let i = intent(&d, &p, &policy);
    let receipt = i
        .validate_response(&json!({"ok":true,"record":row("NEW",Some("P"))}))
        .unwrap();
    let mut raw = parent_read(&d, &p, vec![receipt.raw.clone()]);
    raw["list"].as_object_mut().unwrap().remove("old_unknown");
    raw["list"]["returned_unknown"] = json!([1, 2]);
    raw["record"].as_object_mut().unwrap().remove("old_unknown");
    raw["record"]["returned_unknown"] = json!(true);
    let read = SubitemReadback::from_response("F_LIST", "P", &raw).unwrap();
    let (newdoc, page) = read.merge(&d, &p).unwrap();
    assert!(newdoc.raw.get("old_unknown").is_none());
    assert_eq!(newdoc.raw["returned_unknown"], json!([1, 2]));
    assert!(page.record("P").unwrap().raw.get("old_unknown").is_none());
    for id in ["A", "B", "GRAND", "OTHER"] {
        assert_eq!(page.record(id), p.record(id));
    }
    assert_eq!(page.next_cursor, p.next_cursor);
    assert_eq!(page.raw["response_metadata"], p.raw["response_metadata"]);
    assert_eq!(page.raw["population_extra"], p.raw["population_extra"]);
    assert_eq!(read.confirmed(&i, &receipt).unwrap().id, "NEW");
    let absent =
        SubitemReadback::from_response("F_LIST", "P", &parent_read(&d, &p, vec![])).unwrap();
    assert!(absent.confirmed(&i, &receipt).is_none());
    assert_eq!(i.population(), &p);
    assert_eq!(i.original(), &d);
}
#[test]
fn ambiguous_read_and_oversized_input_cannot_partially_adopt() {
    let (d, p, policy) = fixture();
    let original = p.clone();
    for child in [
        row("P", Some("P")),
        row("A", Some("OTHER")),
        row("OTHER", Some("P")),
    ] {
        let raw = parent_read(&d, &p, vec![child]);
        assert!(
            SubitemReadback::from_response("F_LIST", "P", &raw)
                .and_then(|r| r.merge(&d, &p))
                .is_err()
        );
        assert_eq!(p, original);
    }
    let mut raw = p.raw.clone();
    raw["huge"] = json!("x".repeat(SUBITEM_INPUT_LIMIT));
    let huge = RecordsPage::parse(raw).unwrap();
    assert!(
        SubitemCreationIntent::capture(&d, &huge, "P", Some("full"), Some(&policy), true).is_err()
    );
    assert_eq!(p, original);
}

#[test]
fn retained_arcs_grant_no_authority_to_forged_or_now_denied_snapshots() {
    let (d, p, policy) = fixture();
    let read = SubitemReadback::from_response("F_LIST", "P", &parent_read(&d, &p, vec![])).unwrap();
    let mut forged = d.clone();
    forged.raw["title"] = json!("Unreflected typed title");
    let mut attempt = SubitemCreateAttempt::new(SubitemCreationIntent::retain(
        Arc::new(forged),
        Arc::new(p.clone()),
        "P".into(),
    ));
    assert!(
        attempt
            .admit(&d, &read, Some("full"), Some(&policy), true)
            .is_err()
    );
    for (allow, enabled) in [(Some("none"), true), (Some("full"), false)] {
        let mut attempt = SubitemCreateAttempt::new(intent(&d, &p, &policy));
        assert!(
            attempt
                .admit(&d, &read, allow, Some(&policy), enabled)
                .is_err()
        );
    }
    let mut raw = d.raw.clone();
    raw["list_limits"]["over_row_maximum"] = json!(true);
    let limited = Document::parse(raw).unwrap();
    assert!(
        intent(&d, &p, &policy)
            .revalidate(&limited, &read, Some("full"), Some(&policy), true)
            .is_err()
    );
}

#[test]
fn malformed_parent_lifecycle_and_loaded_cycles_refuse_before_admission() {
    let (d, p, policy) = fixture();
    for (key, value) in [
        ("archived", json!(true)),
        ("is_deleted", json!(true)),
        ("parent_record_id", json!(17)),
    ] {
        let mut raw = p.raw.clone();
        raw["records"][0][key] = value;
        let page = RecordsPage::parse(raw).unwrap();
        assert!(
            SubitemCreationIntent::capture(&d, &page, "P", Some("full"), Some(&policy), true)
                .is_err()
        );
    }
    let mut raw = p.raw.clone();
    raw["records"][1]["parent_record_id"] = json!("GRAND");
    let page = RecordsPage::parse(raw).unwrap();
    assert!(
        SubitemCreationIntent::capture(&d, &page, "P", Some("full"), Some(&policy), true).is_err()
    );
    assert_eq!(p.raw["response_metadata"]["next_cursor"], " opaque ");
}

#[test]
fn complete_population_and_empty_subtasks_preserve_absent_rows_and_incarnation() {
    let (d, p, _) = fixture();
    let mut raw = p.raw.clone();
    raw["response_metadata"]["next_cursor"] = json!("");
    let complete = RecordsPage::parse(raw).unwrap();
    let read =
        SubitemReadback::from_response("F_LIST", "P", &parent_read(&d, &complete, vec![])).unwrap();
    assert_eq!(read.merge(&d, &complete).unwrap().1, complete);
    for target in ["list", "record"] {
        let mut raw = parent_read(&d, &complete, vec![]);
        if target == "list" {
            raw["list"]["created"] = json!(999);
        } else {
            raw["record"]["date_created"] = json!(999);
        }
        let read = SubitemReadback::from_response("F_LIST", "P", &raw).unwrap();
        assert!(read.merge(&d, &complete).is_err());
    }
}
