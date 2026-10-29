use super::*;
use crate::bootstrap::test_support::{harness, pending, users};
use serde_json::json;
use super_platinum_core::search::SearchSort;

fn begin(mut state: Signal<ShellState>) -> SearchRequest {
    let query = state.peek().search_query.clone();
    super::super::begin(&mut state.write(), "T1", &query, false).unwrap()
}

#[tokio::test]
async fn actual_people_batch_keeps_order_and_limit_then_opens_unknown_and_known_profiles_without_seeding_users()
 {
    let h = harness();
    let mut state = h.state;
    let client = SlackClient::default();
    let before = users(state);
    let mut records = vec![
        json!({"id":"U_REMOTE","is_admin":true,"profile":{"display_name":"Remote result","title":"Engineer","email":"alex@example.test"}}),
        json!({"id":"U2","profile":{"display_name":"Returned Jules"}}),
        json!({"id":"U_REMOTE"}),
        json!({"id":""}),
    ];
    records.extend((0..35).map(
        |i| json!({"id":format!("U_MORE_{i}"),"profile":{"display_name":format!("Person {i}")}}),
    ));
    let request = begin(state);
    load_with(
        state,
        request,
        &h.transport,
        &client,
        &h.session,
        None,
        |prepared| async move {
            let super_platinum_core::slack::client::RequestBody::Json(body) = prepared.body else {
                panic!("JSON required")
            };
            assert_eq!(body["query"], "alex@example.test");
            assert_eq!(body["count"], 30);
            Ok(json!({"ok":true,"results":records}))
        },
    )
    .await;
    assert_eq!(rows(&state.peek()).len(), 30);
    assert_eq!(rows(&state.peek())[0].id, "U_REMOTE");
    assert_eq!(rows(&state.peek())[1].id, "U2");
    assert_eq!(users(state), before);
    assert_eq!(
        (
            state.peek().search_pager.page,
            state.peek().search_pager.total
        ),
        (1, 0)
    );
    assert!(!state.peek().search_pager.has_more());
    assert_eq!(
        crate::view::search::summary(&state.peek()),
        "30 shown · Up to 30 people"
    );
    let source = Source::capture(&state.peek(), "U_REMOTE").unwrap();
    let Some(Open::ReadOnly(ticket)) = source.prepare(&mut state.write()) else {
        panic!("unknown profile route")
    };
    {
        let shell = state.peek();
        let details = crate::bootstrap::profile_search::presentation(&shell, "U_REMOTE").unwrap();
        assert!(details.loading);
        assert_eq!(
            details
                .user
                .unwrap()
                .profile
                .as_ref()
                .unwrap()
                .display_name
                .as_deref(),
            Some("Remote result")
        );
    }
    crate::bootstrap::profile_search::run_with(state,ticket,||async{
        Ok(vec![serde_json::from_value(json!({"id":"U_REMOTE","profile":{"display_name":"Fresh details","email":"fresh@example.test"}})).unwrap()])
    }).await;
    assert_eq!(
        crate::bootstrap::profile_search::presentation(&state.peek(), "U_REMOTE")
            .unwrap()
            .user
            .unwrap()
            .profile
            .as_ref()
            .unwrap()
            .display_name
            .as_deref(),
        Some("Fresh details")
    );
    assert_eq!(users(state), before);
    state.write().open_search(false);
    let source = Source::capture(&state.peek(), "U2").unwrap();
    let Some(Open::Known { id, visit }) = source.prepare(&mut state.write()) else {
        panic!("known profile route")
    };
    assert_eq!(id, "U2");
    assert!(visit.current(&state.peek()));
    assert!(state.peek().profile_search.is_none());
    assert_eq!(users(state), before);
    assert_eq!(state.peek().core.composer.text, "Keep draft");
    state.write().core.session.as_mut().unwrap().d_cookie = "replaced".into();
    assert!(!visit.current(&state.peek()));
}

