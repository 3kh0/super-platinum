use super::*;
use serde_json::json;
const TEAM: &str = "T12345678";
const CHANNEL: &str = "C12345678";
const TS: &str = "1700000000.123456";
fn history() -> Value {
    json!({"ok":true,"channel":CHANNEL,"has_more":true,"messages":[{"type":"message","ts":TS,"text":"original whole body"}]})
}
fn info() -> Value {
    json!({"ok":true,"channel":{"id":CHANNEL,"name":"project","is_channel":true,"is_member":true,"is_private":false,"context_team_id":TEAM,"is_org_shared":false}})
}
#[test]
fn thread_timestamp_fresh_root_and_channel_construction_cover_only_the_finite_internal_target() {
    let h = history();
    let i = info();
    let originals = (h.clone(), i.clone());
    let target = Target::from_fresh(TEAM, CHANNEL, TS, &h, &i).unwrap();
    assert_eq!(
        (target.team(), target.channel(), target.timestamp()),
        (TEAM, CHANNEL, TS)
    );
    for thread in [Value::Null, json!(""), json!(TS)] {
        for teams in [Value::Null, json!([]), json!([TEAM])] {
            let mut a = h.clone();
            a["messages"][0]["thread_ts"] = thread.clone();
            let mut b = i.clone();
            b["channel"]["internal_team_ids"] = teams;
            assert!(Target::from_fresh(TEAM, CHANNEL, TS, &a, &b).is_ok());
        }
    }
    // Source3YDY/FHLi/w4o6 use exact ts/thread_ts, not a root bag/count/sender.
    // This pure data result is never current canonical or action authority.
    let mut unrelated = h.clone();
    unrelated["messages"][0]["root"] = json!({"ts":"1600000000.000001"});
    unrelated["messages"][0]["reply_count"] = json!(999);
    unrelated["messages"][0]["source_team_id"] = json!("TOTHER123");
    assert!(Target::from_fresh(TEAM, CHANNEL, TS, &unrelated, &i).is_ok());
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
        "is_tombstoned",
    ] {
        let mut a = h.clone();
        a["messages"][0][key] = json!(false);
        assert!(Target::from_fresh(TEAM, CHANNEL, TS, &a, &i).is_ok());
        for bad in [json!(true), Value::Null, json!("false"), json!(0)] {
            let mut a = h.clone();
            a["messages"][0][key] = bad;
            assert!(
                Target::from_fresh(TEAM, CHANNEL, TS, &a, &i).is_err(),
                "message {key}"
            );
        }
    }
    for key in [
        "is_im",
        "is_group",
        "is_mpim",
        "is_file",
        "is_app",
        "is_app_home",
        "is_archived",
        "is_frozen",
        "isUnknown",
        "isNonExistent",
        "hidden",
        "is_hidden",
        "is_tombstone",
        "is_tombstoned",
        "deleted",
        "is_deleted",
    ] {
        let mut b = i.clone();
        b["channel"][key] = json!(false);
        assert!(Target::from_fresh(TEAM, CHANNEL, TS, &h, &b).is_ok());
        for bad in [json!(true), Value::Null, json!("false"), json!(0)] {
            let mut b = i.clone();
            b["channel"][key] = bad;
            assert!(
                Target::from_fresh(TEAM, CHANNEL, TS, &h, &b).is_err(),
                "channel {key}"
            );
        }
    }
    for key in [
        "id",
        "name",
        "is_channel",
        "is_member",
        "is_private",
        "is_org_shared",
        "context_team_id",
    ] {
        let mut b = i.clone();
        b["channel"].as_object_mut().unwrap().remove(key);
        assert!(
            Target::from_fresh(TEAM, CHANNEL, TS, &h, &b).is_err(),
            "missing {key}"
        );
    }
    for (key, value) in [
        ("id", json!("COTHER123")),
        ("name", json!("")),
        ("name", json!(" project")),
        ("name", json!("a\nb")),
        ("is_channel", json!(false)),
        ("is_channel", json!("true")),
        ("is_member", json!(false)),
        ("is_member", json!(1)),
        ("is_private", json!(true)),
        ("is_org_shared", json!(true)),
        ("context_team_id", json!("TOTHER123")),
        ("internal_team_ids", json!(["TOTHER123"])),
        ("internal_team_ids", json!([TEAM, TEAM])),
        ("internal_team_ids", json!(false)),
        ("internalTeamIds", json!([])),
        ("fromAnotherTeam", json!(false)),
        ("contextTeamId", json!(TEAM)),
        ("isOrgShared", json!(false)),
        ("team_id", json!("TOTHER123")),
    ] {
        let mut b = i.clone();
        b["channel"][key] = value;
        assert!(
            Target::from_fresh(TEAM, CHANNEL, TS, &h, &b).is_err(),
            "channel {key}"
        );
    }
    for (key, value) in [
        ("type", json!("message_changed")),
        ("type", Value::Null),
        ("ts", json!("1700000000.123457")),
        ("channel", json!("COTHER123")),
        ("thread_ts", json!("1600000000.000001")),
        ("thread_ts", json!(false)),
        ("subtype", json!("")),
        ("subtype", json!("thread_broadcast")),
        ("subtype", json!("file_comment")),
        ("subtype", json!("ekm_access_denied")),
        ("subtype", json!("dlp_tombstone")),
        ("subtype", json!("dsa_restricted")),
        ("subtype", json!("retention_threshold")),
    ] {
        let mut a = h.clone();
        a["messages"][0][key] = value;
        assert!(
            Target::from_fresh(TEAM, CHANNEL, TS, &a, &i).is_err(),
            "message {key}"
        );
    }
    for (key, value) in [
        ("messages", json!([])),
        (
            "messages",
            json!([h["messages"][0].clone(), h["messages"][0].clone()]),
        ),
        ("messages", json!([TS])),
        ("channel", json!("COTHER123")),
        ("has_more", Value::Null),
        ("deleted", json!([TS])),
        ("deleted", Value::Null),
        ("unchanged_messages", json!([])),
    ] {
        let mut a = h.clone();
        a[key] = value;
        assert!(
            Target::from_fresh(TEAM, CHANNEL, TS, &a, &i).is_err(),
            "history {key}"
        );
    }
    for (team, channel, ts) in [
        ("T1", CHANNEL, TS),
        ("E12345678", CHANNEL, TS),
        (TEAM, "G12345678", TS),
        (TEAM, "C_BAD", TS),
        (TEAM, CHANNEL, "1700000000123456"),
        (TEAM, CHANNEL, "1700000000.12345"),
        (TEAM, CHANNEL, "1700000000.1234567"),
    ] {
        assert!(Target::from_fresh(team, channel, ts, &h, &i).is_err());
    }
    for mut raw in [h.clone(), i.clone()] {
        raw["ok"] = json!(false);
        assert!(Target::from_fresh(TEAM, CHANNEL, TS, &raw, &i).is_err());
        assert!(Target::from_fresh(TEAM, CHANNEL, TS, &h, &raw).is_err());
    }
    for error in [json!("denied"), json!(false), json!({})] {
        let mut a = h.clone();
        a["error"] = error.clone();
        let mut b = i.clone();
        b["error"] = error;
        assert!(Target::from_fresh(TEAM, CHANNEL, TS, &a, &i).is_err());
        assert!(Target::from_fresh(TEAM, CHANNEL, TS, &h, &b).is_err());
    }
    assert_eq!(
        (h, i),
        originals,
        "data classification preserves original envelopes"
    );
}
