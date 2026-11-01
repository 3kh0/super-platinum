//! Cold metadata still permits original-owner scalar reaction query editing.
use super::*;
use crate::state::search_message_scopes::fallback;

fn cold() -> (VirtualDom, Signal<ShellState>, Rc<Io>, Attributes) {
    let (mut dom, mut state, io, mut a) = setup(0);
    {
        let mut s = state.write();
        s.search_message_scope_fixture = None;
        s.channels.clear();
        let w = s.core.workspaces.get_mut("T1").unwrap();
        w.channels.clear();
        w.users.clear();
    }
    record(&mut a, dom.render_immediate_to_vec().edits);
    assert!(!intent::wants(&state.peek()));
    (dom, state, io, a)
}

#[tokio::test]
async fn actual_cold_filters_keep_manual_identity_reactions_paging_failure_retry_and_clear() {
    use crate::bootstrap::search_message_pages as pages;
    let (mut dom, state, io, mut a) = cold();
    open(&mut dom, &mut a);
    assert!(fallback::Source::capture(&state.peek()).is_some());
    assert!(!intent::has_draft(&state.peek()));
    input(
        &dom,
        mounted_element(&dom, "aria-label", "In channel"),
        "release-room",
    );
    input(&dom, mounted_element(&dom, "aria-label", "From"), "me");
    input(
        &dom,
        mounted_element(&dom, "aria-label", "Has reaction"),
        "eyes from:me",
    );
    click(
        &dom,
        mounted_element(&dom, "data-file-filter-apply", "true"),
    );
    record(&mut a, dom.render_immediate_to_vec().edits);
    assert_eq!(
        state.peek().search_filter_error,
        Some("Use one emoji code, such as :eyes:.")
    );
    assert_eq!(state.peek().search_query, "release");
    assert!(io.reads.borrow().is_empty());
    input(
        &dom,
        mounted_element(&dom, "aria-label", "Has reaction"),
        ":eyes:",
    );
    input(
        &dom,
        mounted_element(&dom, "aria-label", "My reaction"),
        "white_check_mark",
    );
    record(&mut a, dom.render_immediate_to_vec().edits);
    assert_eq!(
        state.peek().search_query,
        "release",
        "all controls are staged"
    );
    let query = "release has::eyes: hasmy::white_check_mark: in:release-room from:me";
    let reply = io.expect(query, 1, SearchSort::Newest);
    click(
        &dom,
        mounted_element(&dom, "data-file-filter-apply", "true"),
    );
    pump(&mut dom, &mut a, || io.reads.borrow().len() == 1).await;
    assert_eq!(state.peek().search_query, query);
    assert!(!fallback::has_draft(&state.peek()));
    reply.send(Ok(fixture::response(1))).unwrap();
    pump(&mut dom, &mut a, || pages::active(&state.peek())).await;
    let reply = io.expect(query, 2, SearchSort::Newest);
    click(&dom, mounted_element(&dom, "id", "message-search-next"));
    pump(&mut dom, &mut a, || io.reads.borrow().len() == 2).await;
    reply
        .send(Err(super_platinum_core::slack::Error::Offline(
            "synthetic outage".into(),
        )))
        .unwrap();
    pump(&mut dom, &mut a, || state.peek().search_pager.failed).await;
    assert_eq!(state.peek().search_results[0].text, "Page 1 release");
    let reply = io.expect(query, 2, SearchSort::Newest);
    click(&dom, mounted_element(&dom, "id", "message-search-retry"));
    pump(&mut dom, &mut a, || io.reads.borrow().len() == 3).await;
    reply.send(Ok(fixture::response(2))).unwrap();
    pump(&mut dom, &mut a, || {
        state.peek().search_pager.page == 2 && !state.peek().search_pager.loading
    })
    .await;
    assert_eq!(state.peek().search_results[0].text, "Page 2 release");
    open(&mut dom, &mut a);
    assert!(fallback::Source::capture(&state.peek()).is_some());
    let fields = state.peek().search_filters.clone().unwrap();
    assert_eq!(
        (fields.channel.as_str(), fields.sender.as_str()),
        ("release-room", "me")
    );
    assert_eq!(fields.reactions.as_ref().unwrap().any, ":eyes:");
    assert_eq!(
        fields.reactions.as_ref().unwrap().mine,
        ":white_check_mark:"
    );
    let reply = io.expect("release", 1, SearchSort::Newest);
    click(
        &dom,
        mounted_element(&dom, "data-file-filter-clear", "true"),
    );
    assert!(
        state.peek().search_filters.is_none(),
        "generic Clear still submits immediately"
    );
    pump(&mut dom, &mut a, || io.reads.borrow().len() == 4).await;
    reply.send(Ok(fixture::response(1))).unwrap();
    pump(&mut dom, &mut a, || pages::active(&state.peek())).await;
    assert_eq!(state.peek().search_query.trim(), "release");
}

#[tokio::test]
async fn cold_draft_preserves_raw_syntax_rejects_incomplete_references_and_never_reacquires_owner()
{
    let (mut dom, mut state, io, mut a) = cold();
    let raw = "café in:<#C12345678|has::opaque: label> -has::eyes: \"hasmy::quoted:\" has::old: has::other:";
    state.write().replace_search_query(raw.into());
    record(&mut a, dom.render_immediate_to_vec().edits);
    open(&mut dom, &mut a);
    assert!(
        !state.write().apply_search_filters(),
        "generic application cannot bypass an owned draft"
    );
    let source = fallback::Source::capture(&state.peek()).unwrap();
    let submission = fallback::apply(&mut state.write(), &source).unwrap();
    assert!(submission.unchanged() && submission.current(&state.peek()));
    assert_eq!(
        state.peek().search_query,
        raw,
        "untouched multi-clause syntax is byte exact"
    );
    // The existing lexer protects modifier-prefixed references, not bare text.
    let incomplete = "release in:<#C12345678|unfinished has::eyes:";
    state.write().replace_search_query(incomplete.into());
    record(&mut a, dom.render_immediate_to_vec().edits);
    open(&mut dom, &mut a);
    input(&dom, mounted_element(&dom, "aria-label", "In channel"), "");
    input(
        &dom,
        mounted_element(&dom, "aria-label", "Has reaction"),
        "rocket",
    );
    click(
        &dom,
        mounted_element(&dom, "data-file-filter-apply", "true"),
    );
    record(&mut a, dom.render_immediate_to_vec().edits);
    assert_eq!(
        state.peek().search_filter_error,
        Some("Close the reference before adding a filter.")
    );
    assert_eq!(state.peek().search_query, incomplete);
    assert!(io.reads.borrow().is_empty());
    let old_input = mounted_element(&dom, "aria-label", "Has reaction");
    let old_apply = mounted_element(&dom, "data-file-filter-apply", "true");
    let before = state.peek().search_filters.clone();
    state.write().core.account_epoch += 1;
    input(&dom, old_input, "eyes");
    click(&dom, old_apply);
    assert_eq!(state.peek().search_filters, before);
    assert!(fallback::Source::capture(&state.peek()).is_none());
    record(&mut a, dom.render_immediate_to_vec().edits);
    assert!(
        fallback::has_draft(&state.peek()),
        "new render cannot acquire a replacement owner"
    );
    click(
        &dom,
        mounted_element(&dom, "data-file-filter-apply", "true"),
    );
    assert_eq!(state.peek().search_filters, before);
    assert_eq!(state.peek().search_query, incomplete);
    assert!(io.reads.borrow().is_empty());
}
