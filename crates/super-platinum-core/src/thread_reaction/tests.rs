use super::*;
use crate::composer_emoji::{catalog::Catalog, preferences::Preferences, standard::Version};
use serde_json::json;
use std::collections::BTreeMap;

const C: &str = "C1";
const TS: &str = "1710000001.000001";
const SELF: &str = "U1";
fn root() -> Value {
    json!({"type":"message","channel":C,"ts":TS,"thread_ts":TS,
        "user":"U2","text":"Original root","is_locked":false,"is_beyond_free_limit":false})
}
fn history(message: &Value) -> Value {
    json!({"ok":true,"messages":[message],"has_more":false})
}
fn info() -> Value {
    json!({"ok":true,"channel":{"id":C,"name":"release","is_channel":true,
        "is_member":true,"is_private":false,"is_frozen":false}})
}
fn full_prefs(csv: &str) -> Value {
    json!({"ok":true,"prefs":{"hidden_users":csv,"preferred_skin_tone":3}})
}
fn full(message: &Value, rows: Value) -> Value {
    let mut message = message.clone();
    message["reactions"] = rows;
    json!({"ok":true,"type":"message","channel":C,"message":message})
}
fn row(name: &str, user: &str) -> Value {
    json!({"name":name,"count":1,"users":[user]})
}

