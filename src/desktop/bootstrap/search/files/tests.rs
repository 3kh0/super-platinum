use super::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use super_platinum_core::{search::SearchSort, slack::client::RequestBody};

fn setup() -> (
    VirtualDom,
    Signal<ShellState>,
    Arc<Transport>,
    WorkspaceSession,
) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    #[cfg(feature = "protected-native-fixture")]
    let transport = Arc::new(Transport::disconnected_fixture("synthetic"));
    #[cfg(not(feature = "protected-native-fixture"))]
    let transport = Arc::new(Transport::new("synthetic").unwrap());
    shell.core.transport = Some(transport.clone());
    shell.core.active_account = Some("file-search-disconnected".into());
    shell.self_account.user_id = shell.core.workspaces["T1"].self_user_id.clone();
    shell.overlay = Some(Overlay::Search);
    shell.set_search_query("\"release notes\" in:ship".into());
    shell.set_search_kind(SearchKind::Files);
    shell.search_pager.change_sort(SearchSort::Oldest);
    let session = WorkspaceSession {
        team_id: "T1".into(),
        user_id: shell.self_account.user_id.clone(),
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "synthetic".into(),
        enterprise_id: None,
    };
    type Props = (
        Rc<RefCell<Option<ShellState>>>,
        Rc<Cell<Option<Signal<ShellState>>>>,
    );
    let slot = Rc::new(Cell::new(None));
    let mut dom = VirtualDom::new_with_props(
        |(input, slot): Props| {
            let state = use_signal(|| input.borrow_mut().take().unwrap());
            slot.set(Some(state));
            VNode::empty()
        },
        (Rc::new(RefCell::new(Some(shell))), slot.clone()),
    );
    dom.rebuild_to_vec();
    (dom, slot.get().unwrap(), transport, session)
}
fn begin(mut state: Signal<ShellState>, append: bool) -> SearchRequest {
    let query = state.peek().search_query.clone();
    super::super::begin(&mut state.write(), "T1", &query, append).unwrap()
}
fn file(id: &str) -> Value {
    json!({"id":id,"title":format!("Release {id}"),"name":format!("{id}.txt"),"mimetype":"text/plain","filetype":"text","mode":"hosted","user":"U1","url_private":format!("https://files.slack.com/files-pri/T1-{id}/{id}.txt")})
}
async fn page(
    state: Signal<ShellState>,
    transport: &Arc<Transport>,
    session: &WorkspaceSession,
    number: u32,
    items: Value,
) {
    let request = begin(state, number > 1);
    load_with(state,request,transport,&SlackClient::default(),session,|request|async move{
        let RequestBody::Form(fields)=request.body else {panic!("form required")};
        let fields=fields.into_iter().collect::<std::collections::BTreeMap<_,_>>();
        assert_eq!(fields["page"],number.to_string());assert_eq!(fields["query"],"\"release notes\" in:ship");
        Ok(json!({"ok":true,"files":{"matches":items,"paging":{"page":number,"pages":3,"total":42,"count":20}}}))
    }).await;
}
fn ids(state: Signal<ShellState>) -> Vec<String> {
    state
        .peek()
        .search_file_results
        .iter()
        .filter_map(|f| f.id.clone())
        .collect()
}

#[tokio::test]
async fn file_query_pages_failure_retry_and_actual_preview_form_one_offline_journey() {
    let (dom, mut state, transport, session) = setup();
    let channel = state.peek().core.active_channel.clone();
    state.write().core.composer.text = "Unsent conversation draft".into();
    page(
        state,
        &transport,
        &session,
        1,
        json!([file("F1"),file("F1"),{}, {"id":"FDEAD","mode":"tombstone"}]),
    )
    .await;
    assert_eq!(ids(state), ["F1"]);
    assert_eq!(state.peek().search_pager.total, 42);
    let request = begin(state, true);
    load_with(
        state,
        request,
        &transport,
        &SlackClient::default(),
        &session,
        |_| async { Err(Error::Offline("synthetic".into())) },
    )
    .await;
    assert_eq!(ids(state), ["F1"]);
    assert_eq!(state.peek().search_pager.page, 1);
    assert!(state.peek().search_pager.failed);
    page(
        state,
        &transport,
        &session,
        2,
        json!([file("F1"), file("F2")]),
    )
    .await;
    assert_eq!(ids(state), ["F1", "F2"]);
    assert!(!state.peek().search_pager.failed);
    let source = Source::capture(&state.peek(), "F2").unwrap();
    dom.in_runtime(|| open(state, source));
    {
        let shell = state.peek();
        assert_eq!(shell.overlay, Some(Overlay::Viewer));
        let viewer = shell.viewer.as_ref().unwrap();
        assert_eq!(viewer.name, "Release F2");
        assert_eq!(viewer.download_name, "F2.txt");
        assert_eq!(viewer.file.as_ref().unwrap().id, "F2");
        assert_eq!(shell.core.active_channel, channel);
        assert_eq!(shell.core.composer.text, "Unsent conversation draft");
        assert!(shell.core.session.is_none());
    }
    state.write().dismiss_overlay();
    state.write().signed_in = true;
    state.write().loading = false;
    state.write().open_search(false);
    assert_eq!(state.peek().search_pager.kind, SearchKind::Files);
    assert_eq!(ids(state), ["F1", "F2"]);
    assert_eq!(state.peek().search_query, "\"release notes\" in:ship");
}

