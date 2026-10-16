use super::*;
use serde_json::json;
fn channel() -> Value {
    json!({"id":"C00000001","is_channel":true,"is_member":true,"is_ext_shared":false,
        "is_read_only":false,"is_thread_only":false})
}
fn facts(raw: &Value) -> PostingFacts {
    let mut f = PostingFacts::new(raw["id"].as_str().unwrap().into());
    assert!(f.observe(raw, None));
    f
}
fn member() -> User {
    serde_json::from_value(json!({"id":"U00000001"})).unwrap()
}
#[test]
fn explicit_unrestricted_joined_public_and_private_paths_are_usable() {
    let raw = channel();
    assert_eq!(destination(&raw, &facts(&raw), None), Eligibility::Allowed);
    let mut private = raw.clone();
    private["is_channel"] = json!(false);
    private["is_private"] = json!(true);
    assert_eq!(
        destination(&private, &facts(&private), None),
        Eligibility::Allowed
    );
    for member in [Value::Null, json!(false)] {
        private["is_member"] = member;
        assert_eq!(
            destination(&private, &facts(&private), None),
            Eligibility::Unsupported(Unsupported::Membership)
        );
    }
}
#[test]
fn missing_effective_data_is_unknown_not_a_default_permission() {
    let mut raw = channel();
    raw.as_object_mut().unwrap().remove("is_read_only");
    assert_eq!(
        destination(&raw, &facts(&raw), None),
        Eligibility::Unsupported(Unsupported::PostingFacts)
    );
    raw.as_object_mut().unwrap().remove("is_thread_only");
    assert_eq!(facts(&raw).values(), (None, None));
    raw["properties"] = json!({"posting_restricted_to":null,"threads_restricted_to":null});
    assert_eq!(facts(&raw).values(), (None, None));
}
#[test]
fn independent_partial_booleans_precede_conflicting_fallback_and_omission_retains() {
    let mut f = facts(&channel());
    assert!(f.observe(&json!({"id":"C00000001","is_read_only":true}), None));
    assert!(f.observe(&json!({"id":"C00000001","name":"renamed"}), None));
    assert_eq!(f.values(), (Some(true), Some(false)));
    assert!(f.observe(
        &json!({"id":"C00000001","is_thread_only":true,
        "properties":{"posting_restricted_to":{"role":["all"]}}}),
        Some(&member())
    ));
    assert_eq!(f.values(), (Some(true), Some(true)));
    assert!(f.observe(&json!({"id":"C00000001","is_read_only":false}), None));
    assert_eq!(f.values(), (Some(false), Some(true)));
    assert!(!f.observe(&json!({"id":"C00000002","is_thread_only":false}), None));
    assert_eq!(f.values(), (Some(false), Some(true)));
}
#[test]
fn fallback_explicit_user_and_first_all_role_are_supported_without_admin_guess() {
    for restriction in [
        json!({"user":["U00000001"],"role":["admin"]}),
        json!({"role":["all","admin"]}),
        json!({"type":["ra"]}),
    ] {
        let raw = json!({"id":"C00000001","is_ext_shared":false,
            "properties":{"posting_restricted_to":restriction}});
        let mut f = PostingFacts::new("C00000001".into());
        f.observe(&raw, Some(&member()));
        assert_eq!(f.values(), (Some(false), Some(false)));
    }
    for role in ["regular", "admin", "org_admin", "unknown"] {
        let raw = json!({"id":"C00000001","is_ext_shared":false,
            "properties":{"posting_restricted_to":{"role":[role]}}});
        let mut f = facts(&channel());
        f.observe(&raw, Some(&member()));
        assert_eq!(f.values(), (None, None));
    }
}
#[test]
fn empty_restrictions_and_companion_omission_follow_sourced_formula() {
    let mut f = facts(&channel());
    f.observe(
        &json!({"id":"C00000001","is_ext_shared":false,
        "properties":{"posting_restricted_to":{}}}),
        None,
    );
    assert_eq!(f.values(), (Some(false), Some(true)));
    f.observe(
        &json!({"id":"C00000001","is_ext_shared":false,
        "properties":{"posting_restricted_to":{},"threads_restricted_to":{}}}),
        None,
    );
    assert_eq!(f.values(), (Some(true), Some(false)));
}
#[test]
fn malformed_fallback_or_effective_values_invalidate_old_grant() {
    for patch in [
        json!({"id":"C00000001","is_read_only":"false"}),
        json!({"id":"C00000001","is_ext_shared":false,"properties":{"posting_restricted_to":false}}),
        json!({"id":"C00000001","is_ext_shared":false,"properties":{"posting_restricted_to":{"role":"all"}}}),
        json!({"id":"C00000001","is_ext_shared":true,"properties":{"posting_restricted_to":{"role":["all"]}}}),
    ] {
        let mut f = facts(&channel());
        f.observe(&patch, Some(&member()));
        assert_eq!(f.values(), (None, None));
    }
}
#[test]
fn restrictions_deny_general_but_do_not_guess_unknown_plan_exception() {
    let mut raw = channel();
    raw["is_read_only"] = json!(true);
    assert_eq!(
        destination(&raw, &facts(&raw), None),
        Eligibility::Unsupported(Unsupported::RestrictionException)
    );
    raw["is_general"] = json!(true);
    assert_eq!(
        destination(&raw, &facts(&raw), None),
        Eligibility::Denied(Denial::ReadOnly)
    );
    raw["is_read_only"] = json!(false);
    raw["is_thread_only"] = json!(true);
    assert_eq!(
        destination(&raw, &facts(&raw), None),
        Eligibility::Denied(Denial::ThreadOnly)
    );
}
#[test]
fn direct_negative_checks_and_malformed_flags_are_distinct() {
    for (key, reason) in [
        ("is_archived", Denial::Archived),
        ("is_frozen", Denial::Frozen),
        ("isUnknown", Denial::UnavailableConversation),
        ("isNonExistent", Denial::UnavailableConversation),
        ("is_file", Denial::FileConversation),
    ] {
        let mut raw = channel();
        raw[key] = json!(true);
        assert_eq!(
            destination(&raw, &facts(&raw), None),
            Eligibility::Denied(reason)
        );
    }
    let mut raw = channel();
    raw["properties"] = json!({"huddles":{"is_huddle_channel":true}});
    assert_eq!(
        destination(&raw, &facts(&raw), None),
        Eligibility::Denied(Denial::HuddleConversation)
    );
    raw["properties"] = json!({});
    raw["is_frozen"] = json!("false");
    assert_eq!(
        destination(&raw, &facts(&raw), None),
        Eligibility::Unsupported(Unsupported::ConversationShape)
    );
}
#[test]
fn human_im_and_existing_mpim_need_known_internal_open_conversation() {
    let mut raw = channel();
    raw["id"] = json!("D00000001");
    raw["is_channel"] = json!(false);
    raw["is_im"] = json!(true);
    raw["is_open"] = json!(true);
    raw["user"] = json!("U00000001");
    let mut peer = json!({"id":"U00000001","is_bot":false,"deleted":false});
    assert_eq!(
        destination(&raw, &facts(&raw), Some(&peer)),
        Eligibility::Allowed
    );
    peer["is_bot"] = json!(true);
    assert_eq!(
        destination(&raw, &facts(&raw), Some(&peer)),
        Eligibility::Unsupported(Unsupported::AppDm)
    );
    peer["deleted"] = json!(true);
    assert_eq!(
        destination(&raw, &facts(&raw), Some(&peer)),
        Eligibility::Denied(Denial::DeletedPeer)
    );
    raw["user"] = json!("USLACK");
    assert_eq!(
        destination(&raw, &facts(&raw), None),
        Eligibility::Denied(Denial::SystemIm)
    );
    raw["is_pending_ext_shared"] = json!(true);
    assert_eq!(
        destination(&raw, &facts(&raw), None),
        Eligibility::Denied(Denial::PendingExternalIm)
    );
    raw["is_pending_ext_shared"] = json!(false);
    raw["is_im"] = json!(false);
    raw["is_mpim"] = json!(true);
    assert_eq!(destination(&raw, &facts(&raw), None), Eligibility::Allowed);
}
#[test]
fn connect_missing_membership_and_foreign_fact_identity_never_grant() {
    let mut raw = channel();
    raw.as_object_mut().unwrap().remove("is_ext_shared");
    assert_eq!(
        destination(&raw, &facts(&raw), None),
        Eligibility::Unsupported(Unsupported::SlackConnect)
    );
    raw["is_ext_shared"] = json!(true);
    assert_eq!(
        destination(&raw, &facts(&raw), None),
        Eligibility::Unsupported(Unsupported::SlackConnect)
    );
    raw["is_ext_shared"] = json!(false);
    assert_eq!(
        destination(&raw, &PostingFacts::new("C00000002".into()), None),
        Eligibility::Unsupported(Unsupported::FactsIdentity)
    );
}
#[test]
fn hosted_file_route_requires_explicit_kind_not_uploader_ownership() {
    let mut raw = json!({"id":"F00000001","mode":"hosted","filetype":"png","is_external":false,"user":"U_OTHER"});
    let check = |v: Value| source(&serde_json::from_value::<File>(v).unwrap());
    assert_eq!(check(raw.clone()), Eligibility::Allowed);
    for kind in ["quip", "canvas", "list", "lists"] {
        raw["filetype"] = json!(kind);
        assert_eq!(
            check(raw.clone()),
            Eligibility::Unsupported(Unsupported::FileKind)
        );
    }
    raw["filetype"] = json!("png");
    raw.as_object_mut().unwrap().remove("is_external");
    assert_eq!(
        check(raw.clone()),
        Eligibility::Unsupported(Unsupported::FileKind)
    );
    raw["mode"] = json!("tombstone");
    assert_eq!(check(raw), Eligibility::Denied(Denial::DeletedFile));
}

