use super::*;
use crate::slack::models::{Channel, User};
fn workspace() -> Workspace {
    let mut ws = Workspace::from_session(&crate::config::WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        name: "Cache test".into(),
        url: "https://example.invalid".into(),
        token: String::new(),
        enterprise_id: None,
    });
    ws.users.insert(
        "U1".into(),
        User {
            id: "U1".into(),
            name: Some("alex".into()),
            ..Default::default()
        },
    );
    ws.channels.insert(
        "C1".into(),
        Channel {
            id: "C1".into(),
            name: Some("design".into()),
            ..Default::default()
        },
    );
    ws
}
fn trigger(q: &str) -> Trigger {
    Trigger::at(q, q.len(), q.len()).unwrap()
}

#[test]
fn owner_reset_rejects_aba_and_old_failure_cannot_clear_newer_build() {
    let ws = workspace();
    let mut cache = Cache::new("A");
    let old = cache.prepare(&"A", &ws).unwrap();
    let ticket = old.ticket();
    cache.reset("B");
    cache.reset("A");
    let new = cache.prepare(&"A", &ws).unwrap();
    assert!(!cache.publish(&"A", &ws, old.finish()));
    assert!(!cache.failed(&ticket));
    assert!(cache.prepare(&"A", &ws).is_none());
    assert!(cache.publish(&"A", &ws, new.finish()));
    assert!(cache.suggestions(&"B", &ws, &trigger("@alex")).is_none());
    assert_eq!(
        cache.suggestions(&"A", &ws, &trigger("@alex")).unwrap()[0].label,
        "@alex"
    );
}

#[tokio::test]
async fn canonical_rename_during_actual_background_build_refuses_publication_then_recovers() {
    let mut ws = workspace();
    let mut cache = Cache::new(1u64);
    let build = cache.prepare(&1, &ws).unwrap();
    let (go, wait) = std::sync::mpsc::channel();
    let task = tokio::task::spawn_blocking(move || {
        wait.recv().unwrap();
        build.finish()
    });
    ws.users.get_mut("U1").unwrap().name = Some("renamed".into());
    go.send(()).unwrap();
    assert!(!cache.publish(&1, &ws, task.await.unwrap()));
    assert!(cache.suggestions(&1, &ws, &trigger("@alex")).is_none());
    let build = cache.prepare(&1, &ws).unwrap();
    assert!(cache.publish(&1, &ws, build.finish()));
    assert!(
        cache
            .suggestions(&1, &ws, &trigger("@alex"))
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        cache.suggestions(&1, &ws, &trigger("@renamed")).unwrap()[0].label,
        "@renamed"
    );
}

#[test]
fn selection_refuses_current_label_change_deleted_user_archived_channel_and_changed_query() {
    let mut ws = workspace();
    let mut cache = Cache::new(1u64);
    let build = cache.prepare(&1, &ws).unwrap();
    assert!(cache.publish(&1, &ws, build.finish()));
    let query = trigger("@al");
    let choice = cache.suggestions(&1, &ws, &query).unwrap().remove(0);
    let mut composer = crate::ComposerState::with_text("@al");
    composer.set_selection(3, 3);
    assert!(!cache.insert_if_current(&2, &ws, &mut composer, &query, &choice));
    ws.users.get_mut("U1").unwrap().name = Some("alex-new".into());
    assert!(!cache.insert_if_current(&1, &ws, &mut composer, &query, &choice));
    assert_eq!(composer.text, "@al");
    let build = cache.prepare(&1, &ws).unwrap();
    assert!(cache.publish(&1, &ws, build.finish()));
    assert!(!cache.insert_if_current(&1, &ws, &mut composer, &query, &choice)); // Old label remains inadmissible after rebuilding.
    let current = cache.suggestions(&1, &ws, &query).unwrap().remove(0);
    composer = crate::ComposerState::with_text("@ale");
    composer.set_selection(4, 4);
    assert!(!cache.insert_if_current(&1, &ws, &mut composer, &query, &current));
    ws.users.get_mut("U1").unwrap().deleted = true;
    assert!(cache.suggestions(&1, &ws, &query).is_none());
    let build = cache.prepare(&1, &ws).unwrap();
    assert!(cache.publish(&1, &ws, build.finish()));
    let query = trigger("#de");
    let choice = cache.suggestions(&1, &ws, &query).unwrap().remove(0);
    ws.channels.get_mut("C1").unwrap().is_archived = true;
    composer = crate::ComposerState::with_text("#de");
    composer.set_selection(3, 3);
    assert!(!cache.insert_if_current(&1, &ws, &mut composer, &query, &choice));
}

#[test]
fn independent_query_and_selection_calls_detect_direct_map_mutations_without_revision_hooks() {
    let mut ws = workspace();
    let mut cache = Cache::new(1u64);
    let build = cache.prepare(&1, &ws).unwrap();
    assert!(cache.publish(&1, &ws, build.finish()));
    let query = trigger("@al");
    let choice = cache.suggestions(&1, &ws, &query).unwrap().remove(0);
    let mut composer = crate::ComposerState::with_text("@al");
    composer.set_selection(3, 3);
    assert!(cache.insert_if_current(&1, &ws, &mut composer, &query, &choice));
    assert_eq!(composer.text, "<@U1> ");
    ws.users.clear();
    assert!(cache.suggestions(&1, &ws, &query).is_none());
}
