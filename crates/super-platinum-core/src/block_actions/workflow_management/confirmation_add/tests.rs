use super::*;
fn channel() -> Value {
    json!({"id":"CADDDEST01","is_channel":true,"is_member":true,"fromAnotherTeam":false})
}
fn flags() -> Flags {
    Flags {
        read_only: false,
        thread_only: false,
    }
}
fn shortcut() -> Shortcut {
    Shortcut::capture(&json!({"id":"TR_REVIEW","type":"shortcut","name":"  Title &amp;  ","shortcut_url":"https://slack.com/shortcuts/ORIGINAL","opaque":{"keep":true}})).unwrap()
}
fn plan() -> Add {
    Add::new(
        &shortcut(),
        &channel(),
        flags(),
        &Bookmarks::decode("CADDDEST01", json!({"ok":true,"bookmarks":[],"opaque":8})).unwrap(),
        "TWORKFLOW",
        "TWORKFLOW",
    )
    .unwrap()
}
fn ack() -> Value {
    json!({"ok":true,"bookmark":{"id":"BkFIRST01","channel_id":"CADDDEST01","shortcut_id":"TR_REVIEW","type":"shortcut","title":"  Title &amp;  ","emoji":"","image_url":null,"opaque":{"keep":[1,true]}},"opaque_receipt":[2]})
}
#[test]
fn original_wire_nullish_title_and_optional_shortcut_link_are_not_link_editor_rules() {
    for raw in [
        json!({"id":"TR1"}),
        json!({"id":"TR1","type":"event"}),
        json!({"id":"TR1","type":null}),
    ] {
        assert!(Shortcut::capture(&raw).is_err());
    }
    let p = plan();
    assert_eq!(p.plan().method(), "bookmarks.add");
    assert_eq!(p.plan().reason(), "channel-tab-add-bookmark");
    assert_eq!(
        p.plan().arguments(),
        &json!({"channel":"CADDDEST01","title":"  Title &amp;  ","type":"shortcut","shortcut_id":"TR_REVIEW","link":"https://slack.com/shortcuts/ORIGINAL"})
    );
    for raw in [
        json!({"id":"TR1","type":"shortcut"}),
        json!({"id":"TR1","type":"shortcut","name":null}),
    ] {
        assert_eq!(Shortcut::capture(&raw).unwrap().title(), "TR1");
    }
    assert_eq!(
        Shortcut::capture(&json!({"id":"TR1","type":"shortcut","name":""}))
            .unwrap()
            .title(),
        ""
    );
    assert!(Shortcut::capture(&json!({"id":"TR1","type":"shortcut","name":false})).is_err());
    let s = Shortcut::capture(&json!({"id":"TR1","type":"shortcut"})).unwrap();
    let p = Add::new(
        &s,
        &channel(),
        flags(),
        &Bookmarks::decode("CADDDEST01", json!({"ok":true,"bookmarks":[]})).unwrap(),
        "T1",
        "T1",
    )
    .unwrap();
    assert!(p.plan().arguments().get("link").is_none());
}
#[test]
fn list_observation_and_duplicate_are_exact_channel_scoped() {
    for raw in [
        json!({"ok":true}),
        json!({"ok":true,"bookmarks":null}),
        json!({"ok":true,"bookmarks":[],"has_more":true}),
        json!({"ok":true,"bookmarks":[],"response_metadata":{"next_cursor":"more"}}),
        json!({"ok":true,"bookmarks":[{"channel_id":"OTHER"}]}),
    ] {
        assert!(Bookmarks::decode("CADDDEST01", raw).is_err());
    }
    let raw = json!({"ok":true,"bookmarks":[{"id":"BkOLD","channel_id":"CADDDEST01","shortcut_id":"TR_REVIEW","opaque":[3]}],"opaque":[4]});
    let b = Bookmarks::decode("CADDDEST01", raw.clone()).unwrap();
    assert_eq!(b.raw(), &raw);
    assert!(b.contains_shortcut("TR_REVIEW"));
    assert!(Add::new(&shortcut(), &channel(), flags(), &b, "T1", "T1").is_err());
    let other = Bookmarks::decode("COTHER001", json!({"ok":true,"bookmarks":[]})).unwrap();
    assert!(Add::new(&shortcut(), &channel(), flags(), &other, "T1", "T1").is_err());
}
#[test]
fn eligibility_requires_acquired_canonical_foreign_fact_and_effective_posting() {
    let good = channel();
    assert!(channel_eligible(&good, flags(), "T1", "E1"));
    for (key, value) in [
        ("is_member", json!(false)),
        ("is_archived", json!(true)),
        ("is_im", json!(true)),
        ("is_mpim", json!(true)),
        ("fromAnotherTeam", json!(true)),
        ("fromAnotherTeam", Value::Null),
        ("is_member", json!("true")),
        ("isUnknown", json!(true)),
    ] {
        let mut c = good.clone();
        c[key] = value;
        assert!(!channel_eligible(&c, flags(), "T1", "E1"), "{key}");
    }
    for f in [
        Flags {
            read_only: true,
            thread_only: false,
        },
        Flags {
            read_only: false,
            thread_only: true,
        },
    ] {
        assert!(!channel_eligible(&good, f, "T1", "E1"));
    }
    let mut shared = good;
    shared["is_ext_shared"] = json!(true);
    shared["conversation_host_id"] = json!("TOTHER");
    assert!(channel_eligible(&shared, flags(), "T1", "E1"));
}
#[test]
fn original_ack_raw_and_normalized_bookmark_are_distinct_from_navigation() {
    let raw = ack();
    let a = plan().acknowledge(raw.clone()).unwrap();
    assert_eq!(a.raw(), &raw);
    assert_eq!(a.raw_bookmark()["opaque"], json!({"keep":[1,true]}));
    assert!(a.bookmark().get("opaque").is_none());
    assert!(a.bookmark().get("emoji").is_none());
    assert_eq!(a.bookmark()["iconUrl"], Value::Null);
    assert_eq!(a.bookmark()["shortcutId"], "TR_REVIEW");
    assert!(a.navigable());
    assert_eq!(a.typed_bookmark().shortcut_id.as_deref(), Some("TR_REVIEW"));
    assert!(!a.typed_bookmark().editable());
    for value in ["Bk", "Bk_1", "Bk-1", "other"] {
        let mut r = raw.clone();
        r["bookmark"]["id"] = json!(value);
        assert!(!plan().acknowledge(r).unwrap().navigable());
    }
    for (key, value) in [
        ("channel_id", json!("OTHER")),
        ("shortcut_id", json!("OTHER")),
        ("type", json!("link")),
        ("id", Value::Null),
    ] {
        let mut r = raw.clone();
        r["bookmark"][key] = value;
        assert!(plan().acknowledge(r).is_err());
    }
    for r in [
        json!({"ok":false,"error":"denied"}),
        json!({"ok":true,"bookmark":false}),
        json!({"ok":true,"error":"contradictory","bookmark":raw["bookmark"]}),
    ] {
        assert!(plan().acknowledge(r).is_err());
    }
}
#[test]
fn typed_shortcut_identity_survives_legacy_decode_and_changes_content() {
    let old: crate::bookmarks::Bookmark =
        serde_json::from_value(json!({"id":"B1","type":"link"})).unwrap();
    assert_eq!(old.shortcut_id, None);
    assert!(old.editable());
    let mut changed = old.clone();
    changed.shortcut_id = Some("TR1".into());
    assert!(!old.same_content(&changed));
    let normalized: crate::bookmarks::Bookmark =
        serde_json::from_value(json!({"id":"B1","shortcutId":"TR1"})).unwrap();
    assert_eq!(normalized.shortcut_id, Some("TR1".into()));
}
#[test]
fn initial_candidate_preserves_retained_object_predicate_not_guessed_string_ids() {
    assert_eq!(
        initial_candidate(
            &json!({}),
            &json!({"steps":[{"inputs":{"b":{"value":"CFIRST001"},"a":{"value":"CSECOND01"}}}]})
        ),
        None
    );
    let wf = json!({"steps":[{"inputs":{"target":{"value":"CADDDEST01"}}}]});
    assert_eq!(
        initial_candidate(&json!({"channel_ids":["COTHER001"]}), &wf),
        Some(json!("CADDDEST01"))
    );
    assert_eq!(
        initial_candidate(
            &json!({"channel_ids":[{"id":"COTHER001","is_channel":true}]}),
            &wf
        ),
        Some(json!({"id":"COTHER001","is_channel":true}))
    );
    assert_eq!(
        initial_candidate(
            &json!({}),
            &json!({"steps":[{"inputs":{"target":{"value":"not-channel"}}}]})
        ),
        None
    );
}
