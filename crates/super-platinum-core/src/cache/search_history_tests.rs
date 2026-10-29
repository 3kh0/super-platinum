use super::*;
use crate::search::{SearchKind, SearchSort};
use serde_json::json;

fn session() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T_HISTORY".into(),
        user_id: "U_HISTORY".into(),
        name: "History fixture".into(),
        url: "https://example.slack.com".into(),
        token: String::new(),
        enterprise_id: None,
    }
}

#[test]
fn recent_searches_survive_full_startup_and_metadata_snapshots_without_cross_user_restore() {
    let session = session();
    let cache = Cache::open(":memory:").unwrap();
    let mut workspace = Workspace::from_session(&session);
    workspace.priority_sidebar_section = true;
    workspace.recent_searches.record(
        "release on:2026-10-01",
        SearchKind::Files,
        SearchSort::Oldest,
    );
    cache.save_workspace(&workspace).unwrap();
    for restored in [
        cache.load_workspace(&session).unwrap().unwrap(),
        cache.load_workspace_for_startup(&session).unwrap().unwrap(),
    ] {
        assert_eq!(restored.recent_searches, workspace.recent_searches);
        assert!(restored.priority_sidebar_section);
    }
    workspace.recent_searches.record(
        "\"planning notes\" from:me",
        SearchKind::Messages,
        SearchSort::Relevant,
    );
    WorkspaceSnapshot::metadata(&workspace)
        .save(&cache)
        .unwrap();
    assert_eq!(
        cache
            .load_workspace_for_startup(&session)
            .unwrap()
            .unwrap()
            .recent_searches,
        workspace.recent_searches
    );
    let mut other = session.clone();
    other.user_id = "U_OTHER".into();
    assert!(
        cache
            .load_workspace(&other)
            .unwrap()
            .unwrap()
            .recent_searches
            .entries()
            .is_empty()
    );
    cache
        .conn
        .execute("update workspaces set self_user_id = ''", [])
        .unwrap();
    assert!(
        cache
            .load_workspace(&session)
            .unwrap()
            .unwrap()
            .recent_searches
            .entries()
            .is_empty()
    );
}

#[test]
fn missing_malformed_or_future_history_does_not_discard_other_cached_preferences() {
    let session = session();
    let cache = Cache::open(":memory:").unwrap();
    cache
        .save_workspace(&Workspace::from_session(&session))
        .unwrap();
    for history in [
        None,
        Some(json!(false)),
        Some(json!({"unexpected":[]})),
        Some(json!([
            {"query":"future","kind":"channels","sort":"newest"},
            {"query":"known","kind":"files","sort":"oldest"},
            {"query":"future sort","kind":"messages","sort":"future"}
        ])),
    ] {
        let mut sidebar = json!({"priority_section":true,"starred_order":["C_PINNED"]});
        if let Some(history) = history {
            sidebar["recent_searches"] = history;
        }
        cache
            .conn
            .execute(
                "update workspaces set sidebar = ?1",
                params![sidebar.to_string()],
            )
            .unwrap();
        let restored = cache.load_workspace_for_startup(&session).unwrap().unwrap();
        assert!(restored.priority_sidebar_section);
        assert_eq!(restored.starred_order, ["C_PINNED"]);
        if sidebar["recent_searches"].is_array() {
            assert_eq!(restored.recent_searches.entries().len(), 1);
            assert_eq!(restored.recent_searches.entries()[0].query, "known");
        } else {
            assert!(restored.recent_searches.entries().is_empty());
        }
    }
}
