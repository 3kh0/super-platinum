use super::*;
use serde_json::json;
fn row(id: &str, name: &str) -> Value {
    json!({"id":id,"name":name,"_real_name_normalized_lc":name,"_display_name_normalized_lc":name})
}
fn apply<'a>(
    q: &str,
    rows: &'a [Value],
    maybe_self: bool,
    remote: &BTreeSet<String>,
    pass: bool,
) -> Vec<local::Scored<'a>> {
    members(
        q,
        &local::PreparedQuery {
            normalized: q.into(),
            matcher_normalized: q.into(),
        },
        rows,
        remote,
        pass,
        maybe_self,
        |_| true,
    )
    .unwrap()
}
#[test]
fn empty_keeps_self_deleted_and_duplicates_without_activity_policy() {
    let mut a = row("U00000001", "ada");
    a["is_self"] = json!(true);
    let mut b = row("U00000002", "bob");
    b["deleted"] = json!(true);
    let rows = vec![a.clone(), b, a];
    let out = apply("", &rows, false, &BTreeSet::new(), false);
    assert_eq!(
        out.iter().map(|r| r.id).collect::<Vec<_>>(),
        ["U00000001", "U00000002", "U00000001"]
    );
    assert!(out.iter().all(|r| r.score == 0
        && !r.is_self
        && r.is_exact_match.is_none()
        && r.score_details.is_none()));
    assert!(std::ptr::eq(out[2].item, &rows[2]));
}
#[test]
fn exact_is_not_promoted_and_email_is_not_matched() {
    let mut r = row("U00000001", "ada");
    r["_email_normalized_lc"] = json!("uniqueaddress@company.test");
    let rows = vec![r];
    let out = apply("ada", &rows, false, &BTreeSet::new(), false);
    assert_eq!(out.len(), 1);
    assert!(out[0].is_exact_match.is_none());
    assert!(out[0].exact_match_details.is_none());
    assert!(apply("uniqueaddress", &rows, false, &BTreeSet::new(), false).is_empty());
}
#[test]
fn localized_self_and_remote_fuzzy_are_explicit_observations() {
    let mut r = row("U00000001", "ada");
    r["is_self"] = json!(true);
    let rows = vec![r];
    assert!(apply("myself", &rows, false, &BTreeSet::new(), false).is_empty());
    let out = apply("myself", &rows, true, &BTreeSet::new(), false);
    assert_eq!(out[0].score_details.as_ref().unwrap()["self_score"], 0);
    assert!(!out[0].is_self);
    let ids = BTreeSet::from(["U00000001".to_owned()]);
    assert!(apply("zzzzzz", &rows, false, &ids, false).is_empty());
    assert!(apply("zzzzzz", &rows, false, &BTreeSet::new(), true).is_empty());
    let out = apply("zzzzzz", &rows, false, &ids, true);
    assert_eq!(out[0].score, 2);
    assert!(out[0].is_flannel_fuzzy_passthrough);
}
#[test]
fn absent_observation_refuses_but_excluded_unknown_needs_no_name() {
    let q = local::PreparedQuery {
        normalized: String::new(),
        matcher_normalized: String::new(),
    };
    let unknown = vec![json!({"id":"U00000001","isUnknown":true})];
    assert!(
        members("", &q, &unknown, &BTreeSet::new(), false, false, |_| true)
            .unwrap()
            .is_empty()
    );
    let raw = vec![json!({"id":"U00000001"})];
    assert!(matches!(
        members("", &q, &raw, &BTreeSet::new(), false, false, |_| true),
        Err(Error::MissingObservation(_))
    ));
    assert!(
        members("", &q, &raw, &BTreeSet::new(), false, false, |_| false)
            .unwrap()
            .is_empty()
    );
}
