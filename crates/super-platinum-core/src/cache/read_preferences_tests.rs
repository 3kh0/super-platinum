use super::*;
use crate::state::{ReadMode, ReadPreferenceError, ReadPreferences};
use serde_json::json;

fn session() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T_READ_CACHE".into(),
        enterprise_id: None,
        user_id: "U_READ_CACHE".into(),
        name: "Read preference fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    }
}

#[test]
fn all_three_modes_round_trip_full_and_startup_cache() {
    let cache = Cache::open(":memory:").unwrap();
    let session = session();
    let mut workspace = Workspace::from_session(&session);
    for mode in [
        ReadMode::LeftOffAndRead,
        ReadMode::NewestAndRead,
        ReadMode::NewestKeepUnseen,
    ] {
        workspace.read_preferences = ReadPreferences::from_prefs(&mode.patch());
        cache.save_workspace(&workspace).unwrap();
        for restored in [
            cache.load_workspace(&session).unwrap().unwrap(),
            cache.load_workspace_for_startup(&session).unwrap().unwrap(),
        ] {
            assert_eq!(restored.read_preferences, workspace.read_preferences);
            assert_eq!(restored.read_preferences.mode(), Ok(mode));
        }
    }
}

#[test]
fn metadata_snapshot_carries_updated_preferences_without_touching_read_cursor() {
    let cache = Cache::open(":memory:").unwrap();
    let session = session();
    let mut workspace = Workspace::from_session(&session);
    workspace.read_preferences = ReadPreferences::from_prefs(&ReadMode::LeftOffAndRead.patch());
    workspace.messages.insert(
        "C_READ".into(),
        ChannelMessages {
            last_read: Some("1800000000.000010".into()),
            unread_count: 20,
            ..Default::default()
        },
    );
    cache.save_workspace(&workspace).unwrap();
    workspace.read_preferences = ReadPreferences::from_prefs(&ReadMode::NewestKeepUnseen.patch());
    workspace.messages.get_mut("C_READ").unwrap().last_read = Some("1800000000.000999".into());
    WorkspaceSnapshot::metadata(&workspace)
        .save(&cache)
        .unwrap();
    let restored = cache.load_workspace(&session).unwrap().unwrap();
    assert_eq!(
        restored.read_preferences.mode(),
        Ok(ReadMode::NewestKeepUnseen)
    );
    assert_eq!(
        restored.messages["C_READ"].last_read.as_deref(),
        Some("1800000000.000010")
    );
    assert_eq!(restored.messages["C_READ"].unread_count, 20);
}

#[test]
fn old_sidebar_without_preferences_remains_unavailable_until_boot() {
    let cache = Cache::open(":memory:").unwrap();
    let session = session();
    let workspace = Workspace::from_session(&session);
    cache.save_workspace(&workspace).unwrap();
    cache
        .conn
        .execute(
            "update workspaces set sidebar = ?1 where team_id = ?2",
            params![r#"{"priority_section":true}"#, session.team_id],
        )
        .unwrap();
    let mut restored = cache.load_workspace_for_startup(&session).unwrap().unwrap();
    assert_eq!(
        restored.read_preferences.mode(),
        Err(ReadPreferenceError::Unavailable)
    );
    assert!(!restored.read_preferences.mark_on_open(true, false));
    assert!(restored.priority_sidebar_section);
    restored.apply_boot(
        serde_json::from_value(
            json!({"prefs":{"mark_msgs_read_immediately":true,"start_scroll_at_oldest":false}}),
        )
        .unwrap(),
    );
    assert_eq!(
        restored.read_preferences.mode(),
        Ok(ReadMode::NewestAndRead)
    );
}

#[test]
fn unsupported_raw_preference_survives_cache_without_restoring_old_read_permission() {
    let cache = Cache::open(":memory:").unwrap();
    let session = session();
    let mut workspace = Workspace::from_session(&session);
    workspace.read_preferences = ReadPreferences::from_prefs(&ReadMode::NewestAndRead.patch());
    workspace
        .read_preferences
        .apply_change("mark_msgs_read_immediately", json!("false"));
    cache.save_workspace(&workspace).unwrap();
    let restored = cache.load_workspace_for_startup(&session).unwrap().unwrap();
    assert_eq!(
        restored.read_preferences.raw()["mark_msgs_read_immediately"],
        "false"
    );
    assert_eq!(
        restored.read_preferences.mode(),
        Err(ReadPreferenceError::UnsupportedValue)
    );
    assert!(!restored.read_preferences.mark_on_open(true, false));
}
