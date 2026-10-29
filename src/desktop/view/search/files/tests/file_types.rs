use super::*;
use crate::{
    bootstrap::{search_file_type_io::Io, test_support},
    state::search_filter_types as intent,
};
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
#[tokio::test]
async fn actual_file_category_checkboxes_stage_apply_exact_query_clear_and_cancel_without_global_preference_writes()
 {
    static EVENTS: std::sync::Once = std::sync::Once::new();
    EVENTS.call_once(|| {
        dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter))
    });
    let h = test_support::harness();
    let mut old = h.state;
    {
        let mut s = old.write();
        s.set_search_kind(SearchKind::Files);
        s.set_search_query("release -type:pdfs type:videos".into());
        assert!(s.core.cache.is_none());
    }
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
            super::super::super::controls(state, &state.read())
        },
        (Rc::new(RefCell::new(Some(shell))), slot.clone()),
    );
    let mut attributes = Attributes::new();
    record(&mut attributes, dom.rebuild_to_vec().edits);
    let mut state = slot.get().unwrap();
    let io = Rc::new(Io::default());
    dom.in_scope(ScopeId::ROOT, || provide_context(io.clone()));
    click(
        &dom,
        mounted_element(&dom, "aria-controls", "search-filter-panel"),
    );
    record(&mut attributes, dom.render_immediate_to_vec().edits);
    assert!(intent::Source::capture(&state.peek()).is_some());
    change(
        &dom,
        mounted_element(&dom, "id", "search-file-type-images"),
        "true",
    );
    input(
        &dom,
        mounted_element(&dom, "aria-label", "On date"),
        "2026-10-29",
    );
    record(&mut attributes, dom.render_immediate_to_vec().edits);
    assert_eq!(state.peek().search_query, "release -type:pdfs type:videos");
    assert!(io.queries.borrow().is_empty());
    let desired = "release -type:pdfs type:videos type:images on:2026-10-29";
    let reply = io.expect(desired);
    click(
        &dom,
        mounted_element(&dom, "data-file-filter-apply", "true"),
    );
    pump(&mut dom, &mut attributes, || io.queries.borrow().len() == 1).await;
    assert_eq!(state.peek().search_query, desired);
    assert!(state.peek().search_file_type_filters.is_none());
    reply.send(Ok(serde_json::json!({"ok":true,"files":{"matches":[{"id":"FCATEGORY","title":"Returned file"}],"pagination":{"page":1,"page_count":1,"total_count":1}}}))).unwrap();
    pump(&mut dom, &mut attributes, || {
        !state.peek().search_pager.loading
    })
    .await;
    assert_eq!(state.peek().search_file_results.len(), 1);
    assert!(state.peek().viewer.is_none());
    click(
        &dom,
        mounted_element(&dom, "aria-controls", "search-filter-panel"),
    );
    record(&mut attributes, dom.render_immediate_to_vec().edits);
    change(
        &dom,
        mounted_element(&dom, "id", "search-file-type-pdfs"),
        "true",
    );
    record(&mut attributes, dom.render_immediate_to_vec().edits);
    click(&dom, mounted_element(&dom, "id", "search-file-types-clear"));
    record(&mut attributes, dom.render_immediate_to_vec().edits);
    assert_eq!(state.peek().search_query, desired);
    assert!(!intent::selected(
        &state.peek(),
        super_platinum_core::search_query::file_types::FileCategory::Images
    ));
    click(
        &dom,
        mounted_element(&dom, "aria-controls", "search-filter-panel"),
    );
    record(&mut attributes, dom.render_immediate_to_vec().edits);
    assert!(state.peek().search_filters.is_none());
    assert_eq!(io.queries.borrow().len(), 1, "cancel issues no read");
    assert_eq!(state.peek().search_query, desired);
    click(
        &dom,
        mounted_element(&dom, "aria-controls", "search-filter-panel"),
    );
    record(&mut attributes, dom.render_immediate_to_vec().edits);
    click(
        &dom,
        mounted_element(&dom, "data-file-filter-clear", "true"),
    );
    record(&mut attributes, dom.render_immediate_to_vec().edits);
    assert_eq!(state.peek().search_query, desired);
    assert!(state.peek().search_filters.as_ref().unwrap().on.is_empty());
    let reply = io.expect("release -type:pdfs type:videos");
    click(
        &dom,
        mounted_element(&dom, "data-file-filter-apply", "true"),
    );
    pump(&mut dom, &mut attributes, || io.queries.borrow().len() == 2).await;
    reply.send(Ok(serde_json::json!({"ok":true,"files":{"matches":[],"pagination":{"page":1,"page_count":1,"total_count":0}}}))).unwrap();
    pump(&mut dom, &mut attributes, || {
        !state.peek().search_pager.loading
    })
    .await;
    assert_eq!(state.peek().search_query, "release -type:pdfs type:videos");
    assert_eq!(
        &*io.queries.borrow(),
        &[desired, "release -type:pdfs type:videos"]
    );
    assert!(state.peek().search_file_type_filters.is_none());
    // Presentation handlers remain inert across a kind change before rendering.
    {
        let mut s = state.write();
        s.core.session = None;
        s.search_filters = Some(super_platinum_core::search_query::SearchFilters::default());
        s.search_filters.as_mut().unwrap().on = "2026-10-28".into();
    }
    record(&mut attributes, dom.render_immediate_to_vec().edits);
    let stale_apply = mounted_element(&dom, "data-file-filter-apply", "true");
    let stale_clear = mounted_element(&dom, "data-file-filter-clear", "true");
    state.write().set_search_kind(SearchKind::Messages);
    click(&dom, stale_apply);
    click(&dom, stale_clear);
    assert_eq!(state.peek().search_query, "release -type:pdfs type:videos");
    assert_eq!(
        state.peek().search_filters.as_ref().unwrap().on,
        "2026-10-28"
    );
    assert_eq!(io.queries.borrow().len(), 2);
}