#[test]
fn missing_human_classification_and_member_unknown_never_grant() {
    let mut raw = channel();
    raw["id"] = json!("D00000001");
    raw["is_im"] = json!(true);
    raw["is_open"] = json!(true);
    raw["user"] = json!("U00000001");
    assert_eq!(
        destination(&raw, &facts(&raw), Some(&json!({"id":"U00000001"}))),
        Eligibility::Unsupported(Unsupported::Peer)
    );
    let mut user = member();
    user.extra.insert("isUnknown".into(), json!(true));
    let mut f = facts(&channel());
    f.observe(
        &json!({"id":"C00000001","is_ext_shared":false,
        "properties":{"posting_restricted_to":{"user":["U00000001"]}}}),
        Some(&user),
    );
    assert_eq!(f.values(), (None, None));
}
#[test]
fn eligibility_is_read_only_and_malformed_file_lifecycle_is_unsupported() {
    let raw = channel();
    let original = raw.clone();
    let f = facts(&raw);
    let previous = f.clone();
    assert_eq!(destination(&raw, &f, None), Eligibility::Allowed);
    assert_eq!(raw, original);
    assert_eq!(f, previous);
    let file: File = serde_json::from_value(json!({"id":"F00000001","mode":"hosted",
        "filetype":"png","is_external":false,"is_deleted":"false"}))
    .unwrap();
    assert_eq!(
        source(&file),
        Eligibility::Unsupported(Unsupported::FileShape)
    );
}

