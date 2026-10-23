use super::*;
use super_platinum_core::{
    block_actions::home::workflow::directory_observations as observed, config::WorkspaceSession,
};

#[test]
fn navigation_storage_namespace_is_a_distinct_uuid_for_each_fixture_instance() {
    let first = Data::new();
    let second = Data::new();
    assert!(uuid::Uuid::parse_str(&first.navigation_owner).is_ok());
    assert!(uuid::Uuid::parse_str(&second.navigation_owner).is_ok());
    assert_ne!(first.navigation_owner, second.navigation_owner);
}

#[test]
fn local_and_original_remote_rows_are_distinct_and_overflow_is_not_ranked_here() {
    let local = rows::local_people();
    assert_eq!(local.len(), 42);
    let mut count = local.len();
    for kind in [wire::Kind::Members, wire::Kind::Channels] {
        let (a, ka) = rows::search_reply(kind, "review", None).unwrap();
        let (b, kb) = rows::search_reply(kind, "review", Some("ranking-page-2")).unwrap();
        assert_eq!(ka.as_ref().unwrap()[0], "z99");
        assert_eq!(kb.as_ref().unwrap()[0], "z86");
        let mut pages = wire::Pages::default();
        pages.accept(a, ka.as_deref()).unwrap();
        pages.accept(b, kb.as_deref()).unwrap();
        let remote = pages.rows().unwrap();
        assert_eq!(remote.len(), 24);
        assert!(
            remote
                .iter()
                .all(|r| !local.iter().any(|v| v["id"] == r["id"]))
        );
        if kind == wire::Kind::Channels {
            assert!(remote.iter().all(|r| r["is_member"] == false));
            assert_eq!(pages.outcomes().unwrap().membership_ids.len(), 25);
        }
        count += remote.len();
    }
    assert_eq!(count, 90);
}

#[test]
fn complete_acquired_preferences_and_grid_are_decodable() {
    use super_platinum_core::block_actions::{name_policy, shortcuts::grid};
    let me = json!({"id":"UWORKFLOW","team_id":"TWORKFLOW","profile":{"real_name":"Original"}});
    let session = WorkspaceSession {
        team_id: "TWORKFLOW".into(),
        user_id: "UWORKFLOW".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    };
    let raw = reads::reply("client.gantryBoot", &me, "cold-mixed").unwrap();
    assert_eq!(raw["self"], me);
    grid::Membership::decode(raw, &session).unwrap();
    name_policy::UserPrefs::decode(&reads::reply("users.prefs.get", &me, "cold-mixed").unwrap())
        .unwrap();
}

#[test]
fn actual_model_reply_distinguishes_write_from_read_and_observed_absence() {
    let absent = reads::reply("search.autocomplete.model", &Value::Null, "cold-mixed").unwrap();
    assert!(observed::decode_model(&absent).unwrap().is_none());
    let raw = reads::reply("search.autocomplete.model", &Value::Null, "cached-write").unwrap();
    let model = observed::decode_model(&raw).unwrap().unwrap();
    assert_eq!(
        model.score(&[("CHANNEL_IS_MEMBER", 1.)], "share_dialog_channel_picker"),
        8.
    );
    assert_eq!(model.score(&[("CHANNEL_IS_MEMBER", 1.)], "settings"), -8.);
    let offline = reads::reply(
        "search.autocomplete.offlineFeatures",
        &Value::Null,
        "cold-mixed",
    )
    .unwrap();
    let keys = reads::offline_keys(&offline).unwrap();
    assert!(keys.is_none());
    observed::Offline::decode_ordered(&offline, keys.as_deref()).unwrap();
    // The old fixture metadata poisoned the shared offline cache after the
    // first cold snapshot, so the next query could not capture a warm snapshot.
    assert!(observed::Offline::decode_ordered(&offline, Some(&[])).is_err());
    assert!(reads::reply("chat.postMessage", &Value::Null, "cold-mixed").is_err());
}

#[test]
fn ordered_priority_evidence_distinguishes_absence_empty_and_unobserved_order() {
    let mut raw = reads::reply(
        "search.autocomplete.offlineFeatures",
        &Value::Null,
        "cached-write",
    )
    .unwrap();
    for value in [Value::Null, json!({})] {
        raw["channel_priority_90d"] = value.clone();
        let keys = reads::offline_keys(&raw).unwrap();
        assert_eq!(keys.is_some(), value.is_object());
        observed::Offline::decode_ordered(&raw, keys.as_deref()).unwrap();
    }
    raw["channel_priority_90d"] = json!({"CRANKR000":1});
    assert!(reads::offline_keys(&raw).is_err());
    raw["channel_priority_90d"] = json!([]);
    assert!(reads::offline_keys(&raw).is_err());
}