#[tokio::test]
async fn result_types_keep_query_sort_and_refuse_late_success_and_failure() {
    let (_dom, mut state, transport, session) = setup();
    let old = begin(state, false);
    let query = state.peek().search_query.clone();
    state.write().set_search_kind(SearchKind::Messages);
    let messages = begin(state, false);
    state.write().set_search_kind(SearchKind::Files);
    let current = begin(state, false);
    finish(
        &mut state.write(),
        &old,
        &transport,
        Ok(api::SearchFilesPage {
            items: vec![serde_json::from_value(file("FSTALE")).unwrap()],
            pagination: None,
        }),
    );
    finish(
        &mut state.write(),
        &old,
        &transport,
        Err(Error::Offline("synthetic".into())),
    );
    assert!(state.peek().search_pager.accepts(&current));
    assert!(!state.peek().search_pager.accepts(&messages));
    assert_eq!(state.peek().search_query, query);
    assert_eq!(state.peek().search_pager.sort, SearchSort::Oldest);
    assert!(ids(state).is_empty());
    assert!(state.peek().search_results.is_empty());
    load_with(
        state,
        current,
        &transport,
        &SlackClient::default(),
        &session,
        |_| async { Ok(json!({"ok":true,"files":{"matches":[file("FCURRENT")]}})) },
    )
    .await;
    assert_eq!(ids(state), ["FCURRENT"]);
    let source = Source::capture(&state.peek(), "FCURRENT").unwrap();
    state.write().core.account_epoch += 1;
    open(state, source);
    assert_eq!(state.peek().overlay, Some(Overlay::Search));
    assert!(state.peek().viewer.is_none());
}

#[tokio::test]
async fn received_canvas_and_list_results_use_existing_readers_without_download_shape() {
    let (dom, mut state, transport, session) = setup();
    page(
        state,
        &transport,
        &session,
        1,
        json!([
            {"id":"FCANVAS","title":"Release canvas","filetype":"quip"},
            {"id":"FLIST","title":"Release tasks","mode":"list"}
        ]),
    )
    .await;
    let canvas = Source::capture(&state.peek(), "FCANVAS").unwrap();
    dom.in_runtime(|| open(state, canvas));
    assert_eq!(state.peek().overlay, Some(Overlay::Canvas));
    assert_eq!(state.peek().canvas.as_ref().unwrap().id, "FCANVAS");
    state.write().dismiss_overlay();
    state.write().overlay = Some(Overlay::Search);
    let list = Source::capture(&state.peek(), "FLIST").unwrap();
    dom.in_runtime(|| open(state, list));
    assert_eq!(state.peek().overlay, Some(Overlay::SlackList));
    assert!(state.peek().viewer.is_none());
    assert!(state.peek().core.session.is_none());
}

