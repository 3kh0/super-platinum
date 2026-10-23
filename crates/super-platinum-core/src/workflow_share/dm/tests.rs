use super::*;
use serde_json::json;

fn channel() -> Value {
    json!({"id":"DDMPEER01","is_im":true,"user":"UHUMAN001","fromAnotherTeam":false})
}
fn peer() -> Value {
    json!({"id":"UHUMAN001","team_id":"TWORKFLOW","profile":{"display_name":"Human"}})
}
fn flags() -> Flags {
    Flags {
        read_only: false,
        thread_only: false,
    }
}
fn allowed(channel: &Value, peer: &Value) -> bool {
    existing_dm_eligible(channel, peer, flags(), "TWORKFLOW")
}

#[test]
fn existing_dm_needs_no_member_or_open_flag_and_does_not_consume_raw() {
    let mut c = channel();
    let mut p = peer();
    assert!(allowed(&c, &p));
    c["is_member"] = json!(false);
    c["is_open"] = json!(false);
    p["is_restricted"] = json!(true);
    let before = (c.clone(), p.clone());
    assert!(allowed(&c, &p));
    assert_eq!((c, p), before);
}

#[test]
fn local_scope_requires_actual_peer_association() {
    let mut p = peer();
    p.as_object_mut().unwrap().remove("team_id");
    assert!(!allowed(&channel(), &p));
    p["enterprise_user"] = json!({"teams":["TOTHER","TWORKFLOW"]});
    assert!(allowed(&channel(), &p));
    p["enterprise_user"]["teams"] = json!(["TOTHER"]);
    assert!(!allowed(&channel(), &p));
    assert!(!existing_dm_eligible(&channel(), &peer(), flags(), ""));
}

#[test]
fn original_dm_and_peer_identity_must_correlate() {
    for id in ["CDMPEER01", "GDMPEER01", "DDM", "Ddmpeer01", ""] {
        let mut c = channel();
        c["id"] = json!(id);
        assert!(!allowed(&c, &peer()), "{id}");
    }
    let mut c = channel();
    c["user"] = json!("UOTHER001");
    assert!(!allowed(&c, &peer()));
    c = channel();
    c.as_object_mut().unwrap().remove("fromAnotherTeam");
    assert!(!allowed(&c, &peer()));
    c["fromAnotherTeam"] = json!(true);
    assert!(!allowed(&c, &peer()));
    for id in [
        "USLACK",
        "USLACKBOT",
        "USLACKSECURITY",
        "USEARCH",
        "UCHANNELEXPERT",
        "Uhuman001",
        "@UHUMAN001",
    ] {
        let mut p = peer();
        p["id"] = json!(id);
        c = channel();
        c["user"] = json!(id);
        assert!(!allowed(&c, &p), "{id}");
    }
}

#[test]
fn source_conversation_denials_and_unsupported_types_refuse() {
    for key in [
        "is_channel",
        "is_private",
        "is_group",
        "is_mpim",
        "is_archived",
        "is_frozen",
        "isUnknown",
        "isNonExistent",
        "is_pending_ext_shared",
        "is_ext_shared",
        "is_file",
    ] {
        let mut c = channel();
        c[key] = json!(true);
        assert!(!allowed(&c, &peer()), "{key}");
        c[key] = json!("false");
        assert!(!allowed(&c, &peer()), "malformed {key}");
    }
    let mut c = channel();
    c["is_im"] = json!(false);
    assert!(!allowed(&c, &peer()));
}

#[test]
fn human_scope_does_not_grant_bot_app_or_external_branches() {
    for key in [
        "deleted",
        "isUnknown",
        "isNonExistent",
        "is_bot",
        "is_app_user",
        "is_slackbot",
        "is_slacksecuritybot",
        "is_workflow_bot",
        "is_profile_only_user",
        "is_agentforce_bot",
        "isExternal",
        "is_stranger",
    ] {
        let mut p = peer();
        p[key] = json!(true);
        assert!(!allowed(&channel(), &p), "{key}");
        p[key] = json!(0);
        assert!(!allowed(&channel(), &p), "malformed {key}");
    }
    let mut p = peer();
    p["bot_id"] = json!("BKNOWN01");
    assert!(!allowed(&channel(), &p));
    p = peer();
    p["profile"]["api_app_id"] = json!("AKNOWN01");
    assert!(!allowed(&channel(), &p));
}

#[test]
fn effective_read_or_thread_only_policy_is_not_overridden_by_dm_type() {
    for (read_only, thread_only) in [(true, false), (false, true), (true, true)] {
        assert!(!existing_dm_eligible(
            &channel(),
            &peer(),
            Flags {
                read_only,
                thread_only
            },
            "TWORKFLOW"
        ));
    }
}

#[test]
fn huddle_is_independent_of_effective_policy() {
    let mut c = channel();
    c["properties"] = json!({"huddles":{"is_huddle_channel":true}});
    assert!(!allowed(&c, &peer()));
    c["properties"]["huddles"]["is_huddle_channel"] = json!(false);
    assert!(allowed(&c, &peer()));
    for value in [
        json!([]),
        json!({"huddles":true}),
        json!({"huddles":{"is_huddle_channel":"false"}}),
    ] {
        c["properties"] = value;
        assert!(!allowed(&c, &peer()));
    }
}

#[test]
fn malformed_peer_containers_do_not_fall_through_to_locality() {
    for (key, value) in [
        ("profile", json!([])),
        ("team_id", json!(false)),
        ("enterprise_user", json!([])),
        ("enterprise_user", json!({"teams":[false]})),
    ] {
        let mut p = peer();
        p[key] = value;
        assert!(!allowed(&channel(), &p), "{key}");
    }
    assert!(!allowed(&Value::Null, &peer()));
    assert!(!allowed(&channel(), &Value::Null));
}
