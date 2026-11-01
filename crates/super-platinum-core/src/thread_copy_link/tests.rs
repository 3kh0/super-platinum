use super::*;
use serde_json::json;

const TEAM: &str = "T01234567";
const CHANNEL: &str = "C01234567";
const TS: &str = "1700000001.000001";

fn inputs() -> (Value, Value, Value) {
    (
        json!({"ok":true,"messages":[{"type":"message","ts":TS,"text":"original body"}]}),
        json!({"ok":true,"channel":{"id":CHANNEL,"name":"ordinary","is_channel":true,
            "is_member":true,"is_private":false,"is_org_shared":false,"context_team_id":TEAM}}),
        json!({"id":TEAM,"url":"https://fixture.slack.com/"}),
    )
}

#[test]
fn thread_copy_fresh_ordinary_root_requires_original_coordinate_policy_and_literal_team_url() {
    let (history, info, team) = inputs();
    let capture =
        |h: &Value, i: &Value, t: &Value| FreshRootLink::capture(TEAM, CHANNEL, TS, h, i, t);
    let link = capture(&history, &info, &team).unwrap();
    assert_eq!(
        (link.team(), link.channel(), link.timestamp()),
        (TEAM, CHANNEL, TS)
    );
    assert_eq!(
        link.url(),
        "https://fixture.slack.com/archives/C01234567/p1700000001000001"
    );
    for thread in [Value::Null, json!(""), json!(TS)] {
        let mut h = history.clone();
        h["messages"][0]["thread_ts"] = thread;
        assert!(capture(&h, &info, &team).is_ok());
    }
    for ids in [Value::Null, json!([]), json!([TEAM])] {
        let mut i = info.clone();
        i["channel"]["internal_team_ids"] = ids;
        assert!(capture(&history, &i, &team).is_ok());
    }
    for (t, c, ts) in [
        ("TTEST", CHANNEL, TS),
        ("T0123456a", CHANNEL, TS),
        (TEAM, "G01234567", TS),
        (TEAM, "C0/123", TS),
        (TEAM, CHANNEL, "1700000001.1"),
        (TEAM, CHANNEL, "1700000001.000001x"),
    ] {
        assert!(FreshRootLink::capture(t, c, ts, &history, &info, &team).is_err());
    }
    for (key, value) in [
        ("type", json!("file")),
        ("ts", json!("1700000000.000001")),
        ("channel", json!("CFOREIGN")),
        ("subtype", json!("bot_message")),
        ("subtype", json!("")),
        ("thread_ts", json!("1700000000.000001")),
        ("thread_ts", json!(false)),
    ] {
        let mut h = history.clone();
        h["messages"][0][key] = value;
        assert!(capture(&h, &info, &team).is_err(), "message {key}");
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
        for value in [json!(true), Value::Null, json!(0)] {
            let mut h = history.clone();
            h["messages"][0][key] = value;
            assert!(capture(&h, &info, &team).is_err(), "message {key}");
        }
        let mut h = history.clone();
        h["messages"][0][key] = json!(false);
        assert!(capture(&h, &info, &team).is_ok());
    }
    for h in [
        json!({"ok":true,"messages":[]}),
        json!({"ok":true,"messages":[history["messages"][0],history["messages"][0]]}),
        json!({"ok":false,"messages":history["messages"]}),
        json!({"ok":true,"error":"denied","messages":history["messages"]}),
        json!({"ok":true,"unchanged_messages":[],"messages":history["messages"]}),
        json!({"ok":true,"deleted":[TS],"messages":history["messages"]}),
        json!({"ok":true,"deleted":null,"messages":history["messages"]}),
    ] {
        assert!(capture(&h, &info, &team).is_err());
    }
    for key in [
        "is_channel",
        "is_member",
        "is_private",
        "is_org_shared",
        "context_team_id",
        "id",
        "name",
    ] {
        let mut i = info.clone();
        i["channel"].as_object_mut().unwrap().remove(key);
        assert!(capture(&history, &i, &team).is_err(), "missing {key}");
    }
    for (key, value) in [
        ("is_channel", json!(false)),
        ("is_member", json!(false)),
        ("is_private", json!(true)),
        ("is_org_shared", json!(true)),
        ("context_team_id", json!("T76543210")),
        ("name", json!("")),
        ("name", json!(" invalid")),
        ("team_id", json!("T76543210")),
        ("internal_team_ids", json!("")),
        ("internal_team_ids", json!(["T76543210"])),
        ("internal_team_ids", json!([TEAM, TEAM])),
        ("internal_team_ids", json!([null])),
        ("internalTeamIds", json!([])),
        ("fromAnotherTeam", json!(false)),
    ] {
        let mut i = info.clone();
        i["channel"][key] = value;
        assert!(capture(&history, &i, &team).is_err(), "channel {key}");
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
        "is_tombstone",
        "deleted",
        "is_deleted",
    ] {
        for value in [json!(true), Value::Null, json!("false")] {
            let mut i = info.clone();
            i["channel"][key] = value;
            assert!(capture(&history, &i, &team).is_err(), "channel {key}");
        }
    }
    for url in ["https://fixture.slack.com", "https://a-b2.slack.com/"] {
        let mut t = team.clone();
        t["url"] = json!(url);
        assert!(capture(&history, &info, &t).is_ok());
    }
    for url in [
        "https://fixture.enterprise.slack.com",
        "https://fixture.enterprise.slack.com/",
        "https://a-b2.enterprise.slack.com/",
    ] {
        let mut t = team.clone();
        t["url"] = json!(url);
        let link = capture(&history, &info, &t).unwrap();
        assert_eq!(
            link.url(),
            format!(
                "{}/archives/{CHANNEL}/p1700000001000001",
                url.strip_suffix('/').unwrap_or(url)
            )
        );
    }
    for url in [
        "",
        "http://fixture.slack.com",
        "HTTPS://fixture.slack.com",
        "https://Fixture.slack.com",
        "https://fixture.slack.com:443",
        "https://u@fixture.slack.com",
        "https://fixture.slack.com/x",
        "https://fixture.slack.com//",
        "https://fixture.slack.com?x=1",
        "https://fixture.slack.com/#x",
        "https://fixture.slack.com.evil",
        "https://app.slack.com",
        "https://my.slack.com",
        "https://a.b.slack.com",
        "https://-a.slack.com",
        "https://a-.slack.com",
        "https://app.enterprise.slack.com",
        "https://my.enterprise.slack.com",
        "https://.enterprise.slack.com",
        "https://a.b.enterprise.slack.com",
        "https://-a.enterprise.slack.com",
        "https://a-.enterprise.slack.com",
        "https://Fixture.enterprise.slack.com",
        "https://%66ixture.enterprise.slack.com",
        "https://u@fixture.enterprise.slack.com",
        "https://fixture.enterprise.slack.com:443",
        "https://fixture.enterprise.slack.com/x",
        "https://fixture.enterprise.slack.com//",
        "https://fixture.enterprise.slack.com?x=1",
        "https://fixture.enterprise.slack.com/#x",
        "https://fixture.enterprise.slack.com.evil",
        "https://é.slack.com",
        "https://fixture.slack.com\n",
    ] {
        let mut t = team.clone();
        t["url"] = json!(url);
        assert!(capture(&history, &info, &t).is_err(), "url {url:?}");
    }
    for t in [
        json!({"id":"T76543210","url":"https://fixture.slack.com"}),
        json!({"id":TEAM,"domain":"fixture"}),
        json!({"id":TEAM,"url":null}),
    ] {
        assert!(capture(&history, &info, &t).is_err());
    }
    assert_eq!(history, inputs().0);
    assert_eq!(info, inputs().1);
    assert_eq!(team, inputs().2);
}