#[tokio::test]
async fn held_query_and_loaded_people_owner_cannot_rebind_after_query_or_credential_replacement() {
    let h = harness();
    let mut state = h.state;
    let client = SlackClient::default();
    let before = users(state);
    let request = begin(state);
    let (release, held) = tokio::sync::oneshot::channel();
    let mut work = Box::pin(load_with(
        state,
        request,
        &h.transport,
        &client,
        &h.session,
        None,
        |_| async move {
            held.await.unwrap();
            Ok(json!({"ok":true,"results":[{"id":"U_OLD","profile":{"display_name":"Old query"}}]}))
        },
    ));
    pending(work.as_mut()).await;
    state.write().set_search_query("replacement".into());
    let replacement = begin(state);
    release.send(()).unwrap();
    work.await;
    assert!(state.peek().search_pager.accepts(&replacement));
    assert!(rows(&state.peek()).is_empty());
    load_with(state,replacement,&h.transport,&client,&h.session,None,|_|async{Ok(json!({"ok":true,"results":[{"id":"U_CURRENT","profile":{"display_name":"Current result"}}]}))}).await;
    let old_record = Source::capture(&state.peek(), "U_CURRENT").unwrap();
    let same_value = rows(&state.peek())[0].as_ref().clone();
    state.write().search_people_results.as_mut().unwrap().users[0] = Arc::new(same_value);
    assert!(old_record.prepare(&mut state.write()).is_none());
    let stale = Source::capture(&state.peek(), "U_CURRENT").unwrap();
    assert_eq!(rows(&state.peek()).len(), 1);
    state.write().core.session.as_mut().unwrap().d_cookie = "replaced".into();
    assert!(retired(&state.peek()));
    assert!(rows(&state.peek()).is_empty());
    assert!(Source::capture(&state.peek(), "U_CURRENT").is_none());
    assert!(stale.prepare(&mut state.write()).is_none());
    assert!(state.peek().profile_user.is_none());
    assert_eq!(users(state), before);
}

#[tokio::test]
async fn people_failure_retry_and_recent_replay_preserve_message_file_sort_and_hide_unsupported_controls()
 {
    let h = harness();
    let mut state = h.state;
    let client = SlackClient::default();
    state.write().set_search_kind(SearchKind::Messages);
    state.write().search_pager.change_sort(SearchSort::Oldest);
    state.write().search_filters = Some(Default::default());
    state.write().set_search_kind(SearchKind::People);
    assert!(state.peek().search_filters.is_none());
    assert!(!crate::view::search::supports_filters(&state.peek()));
    let request = begin(state);
    load_with(
        state,
        request,
        &h.transport,
        &client,
        &h.session,
        None,
        |_| async { Ok(json!({"ok":true})) },
    )
    .await;
    assert!(state.peek().search_pager.failed && !state.peek().search_pager.loading);
    let request = begin(state);
    load_with(
        state,
        request,
        &h.transport,
        &client,
        &h.session,
        None,
        |_| async { Ok(json!({"ok":true,"results":[]})) },
    )
    .await;
    assert!(!state.peek().search_pager.failed && rows(&state.peek()).is_empty());
    let entry = crate::view::search::history::entries(&state.peek())[0].clone();
    assert_eq!(entry.kind, SearchKind::People);
    assert_eq!(entry.sort, SearchSort::Oldest);
    assert_eq!(crate::view::search::history::detail(&entry), "People");
    state.write().set_search_kind(SearchKind::Files);
    assert!(crate::view::search::supports_filters(&state.peek()));
    state.write().search_history_open = true;
    let replay = crate::view::search::history::Selection::capture(&state.peek(), &entry).unwrap();
    assert!(crate::view::search::history::prepare(
        &mut state.write(),
        replay
    ));
    assert_eq!(state.peek().search_pager.kind, SearchKind::People);
    assert_eq!(state.peek().search_pager.sort, SearchSort::Oldest);
    state.write().set_search_kind(SearchKind::Messages);
    assert_eq!(state.peek().search_pager.sort, SearchSort::Oldest);
    assert!(crate::view::search::supports_filters(&state.peek()));
    assert_eq!(state.peek().core.composer.text, "Keep draft");
}

#[test]
fn people_and_read_only_profile_fixture_variants_prepare_the_new_states() {
    for name in [
        "search-people",
        "search-people-narrow",
        "search-people-loading",
        "search-people-loading-narrow",
        "search-people-empty",
        "search-people-empty-narrow",
        "search-people-error",
        "search-people-error-narrow",
        "profile-people-read-only",
        "profile-people-read-only-narrow",
        "profile-people-read-only-error",
        "profile-people-read-only-error-narrow",
    ] {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        install(&mut shell, name);
        assert_eq!(shell.search_pager.kind, SearchKind::People);
        if name.starts_with("profile-") {
            let pane = crate::bootstrap::profile_search::presentation(&shell, "U_REMOTE").unwrap();
            assert!(!pane.loading && pane.user.is_some());
            assert_eq!(pane.error.is_some(), name.contains("-error"));
            assert!(!shell.core.workspaces["T1"].users.contains_key("U_REMOTE"));
        } else if name.contains("-loading") {
            assert!(shell.search_pager.loading);
        } else if name.contains("-error") {
            assert!(shell.search_pager.failed);
        } else {
            assert_eq!(
                rows(&shell).len(),
                if name.contains("-empty") { 0 } else { 2 }
            );
            assert!(!shell.search_pager.has_more());
        }
    }
}