#[test]
fn valid_read_only_with_malformed_thread_companion_retires_only_companion() {
    for malformed in [Value::Null, json!("false"), json!(0), json!({}), json!([])] {
        let mut f = facts(&channel());
        let mut raw = channel();
        raw["is_thread_only"] = malformed;
        assert!(f.observe(&raw, None));
        assert_eq!(f.values(), (Some(false), None));
        assert_eq!(
            destination(&raw, &f, None),
            Eligibility::Unsupported(Unsupported::PostingFacts)
        );
        assert!(f.observe(&json!({"id":"C00000001","name":"partial"}), None));
        assert_eq!(f.values(), (Some(false), None));
        assert!(f.observe(&json!({"id":"C00000001","is_thread_only":false}), None));
        assert_eq!(f.values(), (Some(false), Some(false)));
        assert_eq!(destination(&channel(), &f, None), Eligibility::Allowed);
    }
}
#[test]
fn valid_thread_only_with_malformed_read_companion_retires_only_companion() {
    for malformed in [Value::Null, json!("false"), json!(0), json!({}), json!([])] {
        let mut f = facts(&channel());
        let mut raw = channel();
        raw["is_read_only"] = malformed;
        assert!(f.observe(&raw, None));
        assert_eq!(f.values(), (None, Some(false)));
        assert_eq!(
            destination(&raw, &f, None),
            Eligibility::Unsupported(Unsupported::PostingFacts)
        );
        assert!(f.observe(&json!({"id":"C00000001","is_thread_only":false}), None));
        assert_eq!(f.values(), (None, Some(false)));
        assert!(f.observe(&json!({"id":"C00000001","is_read_only":false}), None));
        assert_eq!(f.values(), (Some(false), Some(false)));
        assert_eq!(destination(&channel(), &f, None), Eligibility::Allowed);
    }
}