#[test]
fn original_root_fresh_policy_full_membership_and_one_wire_toggle_corpus() {
    let raw = root();
    let original = OriginalRoot::capture(C, TS, &raw).unwrap();
    assert_eq!(
        (original.channel(), original.timestamp(), original.user()),
        (C, TS, "U2")
    );
    assert_eq!(original.thread_ts(), Some(TS));
    let h = history(&raw);
    let i = info();
    let p = full_prefs("");
    let policy = RootPolicy::capture(&original, &h, &i, &p).unwrap();
    assert_eq!(policy.original().thread_ts(), Some(TS));
    assert!(policy.current(&original, &h, &i, &p));
    let catalog = Catalog::build_records(Version::V16, vec![]).unwrap();
    let mut prefs = Preferences::default();
    prefs.replace(&BTreeMap::from([("preferred_skin_tone".into(), json!(3))]));
    let membership =
        RootMembership::decode(&original, &full(&raw, json!([row("eyes", "U3")]))).unwrap();
    assert_eq!(membership.original().thread_ts(), Some(TS));
    let add = membership
        .selection(&policy, &catalog, &prefs, "eyes", SELF)
        .unwrap();
    assert!(add.adding());
    assert_eq!(add.name(), "eyes");
    assert!(add.current(&policy, &catalog, &prefs, &membership, SELF));
    // A later full membership changes the operation, never the earlier record.
    let after = RootMembership::decode(
        &original,
        &full(&raw, json!([{"name":"eyes","count":2,"users":["U3",SELF]}])),
    )
    .unwrap();
    let remove = after
        .selection(&policy, &catalog, &prefs, "eyes", SELF)
        .unwrap();
    assert!(!remove.adding());
    assert_eq!(remove.name(), "eyes");
    assert!(!add.current(&policy, &catalog, &prefs, &after, SELF));
    assert!(!remove.current(&policy, &catalog, &prefs, &after, "U3"));
    let another_catalog = Catalog::build_records(Version::V16, vec![]).unwrap();
    assert!(!remove.current(&policy, &another_catalog, &prefs, &after, SELF));
    let mut changed_prefs = prefs.clone();
    changed_prefs.apply_change("preferred_skin_tone", json!(4));
    assert!(!remove.current(&policy, &catalog, &changed_prefs, &after, SELF));
    assert!(
        membership
            .selection(&policy, &catalog, &Preferences::default(), "eyes", SELF)
            .is_err()
    );

    // Actual same-root truthiness is retained; Search remains independently
    // narrower. The no-thread record also proves a real data-profile overlap.
    assert!(crate::search_reaction::FreshPolicy::capture(C, TS, &h, &i).is_err());
    let mut no_thread = raw.clone();
    no_thread.as_object_mut().unwrap().remove("thread_ts");
    let no_thread_origin = OriginalRoot::capture(C, TS, &no_thread).unwrap();
    assert!(RootPolicy::capture(&no_thread_origin, &history(&no_thread), &i, &p).is_ok());
    assert!(crate::search_reaction::FreshPolicy::capture(C, TS, &history(&no_thread), &i).is_ok());
    assert!(RootPolicy::capture(&original, &history(&no_thread), &i, &p).is_err());
    assert!(RootMembership::decode(&original, &full(&no_thread, json!([]))).is_err());
    for thread in [
        Value::Null,
        json!(""),
        json!(false),
        json!("1710000000.000001"),
    ] {
        let mut later = raw.clone();
        later["thread_ts"] = thread;
        assert!(!original.matches(&later));
        assert!(RootPolicy::capture(&original, &history(&later), &i, &p).is_err());
        assert!(RootMembership::decode(&original, &full(&later, json!([]))).is_err());
    }
    for field in ["is_locked", "is_beyond_free_limit"] {
        for value in [json!(true), json!("false"), json!({}), json!([]), json!(0)] {
            let mut denied = raw.clone();
            denied[field] = value;
            assert!(OriginalRoot::capture(C, TS, &denied).is_err(), "{field}");
            assert!(RootPolicy::capture(&original, &history(&denied), &i, &p).is_err());
            assert!(RootMembership::decode(&original, &full(&denied, json!([]))).is_err());
        }
        let mut fresh_missing = raw.clone();
        fresh_missing.as_object_mut().unwrap().remove(field);
        assert!(OriginalRoot::capture(C, TS, &fresh_missing).is_ok());
        fresh_missing[field] = Value::Null;
        assert!(OriginalRoot::capture(C, TS, &fresh_missing).is_ok());
    }
    for field in [
        "is_ephemeral",
        "isEKMError",
        "isUnknown",
        "isNonExistent",
        "hidden",
        "is_hidden",
        "deleted",
        "is_deleted",
        "is_tombstone",
        "is_tombstoned",
        "no_display",
        "_hidden_stub",
    ] {
        let mut denied = raw.clone();
        denied[field] = json!(true);
        assert!(OriginalRoot::capture(C, TS, &denied).is_err(), "{field}");
        assert!(RootPolicy::capture(&original, &history(&denied), &i, &p).is_err());
    }
    for field in [
        "bot_id",
        "app_id",
        "bot_profile",
        "document_comment",
        "workflow",
        "workflow_id",
        "workflow_responses",
        "ai_context",
        "file",
        "file_id",
    ] {
        let mut denied = raw.clone();
        denied[field] = json!({});
        assert!(OriginalRoot::capture(C, TS, &denied).is_err(), "{field}");
    }
    for (field, value) in [
        ("type", json!("file")),
        ("user", json!("__proto__")),
        ("user", json!("U_BAD")),
        ("ts", json!("1.2")),
        ("channel", json!("C2")),
        ("subtype", json!("bot_message")),
        ("files", json!([{}])),
    ] {
        let mut denied = raw.clone();
        denied[field] = value;
        assert!(OriginalRoot::capture(C, TS, &denied).is_err());
    }
    for channel in ["C", "G1", "D1", "c1", "C_1"] {
        assert!(OriginalRoot::capture(channel, TS, &raw).is_err());
    }
    for field in ["is_private", "is_channel", "is_member", "is_frozen"] {
        for value in [Value::Null, json!("false"), json!(0)] {
            let mut bad = i.clone();
            bad["channel"][field] = value;
            assert!(
                RootPolicy::capture(&original, &h, &bad, &p).is_err(),
                "{field}"
            );
        }
        let mut missing = i.clone();
        missing["channel"].as_object_mut().unwrap().remove(field);
        assert!(RootPolicy::capture(&original, &h, &missing, &p).is_err());
    }
    let mut private = i.clone();
    private["channel"]["is_private"] = json!(true);
    assert!(RootPolicy::capture(&original, &h, &private, &p).is_err());
    for field in [
        "is_im",
        "is_group",
        "is_mpim",
        "is_file",
        "is_archived",
        "isUnknown",
        "isNonExistent",
    ] {
        let mut bad = i.clone();
        bad["channel"][field] = json!(true);
        assert!(RootPolicy::capture(&original, &h, &bad, &p).is_err());
    }
    for csv in ["U2", "U3,U2", ",U2,", "U2,U2"] {
        assert!(RootPolicy::capture(&original, &h, &i, &full_prefs(csv)).is_err());
    }
    for csv in ["", "U3", " U2", "U2 ", "u2", "constructor,__proto__"] {
        assert!(RootPolicy::capture(&original, &h, &i, &full_prefs(csv)).is_ok());
    }
    for value in [Value::Null, json!(false), json!(0), json!([]), json!({})] {
        let mut bad = p.clone();
        bad["prefs"]["hidden_users"] = value;
        assert!(RootPolicy::capture(&original, &h, &i, &bad).is_err());
    }
    let mut missing_csv = p.clone();
    missing_csv["prefs"]
        .as_object_mut()
        .unwrap()
        .remove("hidden_users");
    assert!(RootPolicy::capture(&original, &h, &i, &missing_csv).is_err());

    let explicit_empty = RootMembership::decode(&original, &full(&raw, json!([]))).unwrap();
    assert!(explicit_empty.reactions().is_empty());
    assert!(
        explicit_empty
            .selection(&policy, &catalog, &prefs, "eyes", SELF)
            .is_err(),
        "no first-Add"
    );
    let mut omitted = full(&raw, json!([]));
    omitted["message"]
        .as_object_mut()
        .unwrap()
        .remove("reactions");
    assert!(RootMembership::decode(&original, &omitted).is_err());
    for rows in [
        Value::Null,
        json!({}),
        json!([row("eyes", "U3"), row("eyes", "U4")]),
        json!([{"name":"eyes","count":2,"users":["U1","U1"]}]),
        json!([{"name":"eyes","count":1,"users":["U1","U2"]}]),
        json!([{"name":"eyes","count":0,"users":[]}]),
        json!([{"name":"eyes","count":1.5,"users":[]}]),
        json!([{"name":"eyes","count":1}]),
        json!([row("bad name", "U3")]),
        json!([row("eyes", "__proto__")]),
    ] {
        assert!(RootMembership::decode(&original, &full(&raw, rows)).is_err());
    }
    // Tone addition is the existing-pill defaultMulti=true branch, not a
    // picker selection. Remove always names the one actual owned wire row.
    let wave = RootMembership::decode(&original, &full(&raw, json!([row("wave", "U3")]))).unwrap();
    let add_wave = wave
        .selection(&policy, &catalog, &prefs, "wave", SELF)
        .unwrap();
    assert_eq!(add_wave.name(), "wave::skin-tone-3");
    let wave_after = RootMembership::decode(
        &original,
        &full(
            &raw,
            json!([row("wave", "U3"), row("wave::skin-tone-3", SELF)]),
        ),
    )
    .unwrap();
    let remove_wave = wave_after
        .selection(&policy, &catalog, &prefs, "wave", SELF)
        .unwrap();
    assert!(!remove_wave.adding());
    assert_eq!(remove_wave.name(), "wave::skin-tone-3");
    let untoned_own =
        RootMembership::decode(&original, &full(&raw, json!([row("wave", SELF)]))).unwrap();
    assert_eq!(
        untoned_own
            .selection(&policy, &catalog, &prefs, "wave", SELF)
            .unwrap()
            .name(),
        "wave"
    );
    let multi_own = RootMembership::decode(
        &original,
        &full(
            &raw,
            json!([row("wave", SELF), row("wave::skin-tone-3", SELF)]),
        ),
    )
    .unwrap();
    assert!(
        multi_own
            .selection(&policy, &catalog, &prefs, "wave", SELF)
            .is_err()
    );
    let no_raw_base = RootMembership::decode(
        &original,
        &full(&raw, json!([row("wave::skin-tone-3", SELF)])),
    )
    .unwrap();
    assert!(
        no_raw_base
            .selection(&policy, &catalog, &prefs, "wave", SELF)
            .is_err()
    );
    let malformed_own = RootMembership::decode(
        &original,
        &full(
            &raw,
            json!([row("eyes", "U3"), row("wave::skin-tone-9", SELF)]),
        ),
    )
    .unwrap();
    assert!(
        malformed_own
            .selection(&policy, &catalog, &prefs, "eyes", SELF)
            .is_err()
    );

    let quota = |total: usize, own: usize, selected: &str, selected_own: bool| {
        let mut rows = (0..total - 1)
            .map(|n| row(&format!("quota_{n}"), if n < own { SELF } else { "U3" }))
            .collect::<Vec<_>>();
        rows.push(row(selected, if selected_own { SELF } else { "U3" }));
        RootMembership::decode(&original, &full(&raw, json!(rows))).unwrap()
    };
    assert!(
        quota(23, 22, "eyes", false)
            .selection(&policy, &catalog, &prefs, "eyes", SELF)
            .is_ok()
    );
    assert!(
        quota(24, 23, "eyes", false)
            .selection(&policy, &catalog, &prefs, "eyes", SELF)
            .is_err()
    );
    assert!(
        quota(49, 0, "wave", false)
            .selection(&policy, &catalog, &prefs, "wave", SELF)
            .is_ok()
    );
    assert!(
        quota(50, 0, "wave", false)
            .selection(&policy, &catalog, &prefs, "wave", SELF)
            .is_err()
    );
    assert!(
        quota(50, 0, "eyes", false)
            .selection(&policy, &catalog, &prefs, "eyes", SELF)
            .is_ok()
    );
    let remove_over_quota = quota(51, 50, "eyes", true)
        .selection(&policy, &catalog, &prefs, "eyes", SELF)
        .unwrap();
    assert!(!remove_over_quota.adding());
    // Every caller input is still data; this module changes no original raw
    // payload, shared Message, Search classifier, account or network state.
    assert_eq!(raw, root());
    assert!(i["channel"].get("is_file").is_none());
}
