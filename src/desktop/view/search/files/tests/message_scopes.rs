//! Generated shared controls -> real Messages builder/decoder/page/cache path.
use super::*;
use crate::{
    bootstrap::search_message_scope_io::Io,
    state::search_message_scopes::{
        self as intent,
        tests::{self as fixture, C1, C2, U1, U2},
    },
};
use super_platinum_core::{search::SearchSort, search_query::message_scopes::Scope};
fn input(dom: &VirtualDom, id: ElementId, value: &str) {
    let data = dioxus::html::EventData::Form(dioxus::html::SerializedFormData::new(
        value.into(),
        Vec::new(),
    ));
    dom.in_scope(ScopeId::ROOT, || {
        dom.runtime()
            .handle_event("input", dioxus::core::Event::new(data.into_any(), true), id)
    });
}
fn setup(extra: usize) -> (VirtualDom, Signal<ShellState>, Rc<Io>, Attributes) {
    static EVENTS: std::sync::Once = std::sync::Once::new();
    EVENTS.call_once(|| {
        dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter))
    });
    let h = fixture::setup(extra);
    let mut old = h.state;
    let shell = std::mem::replace(
        &mut *old.write(),
        ShellState::fixture(crate::media::MediaRegistry::default()),
    );
    type Input = Rc<RefCell<Option<ShellState>>>;
    type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
    let slot = Slot::default();
    let mut dom = VirtualDom::new_with_props(
        |(input, slot): (Input, Slot)| {
            let state = use_signal(|| input.borrow_mut().take().unwrap());
            slot.set(Some(state));
            rsx! {{super::super::super::controls(state,&state.read())}{super::super::super::message_page_controls(state,&state.read())}{super::super::super::message_results(state,&state.read())}}
        },
        (Rc::new(RefCell::new(Some(shell))), slot.clone()),
    );
    let mut attributes = Attributes::new();
    record(&mut attributes, dom.rebuild_to_vec().edits);
    let state = slot.get().unwrap();
    let io = Rc::new(Io::default());
    dom.in_scope(ScopeId::ROOT, || provide_context(io.clone()));
    (dom, state, io, attributes)
}
fn open(dom: &mut VirtualDom, a: &mut Attributes) {
    click(
        dom,
        mounted_element(dom, "aria-controls", "search-filter-panel"),
    );
    record(a, dom.render_immediate_to_vec().edits);
}
#[tokio::test]
async fn actual_multi_channel_author_apply_pages_cached_return_refine_scoped_clear_cancel_and_noop_use_original_read_path()
 {
    use crate::bootstrap::search_message_pages as pages;
    let (mut dom, state, io, mut a) = setup(2);
    open(&mut dom, &mut a);
    assert!(intent::Source::capture(&state.peek()).is_some());
    for id in [C1, C2, U1, U2] {
        change(
            &dom,
            mounted_element(&dom, "data-message-scope-id", id),
            "true",
        );
        record(&mut a, dom.render_immediate_to_vec().edits);
    }
    input(
        &dom,
        mounted_element(&dom, "aria-label", "On date"),
        "2026-10-29",
    );
    change(
        &dom,
        mounted_element(&dom, "aria-label", "Has files"),
        "true",
    );
    record(&mut a, dom.render_immediate_to_vec().edits);
    assert_eq!(state.peek().search_query, "release");
    assert!(io.reads.borrow().is_empty());
    assert_eq!(intent::count(&state.peek(), Scope::Channels), 2);
    assert_eq!(intent::count(&state.peek(), Scope::Authors), 2);
    let query = "release in:<#C12345678|ship> in:<#C87654321|design> from:<@U12345678> from:<@W87654321> on:2026-10-29 has:file";
    let reply = io.expect(query, 1, SearchSort::Newest);
    click(
        &dom,
        mounted_element(&dom, "data-file-filter-apply", "true"),
    );
    pump(&mut dom, &mut a, || io.reads.borrow().len() == 1).await;
    assert_eq!(state.peek().search_query, query);
    reply.send(Ok(fixture::response(1))).unwrap();
    pump(&mut dom, &mut a, || pages::active(&state.peek())).await;
    assert_eq!(state.peek().search_pager.page, 1);
    let reply = io.expect(query, 2, SearchSort::Newest);
    click(&dom, mounted_element(&dom, "id", "message-search-next"));
    pump(&mut dom, &mut a, || io.reads.borrow().len() == 2).await;
    reply.send(Ok(fixture::response(2))).unwrap();
    pump(&mut dom, &mut a, || {
        state.peek().search_pager.page == 2 && !state.peek().search_pager.loading
    })
    .await;
    assert_eq!(state.peek().search_results[0].text, "Page 2 release");
    click(&dom, mounted_element(&dom, "id", "message-search-previous"));
    pump(&mut dom, &mut a, || state.peek().search_pager.page == 1).await;
    assert_eq!(
        io.reads.borrow().len(),
        2,
        "cached page return performs no read"
    );
    assert_eq!(state.peek().search_results[0].text, "Page 1 release");
    // Cache preservation must hold AFTER the spawned read reaches transport,
    // while its response is withheld, and after success for the other page.
    open(&mut dom, &mut a);
    let reply = io.expect(query, 1, SearchSort::Newest);
    click(
        &dom,
        mounted_element(&dom, "data-file-filter-apply", "true"),
    );
    pump(&mut dom, &mut a, || io.reads.borrow().len() == 3).await;
    assert!(pages::has_scope(&state.peek()));
    assert!(pages::active(&state.peek()));
    assert_eq!(state.peek().search_results[0].text, "Page 1 release");
    assert_eq!(
        pages::rows(&state.peek()).len(),
        1,
        "loaded rows remain while refresh is withheld"
    );
    reply.send(Ok(fixture::response(1))).unwrap();
    pump(&mut dom, &mut a, || !state.peek().search_pager.loading).await;
    click(&dom, mounted_element(&dom, "id", "message-search-next"));
    pump(&mut dom, &mut a, || state.peek().search_pager.page == 2).await;
    assert_eq!(
        io.reads.borrow().len(),
        3,
        "cached page 2 survives the unchanged-query read"
    );
    click(&dom, mounted_element(&dom, "id", "message-search-previous"));
    pump(&mut dom, &mut a, || state.peek().search_pager.page == 1).await;
    open(&mut dom, &mut a);
    change(
        &dom,
        mounted_element(&dom, "data-message-scope-id", C2),
        "false",
    );
    change(
        &dom,
        mounted_element(&dom, "data-message-scope-id", U2),
        "false",
    );
    record(&mut a, dom.render_immediate_to_vec().edits);
    assert_eq!(state.peek().search_query, query);
    let refined = query
        .replace("in:<#C87654321|design>", "")
        .replace("from:<@W87654321>", "");
    let reply = io.expect(&refined, 1, SearchSort::Newest);
    click(
        &dom,
        mounted_element(&dom, "data-file-filter-apply", "true"),
    );
    pump(&mut dom, &mut a, || io.reads.borrow().len() == 4).await;
    assert!(
        !pages::has_scope(&state.peek()),
        "changed trimmed query clears the old Book before response"
    );
    reply.send(Ok(fixture::response(1))).unwrap();
    pump(&mut dom, &mut a, || pages::active(&state.peek())).await;
    open(&mut dom, &mut a);
    click(
        &dom,
        mounted_element(&dom, "data-message-scope-clear", "channels"),
    );
    record(&mut a, dom.render_immediate_to_vec().edits);
    assert_eq!(intent::count(&state.peek(), Scope::Channels), 0);
    assert_eq!(intent::count(&state.peek(), Scope::Authors), 1);
    assert_eq!(
        state.peek().search_filters.as_ref().unwrap().on,
        "2026-10-29"
    );
    click(
        &dom,
        mounted_element(&dom, "data-message-scope-clear", "authors"),
    );
    record(&mut a, dom.render_immediate_to_vec().edits);
    assert_eq!(intent::count(&state.peek(), Scope::Authors), 0);
    assert_eq!(state.peek().search_query, refined);
    open(&mut dom, &mut a);
    assert!(state.peek().search_filters.is_none());
    assert_eq!(io.reads.borrow().len(), 4);
    assert!(
        pages::active(&state.peek()),
        "cancel retains the loaded results and query"
    );
    open(&mut dom, &mut a);
    click(
        &dom,
        mounted_element(&dom, "data-file-filter-clear", "true"),
    );
    record(&mut a, dom.render_immediate_to_vec().edits);
    assert_eq!(state.peek().search_query, refined);
    assert!(state.peek().search_filters.as_ref().unwrap().on.is_empty());
    assert_eq!(intent::count(&state.peek(), Scope::Channels), 0);
    assert_eq!(intent::count(&state.peek(), Scope::Authors), 0);
    let reply = io.expect("release", 1, SearchSort::Newest);
    click(
        &dom,
        mounted_element(&dom, "data-file-filter-apply", "true"),
    );
    pump(&mut dom, &mut a, || io.reads.borrow().len() == 5).await;
    assert_eq!(state.peek().search_query.trim(), "release");
    reply.send(Ok(fixture::response(1))).unwrap();
    pump(&mut dom, &mut a, || pages::active(&state.peek())).await;
    assert_eq!(io.completed.get(), 5);
    assert!(state.peek().viewer.is_none());
}
#[tokio::test]
async fn actual_cached_find_reaches_beyond_thirty_rows_without_read_and_deleted_original_choice_refuses_apply()
 {
    let (mut dom, mut state, io, mut a) = setup(40);
    open(&mut dom, &mut a);
    for i in 0..40 {
        input(
            &dom,
            mounted_element(&dom, "aria-label", "Find known people"),
            &format!("person{i:02}"),
        );
        record(&mut a, dom.render_immediate_to_vec().edits);
        let id = if i == 0 {
            U1.into()
        } else if i == 1 {
            U2.into()
        } else {
            format!("U{i:08}")
        };
        change(
            &dom,
            mounted_element(&dom, "data-message-scope-id", &id),
            "true",
        );
        record(&mut a, dom.render_immediate_to_vec().edits);
    }
    assert_eq!(intent::count(&state.peek(), Scope::Authors), 40);
    assert!(io.reads.borrow().is_empty());
    assert_eq!(state.peek().search_query, "release");
    input(
        &dom,
        mounted_element(&dom, "aria-label", "Find known people"),
        "",
    );
    record(&mut a, dom.render_immediate_to_vec().edits);
    assert_eq!(
        intent::rows(&state.peek(), Scope::Authors).len(),
        30,
        "bounded render window is separate from selected collection"
    );
    state
        .write()
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .users
        .get_mut("U00000039")
        .unwrap()
        .deleted = true;
    click(
        &dom,
        mounted_element(&dom, "data-file-filter-apply", "true"),
    );
    record(&mut a, dom.render_immediate_to_vec().edits);
    assert_eq!(state.peek().search_query, "release");
    assert!(intent::has_draft(&state.peek()));
    assert!(state.peek().search_filter_error.is_some());
    assert!(
        io.reads.borrow().is_empty(),
        "metadata retirement cannot publish a query or start a read"
    );
    open(&mut dom, &mut a);
    open(&mut dom, &mut a);
    input(
        &dom,
        mounted_element(&dom, "aria-label", "Find known people"),
        "person39",
    );
    record(&mut a, dom.render_immediate_to_vec().edits);
    assert!(intent::rows(&state.peek(), Scope::Authors).is_empty());
    assert_eq!(intent::count(&state.peek(), Scope::Authors), 0);
}