#[tokio::test]
async fn verified_file_names_reconcile_retained_rows_and_an_in_flight_page_before_preview() {
    let (dom, mut state, transport, session) = setup();
    page(state, &transport, &session, 1, json!([file("F1")])).await;
    let request = begin(state, true);
    load_with(state, request, &transport, &SlackClient::default(), &session, |_| async move {
        for id in ["F1", "F2"] {
            let verified = serde_json::from_value(json!({
                "id":id,"title":format!("Verified {id}"),"name":format!("current-{id}.txt"),
                "url_private":"https://unrelated.example/must-not-replace-preview"
            })).unwrap();
            assert!(crate::file_events::reconcile_metadata(&mut state.write(), "T1", &verified));
        }
        Ok(json!({"ok":true,"files":{"matches":[file("F2")],"paging":{"page":2,"pages":3,"total":42,"count":20}}}))
    }).await;
    assert_eq!(
        state.peek().search_file_results[0].title.as_deref(),
        Some("Verified F1")
    );
    assert_eq!(
        state.peek().search_file_results[1].title.as_deref(),
        Some("Verified F2")
    );
    let retained = Source::capture(&state.peek(), "F1").unwrap();
    let verified = serde_json::from_value(json!({"id":"F1","title":"Latest F1"})).unwrap();
    assert!(crate::file_events::reconcile_metadata(
        &mut state.write(),
        "T1",
        &verified
    ));
    // The old row callback resolves today's retained metadata by ID.
    dom.in_runtime(|| open(state, retained));
    assert_eq!(state.peek().viewer.as_ref().unwrap().name, "Latest F1");
    state.write().dismiss_overlay();
    state.write().overlay = Some(Overlay::Search);
    let source = Source::capture(&state.peek(), "F2").unwrap();
    dom.in_runtime(|| open(state, source));
    {
        let shell = state.peek();
        let viewer = shell.viewer.as_ref().unwrap();
        assert_eq!(viewer.name, "Verified F2");
        assert_eq!(viewer.download_name, "current-F2.txt");
        assert_eq!(
            shell.media.original_download_url(&viewer.id).as_deref(),
            Some("https://files.slack.com/files-pri/T1-F2/F2.txt")
        );
    }
    state.write().dismiss_overlay();
    state.write().overlay = Some(Overlay::Search);
    let source = Source::capture(&state.peek(), "F2").unwrap();
    state.write().core.active_team = Some("OTHER".into());
    open(state, source);
    assert_eq!(state.peek().overlay, Some(Overlay::Search));
}

#[tokio::test]
async fn exact_date_filter_reaches_file_request_and_invalid_edit_keeps_the_openable_results() {
    use super_platinum_core::search_query::SearchFilters;
    let (dom, mut state, transport, session) = setup();
    let mut filters = SearchFilters::from_query(&state.peek().search_query);
    filters.on = "2024-02-29".into();
    state.write().search_filters = Some(filters);
    assert!(state.write().apply_search_filters());
    let query = "\"release notes\" in:ship on:2024-02-29";
    assert_eq!(state.peek().search_query, query);
    assert_eq!(state.peek().search_pager.kind, SearchKind::Files);
    assert_eq!(state.peek().search_pager.sort, SearchSort::Oldest);
    let request = begin(state, false);
    load_with(state, request, &transport, &SlackClient::default(), &session, |request| async move {
        let RequestBody::Form(fields) = request.body else { panic!("form required") };
        let fields = fields.into_iter().collect::<std::collections::BTreeMap<_,_>>();
        assert_eq!(fields["query"], query);
        assert_eq!(fields["sort_dir"], "asc");
        Ok(json!({"ok":true,"files":{"matches":[file("FLEAP")],"paging":{"page":1,"pages":1,"total":1,"count":20}}}))
    }).await;
    let source = Source::capture(&state.peek(), "FLEAP").unwrap();
    let revision = state.peek().search_pager.revision();
    let visit = state.peek().search_open_generation;
    let mut filters = SearchFilters::from_query(query);
    filters.on = "2026-02-30".into();
    state.write().search_filters = Some(filters);
    assert!(!state.write().apply_search_filters());
    assert_eq!(state.peek().search_query, query);
    assert_eq!(ids(state), ["FLEAP"]);
    assert_eq!(state.peek().search_pager.revision(), revision);
    assert_eq!(state.peek().search_open_generation, visit);
    assert_eq!(
        state.peek().search_filter_error,
        Some("Use YYYY-MM-DD for dates.")
    );
    assert_eq!(
        state.peek().search_filters.as_ref().unwrap().on,
        "2026-02-30"
    );
    dom.in_runtime(|| open(state, source));
    assert_eq!(state.peek().overlay, Some(Overlay::Viewer));
    assert_eq!(
        state
            .peek()
            .viewer
            .as_ref()
            .unwrap()
            .file
            .as_ref()
            .unwrap()
            .id,
        "FLEAP"
    );
}

