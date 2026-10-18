use super::*;

const A: &str = "U00000001";
const B: &str = "U00000002";
const C: &str = "U00000003";
fn row(id: &str, version: Value, interaction: Option<bool>) -> Current {
    Current {
        native: json!({"id":id,"updated":9999,"isExternal":true,"deleted":true}),
        source: json!({"id":id,"updated":version,"profile":{"display_name":"  Cafe\u{301}  "}}),
        interaction,
    }
}
fn authenticated(read: &Read) -> Value {
    let mut body = read.body();
    body["token"] = json!("synthetic");
    body
}

#[test]
fn conditional_uses_raw_versions_and_observed_interactions_in_request_order() {
    let a = row(A, json!(101), Some(false));
    let mut b = row(B, json!(102), None);
    b.source["isExternal"] = json!(true);
    b.source["deleted"] = json!(true);
    let mut c = row(C, json!(0), Some(false));
    c.source["isExternal"] = json!(true);
    c.source["deleted"] = json!(true);
    let read = Read::conditional(vec![B.into(), A.into(), C.into()], &[a, b, c]).unwrap();
    assert_eq!(read.ids(), [B, A, C]);
    assert_eq!(
        read.body(),
        json!({
            "updated_ids":{(A):101,(B):102,(C):0},
            "check_interaction":true,"include_profile_only_users":true,
            "cannot_interact":[A],"disconnected":[B]
        })
    );
    assert!(valid_wire_body(&authenticated(&read)));
    let next = read.next_page("next-page").unwrap();
    let mut expected = read.body();
    expected["marker"] = json!("next-page");
    assert_eq!(next.body(), expected);
    assert!(valid_wire_body(&authenticated(&next)));
}

#[test]
fn version_zero_omits_optional_arrays_and_malformed_truthy_versions_refuse() {
    for value in [Value::Null, json!(false), json!(""), json!(0), json!(-0.0)] {
        let read = Read::conditional(vec![A.into()], &[row(A, value, Some(false))]).unwrap();
        assert_eq!(read.body()["updated_ids"][A], 0);
        assert!(read.body().get("cannot_interact").is_none());
        assert!(read.body().get("disconnected").is_none());
    }
    let mut missing = row(A, json!(1), None);
    missing.source.as_object_mut().unwrap().remove("updated");
    assert_eq!(
        Read::conditional(vec![A.into()], &[missing])
            .unwrap()
            .body()["updated_ids"][A],
        0
    );
    for value in [
        json!(true),
        json!("101"),
        json!(-1),
        json!(1.5),
        json!(9007199254740992_u64),
        json!([]),
        json!({}),
    ] {
        assert!(Read::conditional(vec![A.into()], &[row(A, value, None)]).is_err());
    }
    assert_eq!(
        Read::conditional(vec![A.into()], &[row(A, json!(101.0), None)])
            .unwrap()
            .body()["updated_ids"][A],
        101
    );
}

#[test]
fn conditional_requires_each_requested_source_once_and_never_uses_native_facts() {
    let a = row(A, json!(7), None);
    assert!(Read::conditional(vec![A.into(), B.into()], &[a.clone()]).is_err());
    assert!(Read::conditional(vec![A.into()], &[a.clone(), a.clone()]).is_err());
    assert!(Read::conditional(vec![A.into(), A.into()], &[a.clone()]).is_err());
    let read = Read::conditional(vec![A.into()], &[row(B, json!(55), Some(false)), a]).unwrap();
    assert_eq!(read.body()["updated_ids"], json!({(A):7}));
    assert!(read.body().get("cannot_interact").is_none());
    assert!(read.body().get("disconnected").is_none());
}

#[test]
fn raw_external_deleted_truthiness_is_distinct_from_disconnection_ledger() {
    for (external, deleted, expected) in [
        (json!(true), json!(true), true),
        (json!("yes"), json!([]), true),
        (json!(false), json!(true), false),
        (json!(true), json!(0), false),
        (Value::Null, json!(true), false),
    ] {
        let mut a = row(A, json!(8), Some(true));
        a.source["isExternal"] = external;
        a.source["deleted"] = deleted;
        a.source["disconnected"] = json!(!expected);
        let read = Read::conditional(vec![A.into()], &[a]).unwrap();
        assert_eq!(read.body().get("disconnected").is_some(), expected);
    }
}

#[test]
fn transport_union_preserves_forced_absent_and_refuses_forged_conditionals() {
    assert!(valid_wire_body(&authenticated(
        &Read::capture(vec![A.into()]).unwrap()
    )));
    assert!(valid_wire_body(&authenticated(
        &Read::absent(vec![A.into()]).unwrap()
    )));
    let read = Read::conditional(
        vec![A.into(), B.into()],
        &[row(A, json!(8), Some(false)), row(B, json!(0), None)],
    )
    .unwrap();
    let good = authenticated(&read);
    for (key, value) in [
        ("ids", json!([A])),
        ("subscribe", json!(true)),
        ("cannot_interact", json!([])),
        ("cannot_interact", json!([A, A])),
        ("cannot_interact", json!([C])),
        ("cannot_interact", json!([B])),
        ("disconnected", json!([B])),
        ("disconnected", json!(true)),
        ("updated_ids", json!({(A):"8"})),
        ("updated_ids", json!({(A):-1})),
    ] {
        let mut bad = good.clone();
        bad[key] = value;
        assert!(!valid_wire_body(&bad), "admitted forged field {key}");
    }
    let mut forced = authenticated(&Read::capture(vec![A.into()]).unwrap());
    forced["cannot_interact"] = json!([A]);
    assert!(!valid_wire_body(&forced));
}

#[test]
fn conditional_page_preserves_order_partial_outcomes_and_original_request() {
    let read = Read::conditional(
        vec![A.into(), B.into()],
        &[row(A, json!(8), None), row(B, json!(9), Some(false))],
    )
    .unwrap();
    let raw = json!({"ok":true,"results":{"last":{"id":B,"updated":10,"opaque":{"keep":1}},"first":{"id":A,"updated":8}},"pending_ids":[B],"can_interact":{(A):false},"disconnected_ids":[B],"next_marker":"next"});
    assert!(Page::decode(&read, raw.clone(), None).is_err());
    let page = Page::decode(
        &read,
        raw.clone(),
        Some(vec!["last".into(), "first".into()]),
    )
    .unwrap();
    assert_eq!(page.raw(), &raw);
    assert_eq!(
        page.rows()
            .iter()
            .map(|r| r["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [B, A]
    );
    assert_eq!(page.pending_ids(), [B]);
    assert_eq!(page.disconnected_ids(), [B]);
    assert_eq!(page.request().body(), read.body());
}
