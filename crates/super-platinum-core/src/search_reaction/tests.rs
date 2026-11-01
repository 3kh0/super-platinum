use super::*;
use serde_json::json;

const CHANNEL: &str = "C1";
const TS: &str = "1710000001.000001";
fn history() -> Value {
    json!({"ok":true,"messages":[{"type":"message","ts":TS,"text":"Original ordinary root"}]})
}
fn info() -> Value {
    json!({"ok":true,"channel":{"id":CHANNEL,"name":"release","is_channel":true,"is_member":true,"is_frozen":false}})
}
fn membership() -> Value {
    json!({"ok":true,"type":"message","channel":CHANNEL,"message":{"type":"message","ts":TS,"reactions":[
        {"name":"eyes","count":4,"users":["U1","U2"]},
        {"name":"thumbsup::skin-tone-3","count":1,"users":["U2"]}
    ]}})
}

#[test]
fn search_reaction_fresh_policy_and_explicit_complete_membership_contract_corpus() {
    let policy = FreshPolicy::capture(CHANNEL, TS, &history(), &info()).unwrap();
    assert_eq!(policy.channel(), CHANNEL);
    assert_eq!(policy.timestamp(), TS);
    let mut explicit_history = history();
    let mut explicit_info = info();
    for key in ["is_ephemeral", "isEKMError", "isUnknown", "isNonExistent"] {
        explicit_history["messages"][0][key] = json!(false);
    }
    for key in [
        "is_file",
        "is_im",
        "is_group",
        "is_mpim",
        "is_archived",
        "isUnknown",
        "isNonExistent",
    ] {
        explicit_info["channel"][key] = json!(false);
    }
    FreshPolicy::capture(CHANNEL, TS, &explicit_history, &explicit_info).unwrap();
    // Fresh absence is the constructor-bound data profile, never an old model
    // merge or fabricated explicit false. The input envelopes remain untouched.
    assert!(history()["messages"][0].get("is_ephemeral").is_none());
    assert!(info()["channel"].get("is_file").is_none());
    for (channel, ts) in [
        ("D1", TS),
        ("C", TS),
        (CHANNEL, "1.2"),
        (CHANNEL, "1710000001.x00001"),
    ] {
        assert!(FreshPolicy::capture(channel, ts, &history(), &info()).is_err());
    }
    for case in [
        "no_ok",
        "error",
        "missing",
        "duplicate",
        "wrong_ts",
        "wrong_channel",
        "no_type",
        "thread",
        "bad_thread",
        "subtype",
    ] {
        let mut h = history();
        match case {
            "no_ok" => {
                h.as_object_mut().unwrap().remove("ok");
            }
            "error" => h["error"] = json!("not_allowed"),
            "missing" => h["messages"] = json!([]),
            "duplicate" => {
                h["messages"] = json!([h["messages"][0].clone(), h["messages"][0].clone()])
            }
            "wrong_ts" => h["messages"][0]["ts"] = json!("1710000001.000002"),
            "wrong_channel" => h["messages"][0]["channel"] = json!("C2"),
            "no_type" => {
                h["messages"][0].as_object_mut().unwrap().remove("type");
            }
            "thread" => h["messages"][0]["thread_ts"] = json!(TS),
            "bad_thread" => h["messages"][0]["thread_ts"] = json!(false),
            _ => h["messages"][0]["subtype"] = json!("bot_message"),
        }
        assert!(
            FreshPolicy::capture(CHANNEL, TS, &h, &info()).is_err(),
            "{case}"
        );
    }
    for key in [
        "is_ephemeral",
        "isEKMError",
        "isUnknown",
        "isNonExistent",
        "hidden",
        "is_hidden",
        "deleted",
        "is_deleted",
        "is_tombstone",
    ] {
        for value in [json!(true), json!(null), json!(0), json!("false")] {
            let mut h = history();
            h["messages"][0][key] = value;
            assert!(
                FreshPolicy::capture(CHANNEL, TS, &h, &info()).is_err(),
                "{key}"
            );
        }
    }
    for case in [
        "error",
        "wrong_id",
        "empty_name",
        "missing_channel",
        "not_member",
        "frozen",
        "missing_frozen",
        "malformed_channel",
    ] {
        let mut i = info();
        match case {
            "error" => i["ok"] = json!(false),
            "wrong_id" => i["channel"]["id"] = json!("C2"),
            "empty_name" => i["channel"]["name"] = json!(""),
            "missing_channel" => {
                i["channel"].as_object_mut().unwrap().remove("is_channel");
            }
            "not_member" => i["channel"]["is_member"] = json!(false),
            "frozen" => i["channel"]["is_frozen"] = json!(true),
            "missing_frozen" => {
                i["channel"].as_object_mut().unwrap().remove("is_frozen");
            }
            _ => i["channel"]["is_channel"] = json!("true"),
        }
        assert!(
            FreshPolicy::capture(CHANNEL, TS, &history(), &i).is_err(),
            "{case}"
        );
    }
    for key in [
        "is_file",
        "is_im",
        "is_group",
        "is_mpim",
        "is_archived",
        "isUnknown",
        "isNonExistent",
    ] {
        for value in [json!(true), json!(null), json!("false")] {
            let mut i = info();
            i["channel"][key] = value;
            assert!(
                FreshPolicy::capture(CHANNEL, TS, &history(), &i).is_err(),
                "{key}"
            );
        }
    }
    let data = Membership::decode(CHANNEL, TS, &membership()).unwrap();
    assert_eq!(data.reactions().len(), 2);
    assert_eq!(data.reactions()[0].count, 4);
    assert_eq!(data.reactions()[0].users, ["U1", "U2"]);
    assert!(!data.adding("eyes", "U1"));
    assert!(data.adding("thumbsup::skin-tone-3", "U1"));
    assert!(data.adding("new_name", "U1"));
    assert!(!data.adding("", "U1") && !data.adding("eyes", ""));
    let mut empty = membership();
    empty["message"]["reactions"] = json!([]);
    let empty = Membership::decode(CHANNEL, TS, &empty).unwrap();
    assert!(empty.reactions().is_empty() && empty.adding("eyes", "U1"));
    for case in [
        "no_array",
        "null_array",
        "wrong_type",
        "wrong_channel",
        "wrong_ts",
        "no_message_type",
        "reply",
        "duplicate_name",
        "duplicate_user",
        "missing_users",
        "bad_user",
        "bad_name",
        "missing_count",
        "zero_count",
        "negative_count",
        "fraction_count",
        "overflow_count",
        "too_few_count",
    ] {
        let mut v = membership();
        match case {
            "no_array" => {
                v["message"].as_object_mut().unwrap().remove("reactions");
            }
            "null_array" => v["message"]["reactions"] = Value::Null,
            "wrong_type" => v["type"] = json!("file"),
            "wrong_channel" => v["channel"] = json!("C2"),
            "wrong_ts" => v["message"]["ts"] = json!("1710000001.000002"),
            "no_message_type" => {
                v["message"].as_object_mut().unwrap().remove("type");
            }
            "reply" => v["message"]["thread_ts"] = json!(TS),
            "duplicate_name" => v["message"]["reactions"][1]["name"] = json!("eyes"),
            "duplicate_user" => v["message"]["reactions"][0]["users"] = json!(["U1", "U1"]),
            "missing_users" => {
                v["message"]["reactions"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("users");
            }
            "bad_user" => v["message"]["reactions"][0]["users"] = json!(["U1", "invalid"]),
            "bad_name" => v["message"]["reactions"][0]["name"] = json!("bad name"),
            "missing_count" => {
                v["message"]["reactions"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("count");
            }
            "zero_count" => v["message"]["reactions"][0]["count"] = json!(0),
            "negative_count" => v["message"]["reactions"][0]["count"] = json!(-1),
            "fraction_count" => v["message"]["reactions"][0]["count"] = json!(1.5),
            "overflow_count" => {
                v["message"]["reactions"][0]["count"] = json!(u64::from(u32::MAX) + 1)
            }
            _ => v["message"]["reactions"][0]["count"] = json!(1),
        }
        assert!(Membership::decode(CHANNEL, TS, &v).is_err(), "{case}");
    }
    use crate::composer_emoji::{
        catalog::{Catalog, CustomRecord},
        preferences::Preferences,
        standard::Version,
    };
    let catalog = Catalog::build_records(
        Version::V17,
        vec![CustomRecord {
            name: "retired_emoji".into(),
            value: "https://fixture.test/retired.png".into(),
            is_nonexistent: true,
        }],
    )
    .unwrap();
    let mut prefs = Preferences::default();
    assert!(empty.selection(&catalog, &prefs, "wave", "U1").is_err());
    prefs.replace(&BTreeMap::from([("preferred_skin_tone".into(), json!(3))]));
    let selected = empty.selection(&catalog, &prefs, "wave", "U1").unwrap();
    assert_eq!(selected.name(), "wave::skin-tone-3");
    assert!(selected.adding() && selected.current(&catalog, &prefs, &empty, "U1"));
    assert!(!selected.current(&catalog, &prefs, &empty, "U2"));
    assert!(empty.selection(&catalog, &prefs, "bad name", "U1").is_err());
    assert!(empty.selection(&catalog, &prefs, "wave", "").is_err());
    assert!(
        empty
            .selection(&catalog, &prefs, "retired_emoji", "U1")
            .is_err()
    );
    let mut owned = membership();
    owned["message"]["reactions"] = json!([
        {"name":"wave::skin-tone-3","count":1,"users":["U1"]},
        {"name":"retired_emoji","count":1,"users":["U1"]}
    ]);
    let owned = Membership::decode(CHANNEL, TS, &owned).unwrap();
    assert!(!selected.current(&catalog, &prefs, &owned, "U1"));
    let remove = owned.selection(&catalog, &prefs, "wave", "U1").unwrap();
    assert_eq!(remove.name(), "wave::skin-tone-3");
    assert!(!remove.adding() && remove.current(&catalog, &prefs, &owned, "U1"));
    assert!(
        !owned
            .selection(&catalog, &prefs, "retired_emoji", "U1")
            .unwrap()
            .adding()
    );
    let mut changed_prefs = prefs.clone();
    changed_prefs.apply_change("preferred_skin_tone", json!(2));
    assert!(!selected.current(&catalog, &changed_prefs, &empty, "U1"));
    let another_catalog = Catalog::build_records(Version::V17, vec![]).unwrap();
    assert!(!selected.current(&another_catalog, &prefs, &empty, "U1"));
    // Complete raw reaction rows, not grouped/normalized families. These
    // synthetic names exercise only the original selector's finite quotas.
    let quota = |total: usize, own: usize, existing: Option<&str>| {
        let mut raw = membership();
        let mut rows = (0..total)
            .map(|i| {
                json!({"name":format!("quota_{i}"),"count":1,
                "users":[if i<own{"U1"}else{"U2"}]})
            })
            .collect::<Vec<_>>();
        if let Some(name) = existing {
            rows.last_mut().unwrap()["name"] = json!(name);
        }
        raw["message"]["reactions"] = json!(rows);
        Membership::decode(CHANNEL, TS, &raw).unwrap()
    };
    let own22 = quota(22, 22, None);
    let add22 = own22.selection(&catalog, &prefs, "eyes", "U1").unwrap();
    assert!(add22.adding() && add22.current(&catalog, &prefs, &own22, "U1"));
    let own23 = quota(23, 23, None);
    assert!(own23.selection(&catalog, &prefs, "eyes", "U1").is_err());
    assert!(!add22.current(&catalog, &prefs, &own23, "U1"));
    let own23_existing = quota(24, 23, Some("eyes"));
    assert!(
        own23_existing
            .selection(&catalog, &prefs, "eyes", "U1")
            .is_err()
    );
    let rows49 = quota(49, 0, None);
    let add49 = rows49.selection(&catalog, &prefs, "eyes", "U1").unwrap();
    assert!(add49.adding());
    let rows50 = quota(50, 0, None);
    assert!(rows50.selection(&catalog, &prefs, "eyes", "U1").is_err());
    assert!(!add49.current(&catalog, &prefs, &rows50, "U1"));
    let existing50 = quota(50, 0, Some("eyes"));
    let add_existing = existing50
        .selection(&catalog, &prefs, "eyes", "U1")
        .unwrap();
    assert!(add_existing.adding() && add_existing.current(&catalog, &prefs, &existing50, "U1"));
    let remove50 = quota(50, 50, Some("eyes"));
    let remove = remove50.selection(&catalog, &prefs, "eyes", "U1").unwrap();
    assert!(!remove.adding() && remove.current(&catalog, &prefs, &remove50, "U1"));
    let alias49 = quota(49, 0, Some("+1::skin-tone-3"));
    assert!(
        alias49
            .selection(&catalog, &prefs, "thumbsup", "U1")
            .unwrap()
            .adding()
    );
    let alias50 = quota(50, 0, Some("+1::skin-tone-3"));
    assert!(
        alias50
            .selection(&catalog, &prefs, "thumbsup", "U1")
            .is_err()
    );
}
