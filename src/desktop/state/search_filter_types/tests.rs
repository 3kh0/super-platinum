use super::*;
use crate::bootstrap::test_support;
use dioxus::prelude::*;
use serde_json::json;
use std::rc::Rc;

#[test]
fn staged_edits_clear_cancel_and_queued_submission_bind_original_file_draft_and_preserve_generic_contract()
 {
    fn send<T: Send>() {}
    send::<ShellState>();
    let mut presentation = ShellState::fixture(crate::media::MediaRegistry::default());
    presentation.overlay = Some(Overlay::Search);
    presentation.set_search_kind(SearchKind::Files);
    presentation.search_filters = Some(SearchFilters::default());
    assert!(
        !allows(&presentation, None),
        "unowned Files fields and shared Apply/Clear/Enter must refuse"
    );
    presentation.set_search_kind(SearchKind::Messages);
    assert!(
        allows(&presentation, None),
        "Messages retain the existing generic contract"
    );
    let h = test_support::harness();
    let mut state = h.state;
    {
        let mut s = state.write();
        s.set_search_kind(SearchKind::Files);
        s.set_search_query("release -type:pdfs type:videos".into());
    }
    let toggle_source = Toggle::capture(&state.peek());
    toggle(&mut state.write(), toggle_source.clone());
    let first = Source::capture(&state.peek()).unwrap();
    set(&mut state.write(), &first, FileCategory::Images, true);
    assert_eq!(state.peek().search_query, "release -type:pdfs type:videos");
    assert!(
        !state.write().apply_search_filters(),
        "shared unowned Apply cannot bypass new File origin"
    );
    clear(&mut state.write(), &first, false);
    let cleared = Source::capture(&state.peek()).unwrap();
    assert!(!allows(&state.peek(), Some(&first)));
    set(&mut state.write(), &first, FileCategory::Pdfs, true);
    assert!(!selected(&state.peek(), FileCategory::Pdfs));
    set(&mut state.write(), &cleared, FileCategory::Documents, true);
    let work = apply(&mut state.write(), &cleared).unwrap();
    assert_eq!(
        state.peek().search_query,
        "release -type:pdfs type:videos type:documents"
    );
    assert!(work.current(&state.peek()));
    state.write().set_search_query("another query".into());
    assert!(
        !work.current(&state.peek()),
        "queued old Apply never searches successor text"
    );
    toggle(&mut state.write(), toggle_source);
    assert!(
        state.peek().search_filters.is_none(),
        "old toggle cannot open a successor visit"
    );
    let source = Toggle::capture(&state.peek());
    toggle(&mut state.write(), source);
    let active = Source::capture(&state.peek()).unwrap();
    set(&mut state.write(), &active, FileCategory::Pdfs, true);
    let close = Toggle::capture(&state.peek());
    toggle(&mut state.write(), close);
    assert_eq!(state.peek().search_query, "another query");
    assert!(state.peek().search_file_type_filters.is_none());
    assert!(!allows(&state.peek(), Some(&active)));
    let source = Toggle::capture(&state.peek());
    toggle(&mut state.write(), source);
    let active = Source::capture(&state.peek()).unwrap();
    state.write().core.account_epoch += 1;
    assert!(!allows(&state.peek(), Some(&active)));
    assert!(apply(&mut state.write(), &active).is_none());
}
#[tokio::test]
async fn unchanged_apply_retains_private_scope_and_changed_literal_explicitly_enters_original_file_request_path()
 {
    use crate::app_home::channel_fetches::calendar_member_test_support as renderer;
    use crate::bootstrap::{search_file_pages as pages, search_file_type_io::Io};
    let mut f = pages::saved::tests::loaded().await;
    let mut state = f.h.state;
    assert!(state.peek().core.cache.is_none());
    let private = Rc::new(pages::test_io::Io::default());
    let legacy = Rc::new(Io::default());
    f.h._dom.in_scope(ScopeId::ROOT, || {
        provide_context(private.clone());
        provide_context(legacy.clone());
    });
    let open = Toggle::capture(&state.peek());
    toggle(&mut state.write(), open);
    let source = Source::capture(&state.peek()).unwrap();
    let work = apply(&mut state.write(), &source).unwrap();
    assert!(pages::has_scope(&state.peek()));
    assert!(work.current(&state.peek()));
    let reply = private.expect(1);
    reply.send(Ok(pages::tests::response(1, false))).unwrap();
    renderer::run_pumped(&mut f.h._dom, crate::bootstrap::search(state)).await;
    assert_eq!(&*private.pages.borrow(), &[1]);
    assert!(legacy.queries.borrow().is_empty());
    assert!(pages::active(&state.peek()));
    let open = Toggle::capture(&state.peek());
    toggle(&mut state.write(), open);
    let source = Source::capture(&state.peek()).unwrap();
    set(&mut state.write(), &source, FileCategory::Pdfs, true);
    let work = apply(&mut state.write(), &source).unwrap();
    assert!(work.current(&state.peek()));
    assert_eq!(state.peek().search_query, "design notes type:pdfs");
    assert!(
        !pages::has_scope(&state.peek()),
        "actual trimmed query change clears private scene before legacy dispatch"
    );
    let reply = legacy.expect("design notes type:pdfs");
    reply.send(Ok(json!({"ok":true,"files":{"matches":[{"id":"FTYPE","title":"Category match"}],"pagination":{"page":1,"page_count":1,"total_count":1}}}))).unwrap();
    renderer::run_pumped(&mut f.h._dom, crate::bootstrap::search(state)).await;
    assert_eq!(&*legacy.queries.borrow(), &["design notes type:pdfs"]);
    assert_eq!(&*private.pages.borrow(), &[1]);
    assert_eq!(state.peek().search_file_results.len(), 1);
    assert!(state.peek().viewer.is_none());
    let open = Toggle::capture(&state.peek());
    toggle(&mut state.write(), open);
    let source = Source::capture(&state.peek()).unwrap();
    clear(&mut state.write(), &source, true);
    assert_eq!(
        state.peek().search_query,
        "design notes type:pdfs",
        "clear stages only"
    );
    let current = Source::capture(&state.peek()).unwrap();
    assert!(!allows(&state.peek(), Some(&source)));
    let work = apply(&mut state.write(), &current).unwrap();
    assert_eq!(state.peek().search_query, "design notes ");
    assert_eq!(state.peek().search_query.trim(), "design notes");
    assert!(work.current(&state.peek()));
}
#[test]
fn combined_refusal_keeps_original_query_and_generic_message_type_syntax_unowned() {
    let h = test_support::harness();
    let mut state = h.state;
    {
        let mut s = state.write();
        s.set_search_kind(SearchKind::Files);
        s.set_search_query("release in:\"unfinished".into());
    }
    let open = Toggle::capture(&state.peek());
    toggle(&mut state.write(), open);
    let source = Source::capture(&state.peek()).unwrap();
    set(&mut state.write(), &source, FileCategory::Images, true);
    state
        .write()
        .search_filters
        .as_mut()
        .unwrap()
        .channel
        .clear();
    assert!(apply(&mut state.write(), &source).is_none());
    assert_eq!(state.peek().search_query, "release in:\"unfinished");
    assert!(Source::capture(&state.peek()).is_some());
    state.write().set_search_kind(SearchKind::Messages);
    state
        .write()
        .set_search_query("release type:image -type:pdfs type:videos".into());
    let open = Toggle::capture(&state.peek());
    toggle(&mut state.write(), open);
    assert!(state.peek().search_file_type_filters.is_none());
    assert!(state.write().apply_search_filters());
    assert_eq!(
        state.peek().search_query,
        "release type:image -type:pdfs type:videos"
    );
}