#[tokio::test]
async fn admitted_file_query_can_be_replayed_from_local_history_into_the_real_preview() {
    use crate::view::search::history;
    use super_platinum_core::search_query::SearchFilters;
    let (dom, mut state, transport, session) = setup();
    // Missing credentials and rejected/append admission must not add history.
    super::super::search(state).await;
    assert!(history::entries(&state.peek()).is_empty());
    let failed = begin(state, false);
    load_with(
        state,
        failed,
        &transport,
        &SlackClient::default(),
        &session,
        |_| std::future::ready(Err(Error::Offline("synthetic".into()))),
    )
    .await;
    assert_eq!(history::entries(&state.peek()).len(), 1);
    assert_eq!(
        history::entries(&state.peek())[0].query,
        "\"release notes\" in:ship"
    );
    assert!(state.peek().search_pager.failed);
    page(state, &transport, &session, 1, json!([file("FHISTORY")])).await;
    let entry = history::entries(&state.peek())[0].clone();
    // Make recording observable: an erroneous append/rejected record would
    // move this query ahead of a distinct MRU head, instead of being a no-op.
    state
        .write()
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .recent_searches
        .record(
            "distinct MRU head",
            SearchKind::Messages,
            SearchSort::Newest,
        );
    let before = history::entries(&state.peek()).to_vec();
    let append = begin(state, true);
    assert!(super::super::begin(&mut state.write(), "T1", &entry.query, false).is_none());
    finish(
        &mut state.write(),
        &append,
        &transport,
        Err(Error::Offline("synthetic".into())),
    );
    assert_eq!(history::entries(&state.peek()), before.as_slice());

    state.write().set_search_query("different".into());
    state.write().set_search_kind(SearchKind::Messages);
    state.write().search_pager.change_sort(SearchSort::Relevant);
    let draft = SearchFilters {
        on: "2026-02-30".into(),
        ..Default::default()
    };
    state.write().search_filters = Some(draft.clone());
    history::toggle(&mut state.write());
    let old = history::Selection::capture(&state.peek(), &entry).unwrap();
    history::toggle(&mut state.write());
    assert_eq!(state.peek().search_filters.as_ref(), Some(&draft));
    history::toggle(&mut state.write());
    assert!(!history::prepare(&mut state.write(), old));
    let whitespace = history::Selection::capture(&state.peek(), &entry).unwrap();
    state.write().set_search_query("different ".into());
    assert!(!history::prepare(&mut state.write(), whitespace));
    history::toggle(&mut state.write());
    let foreign = history::Selection::capture(&state.peek(), &entry).unwrap();
    let mut other = state.peek().core.workspaces["T1"].clone();
    other.team_id = "OTHER".into();
    other.recent_searches = Default::default();
    state.write().core.workspaces.insert("OTHER".into(), other);
    state.write().core.active_team = Some("OTHER".into());
    assert!(history::entries(&state.peek()).is_empty());
    assert!(!history::prepare(&mut state.write(), foreign));
    state.write().core.active_team = Some("T1".into());
    let selection = history::Selection::capture(&state.peek(), &entry).unwrap();
    assert!(history::prepare(&mut state.write(), selection));
    assert_eq!(state.peek().search_query, entry.query);
    assert_eq!(state.peek().search_pager.kind, SearchKind::Files);
    assert_eq!(state.peek().search_pager.sort, SearchSort::Oldest);
    assert!(!state.peek().search_history_open);
    assert!(state.peek().search_filters.is_none());
    assert!(state.peek().search_file_results.is_empty());

    let request = begin(state, false);
    load_with(state, request, &transport, &SlackClient::default(), &session, |request| async move {
        assert_eq!(url::Url::parse(&request.url).unwrap().path(), "/api/search.files");
        let RequestBody::Form(fields) = request.body else { panic!("form required") };
        let fields = fields.into_iter().collect::<std::collections::BTreeMap<_,_>>();
        assert_eq!(fields["query"], "\"release notes\" in:ship");
        assert_eq!(fields["sort"], "timestamp");
        assert_eq!(fields["sort_dir"], "asc");
        assert_eq!(fields["page"], "1");
        Ok(json!({"ok":true,"files":{"matches":[file("FREPLAY")],"paging":{"page":1,"pages":1,"total":1,"count":20}}}))
    }).await;
    let source = Source::capture(&state.peek(), "FREPLAY").unwrap();
    dom.in_runtime(|| open(state, source));
    assert_eq!(state.peek().overlay, Some(Overlay::Viewer));
    assert_eq!(
        state
            .peek()
            .viewer
            .as_ref()
            .unwrap()
            .file
            .as_ref()
            .unwrap()
            .id,
        "FREPLAY"
    );
    assert!(state.peek().core.session.is_none());
}
