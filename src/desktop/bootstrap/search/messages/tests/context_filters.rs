use super::*;
use super_platinum_core::search_query::SearchFilters;

#[tokio::test]
async fn applied_person_and_thread_filters_reach_actual_message_request_then_clear_and_replay() {
    let (_dom, mut state, transport, session) = setup("U2", false);
    let users = serde_json::to_value(&state.peek().core.workspaces["T1"].users).unwrap();
    state.write().search_filters = Some(SearchFilters {
        with_person: "@Jules".into(),
        in_thread: true,
        saved: true,
        pinned: true,
        ..Default::default()
    });
    assert!(state.write().apply_search_filters());
    let applied = "release with:@Jules is:thread is:saved has:pin";
    assert_eq!(state.peek().search_query, applied);
    let request = begin(state);
    let payload = page(state, "U2");
    load_with_profiles(
        state,
        request,
        &transport,
        &SlackClient::default(),
        &session,
        None,
        |request| async move {
            assert!(request.url.contains("search.modules.messages"));
            let RequestBody::Form(fields) = request.body else {
                panic!("form required")
            };
            let fields = fields
                .into_iter()
                .collect::<std::collections::BTreeMap<_, _>>();
            assert_eq!(fields["query"], applied);
            assert_eq!(fields["sort"], "timestamp");
            assert_eq!(fields["sort_dir"], "asc");
            assert_eq!(fields["page"], "1");
            Ok(payload)
        },
        |_| async { panic!("known author must not trigger metadata lookup") },
    )
    .await;
    assert_eq!(state.peek().search_results.len(), 1);
    assert_eq!(
        state.peek().search_results[0].thread_ts.as_deref(),
        Some("120.000000")
    );
    let entry = crate::view::search::history::entries(&state.peek())[0].clone();
    assert_eq!(entry.query, applied);
    state.write().search_filters = Some(SearchFilters::default());
    assert!(state.write().apply_search_filters());
    assert_eq!(state.peek().search_query, "release    ");
    assert!(state.peek().search_filters.is_none());
    crate::view::search::history::toggle(&mut state.write());
    let selected = crate::view::search::history::Selection::capture(&state.peek(), &entry).unwrap();
    assert!(crate::view::search::history::prepare(
        &mut state.write(),
        selected
    ));
    assert_eq!(state.peek().search_query, applied);
    let restored = SearchFilters::from_query(&state.peek().search_query);
    assert_eq!(restored.with_person, "@Jules");
    assert!(restored.in_thread);
    assert!(restored.saved && restored.pinned);
    assert_eq!(state.peek().search_pager.kind, SearchKind::Messages);
    assert_eq!(state.peek().search_pager.sort, SearchSort::Oldest);
    assert_eq!(state.peek().core.composer.text, "Keep draft");
    assert_eq!(
        serde_json::to_value(&state.peek().core.workspaces["T1"].users).unwrap(),
        users
    );
}
