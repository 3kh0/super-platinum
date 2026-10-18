use super::*;
fn ids() -> Vec<String> {
    vec!["C00000001".into(), "G00000002".into()]
}
#[test]
fn conditional_and_unconditional_requests_are_closed_and_membership_checked() {
    let plain = Read::capture(ids(), None).unwrap();
    assert_eq!(plain.body(), json!({"ids":ids(),"check_membership":true}));
    let versions = BTreeMap::from([("C00000001".into(), 0), ("G00000002".into(), 42)]);
    let conditional = Read::capture(ids(), Some(versions.clone())).unwrap();
    assert_eq!(
        conditional.body(),
        json!({"updated_ids":versions,"check_membership":true})
    );
    let mut wire = conditional.body();
    wire["token"] = json!("synthetic");
    assert!(valid_wire_body(&wire));
    wire["ids"] = json!(ids());
    assert!(!valid_wire_body(&wire));
    assert!(Read::capture(ids(), Some(BTreeMap::new())).is_err());
    assert!(Read::capture(vec!["C00000001".into(); 101], None).is_err());
    assert!(Read::capture(vec!["USER00001".into()], None).is_err());
}
#[test]
fn result_object_requires_same_byte_keys_and_preserves_flattened_order() {
    let read = Read::capture(ids(), None).unwrap();
    let raw = json!({"results":{"z":[{"id":"C00000001"}],"2":[{"id":"G00000002"}]}});
    assert!(Page::decode(&read, raw.clone(), None).is_err());
    let page = Page::decode(&read, raw.clone(), Some(vec!["2".into(), "z".into()])).unwrap();
    assert_eq!(
        page.rows()
            .iter()
            .map(|v| v["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["G00000002", "C00000001"]
    );
    assert_eq!(page.raw(), &raw);
    assert!(Page::decode(&read, raw, Some(vec!["z".into(), "z".into()])).is_err());
}
#[test]
fn flannel_absent_ok_is_valid_but_contradictory_error_or_unrelated_ids_refuse() {
    let read = Read::capture(ids(), None).unwrap();
    assert!(Page::decode(&read, json!({"results":[]}), None).is_ok());
    for error in [json!("bad"), json!(false), json!(0), json!({})] {
        assert!(Page::decode(&read, json!({"ok":true,"error":error,"results":[]}), None).is_err());
    }
    assert!(Page::decode(&read, json!({"ok":false}), None).is_err());
    assert!(Page::decode(&read, json!({"results":[{"id":"C00000099"}]}), None).is_err());
    assert!(Page::decode(&read, json!({"failed_ids":["C00000099"]}), None).is_err());
}
#[test]
fn failed_invalid_pending_and_memberships_stay_distinct_without_coverage_inference() {
    let read = Read::capture(ids(), None).unwrap();
    let page=Page::decode(&read,json!({"failed_ids":["C00000001"],"invalid_ids":["G00000002"],"pending_ids":["C00000001"],"member_channels":["G00000002"],"next_marker":"next"}),None).unwrap();
    assert_eq!(page.failed_ids(), ["C00000001"]);
    assert_eq!(page.invalid_ids(), ["G00000002"]);
    assert_eq!(page.pending_ids(), ["C00000001"]);
    assert_eq!(page.membership_ids(), ["G00000002"]);
    let next = read.next_page(page.next_marker().unwrap()).unwrap();
    assert!(next.next_page("next").is_err());
    assert_eq!(next.body()["marker"], "next");
    assert!(page.rows().is_empty());
}
