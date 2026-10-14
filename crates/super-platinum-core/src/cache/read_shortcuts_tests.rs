use super::*;
use crate::state::{CONFIRM_CLEAR_UNREADS, ClearUnreadsPolicyError, ClearUnreadsPreference};
use serde_json::{Value, json};

fn session() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T_SHORTCUT_CACHE".into(),
        enterprise_id: None,
        user_id: "U_SHORTCUT_CACHE".into(),
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    }
}

fn pref(value: Value) -> ClearUnreadsPreference {
    ClearUnreadsPreference::from_prefs(&serde_json::from_value(value).unwrap())
}

#[test]
fn confirmation_authority_roundtrips_full_and_startup_including_malformed_values() {
    let cache = Cache::open(":memory:").unwrap();
    let session = session();
    let mut workspace = Workspace::from_session(&session);
    for preference in [
        ClearUnreadsPreference::default(),
        pref(json!({})),
        pref(json!({CONFIRM_CLEAR_UNREADS:true})),
        pref(json!({CONFIRM_CLEAR_UNREADS:false})),
        pref(json!({CONFIRM_CLEAR_UNREADS:null})),
        pref(json!({CONFIRM_CLEAR_UNREADS:"false"})),
    ] {
        workspace.clear_unreads_preference = preference;
        cache.save_workspace(&workspace).unwrap();
        for restored in [
            cache.load_workspace(&session).unwrap().unwrap(),
            cache.load_workspace_for_startup(&session).unwrap().unwrap(),
        ] {
            assert_eq!(
                restored.clear_unreads_preference,
                workspace.clear_unreads_preference
            );
            assert_eq!(
                restored.clear_unreads_preference.requires_confirmation(),
                workspace.clear_unreads_preference.requires_confirmation()
            );
        }
    }
}

#[test]
fn metadata_snapshot_carries_partial_pref_without_changing_channel_cursor() {
    let cache = Cache::open(":memory:").unwrap();
    let session = session();
    let mut workspace = Workspace::from_session(&session);
    workspace.clear_unreads_preference = pref(json!({CONFIRM_CLEAR_UNREADS:true}));
    workspace.messages.insert(
        "C_FIXTURE".into(),
        ChannelMessages {
            last_read: Some("10.000001".into()),
            unread_count: 7,
            ..Default::default()
        },
    );
    cache.save_workspace(&workspace).unwrap();
    workspace
        .clear_unreads_preference
        .apply_change(CONFIRM_CLEAR_UNREADS, json!(false));
    workspace.messages.get_mut("C_FIXTURE").unwrap().last_read = Some("20.000001".into());
    WorkspaceSnapshot::metadata(&workspace)
        .save(&cache)
        .unwrap();
    let restored = cache.load_workspace(&session).unwrap().unwrap();
    assert_eq!(
        restored.clear_unreads_preference.requires_confirmation(),
        Ok(false)
    );
    assert_eq!(
        restored.messages["C_FIXTURE"].last_read.as_deref(),
        Some("10.000001")
    );
    assert_eq!(restored.messages["C_FIXTURE"].unread_count, 7);
}

#[test]
fn old_sidebar_missing_confirmation_authority_stays_unknown_until_complete_boot() {
    let cache = Cache::open(":memory:").unwrap();
    let session = session();
    cache
        .save_workspace(&Workspace::from_session(&session))
        .unwrap();
    cache
        .conn
        .execute(
            "update workspaces set sidebar = ?1 where team_id = ?2",
            params![r#"{"priority_section":true}"#, session.team_id],
        )
        .unwrap();
    let mut restored = cache.load_workspace_for_startup(&session).unwrap().unwrap();
    assert_eq!(
        restored.clear_unreads_preference.requires_confirmation(),
        Err(ClearUnreadsPolicyError::Unavailable)
    );
    restored
        .clear_unreads_preference
        .apply_change(CONFIRM_CLEAR_UNREADS, json!(false));
    assert_eq!(
        restored.clear_unreads_preference.requires_confirmation(),
        Err(ClearUnreadsPolicyError::Unavailable)
    );
    restored.apply_boot(serde_json::from_value(json!({"prefs":{}})).unwrap());
    assert_eq!(
        restored.clear_unreads_preference.requires_confirmation(),
        Ok(false)
    );
}
