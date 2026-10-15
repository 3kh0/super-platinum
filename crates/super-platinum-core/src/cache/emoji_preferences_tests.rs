use super::*;
use serde_json::json;

#[test]
fn ordered_frecency_wire_and_presence_survive_full_startup_and_metadata_cache() {
    let session = WorkspaceSession {
        team_id: "T_EMOJI".into(),
        user_id: "U".into(),
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
        enterprise_id: None,
    };
    let cache = Cache::open(":memory:").unwrap();
    let mut workspace = Workspace::from_session(&session);
    let wire = r#"{"z":{"id":"Ez","count":1,"visits":[1]},"a":{"id":"Ea","count":1,"visits":[1]}}"#;
    workspace
        .emoji_preferences
        .replace(&std::collections::BTreeMap::from([
            ("frecency".into(), json!(wire)),
            ("preferred_skin_tone".into(), json!(4)),
        ]));
    cache.save_workspace(&workspace).unwrap();
    for restored in [
        cache.load_workspace(&session).unwrap().unwrap(),
        cache.load_workspace_for_startup(&session).unwrap().unwrap(),
    ] {
        assert_eq!(restored.emoji_preferences, workspace.emoji_preferences);
        assert_eq!(
            restored
                .emoji_preferences
                .frecency()
                .and_then(serde_json::Value::as_str),
            Some(wire)
        );
    }
    workspace
        .emoji_preferences
        .apply_change("emoji_mode", json!("as_text"));
    WorkspaceSnapshot::metadata(&workspace)
        .save(&cache)
        .unwrap();
    let restored = cache.load_workspace_for_startup(&session).unwrap().unwrap();
    assert_eq!(restored.emoji_preferences, workspace.emoji_preferences);
    cache
        .conn
        .execute(
            "update workspaces set sidebar = ?1 where team_id = ?2",
            params![r#"{"priority_section":true}"#, session.team_id],
        )
        .unwrap();
    let mut restored = cache.load_workspace_for_startup(&session).unwrap().unwrap();
    assert!(!restored.emoji_preferences.loaded());
    restored.apply_boot(serde_json::from_value(json!({"prefs":{}})).unwrap());
    assert!(restored.emoji_preferences.loaded());
    assert!(restored.emoji_preferences.frecency().is_none());
}
