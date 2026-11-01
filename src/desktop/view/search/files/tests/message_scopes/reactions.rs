//! Actual typed reaction fields -> existing owner-bound message read path.
use super::*;

#[tokio::test]
async fn actual_reaction_fields_validate_apply_page_clear_and_retire_with_original_account() {
    use crate::bootstrap::search_message_pages as pages;
    let (mut dom, mut state, io, mut a) = setup(2);
    open(&mut dom, &mut a);
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
    assert_eq!(state.peek().search_query, "release");
    assert_eq!(
        state.peek().search_filter_error,
        Some("Use one emoji code, such as :eyes:.")
    );
    assert!(intent::has_draft(&state.peek()) && io.reads.borrow().is_empty());
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
        "typing remains staged"
    );
    assert!(io.reads.borrow().is_empty());
    let query = "release has::eyes: hasmy::white_check_mark:";
    let reply = io.expect(query, 1, SearchSort::Newest);
    click(
        &dom,
        mounted_element(&dom, "data-file-filter-apply", "true"),
    );
    pump(&mut dom, &mut a, || io.reads.borrow().len() == 1).await;
    assert_eq!(state.peek().search_query, query);
    reply.send(Ok(fixture::response(1))).unwrap();
    pump(&mut dom, &mut a, || pages::active(&state.peek())).await;
    let reply = io.expect(query, 2, SearchSort::Newest);
    click(&dom, mounted_element(&dom, "id", "message-search-next"));
    pump(&mut dom, &mut a, || io.reads.borrow().len() == 2).await;
    reply.send(Ok(fixture::response(2))).unwrap();
    pump(&mut dom, &mut a, || {
        state.peek().search_pager.page == 2 && !state.peek().search_pager.loading
    })
    .await;
    assert_eq!(state.peek().search_results[0].text, "Page 2 release");
    open(&mut dom, &mut a);
    let fields = state.peek().search_filters.clone().unwrap();
    assert_eq!(fields.reactions.as_ref().unwrap().any, ":eyes:");
    assert_eq!(
        fields.reactions.as_ref().unwrap().mine,
        ":white_check_mark:"
    );
    click(
        &dom,
        mounted_element(&dom, "data-file-filter-clear", "true"),
    );
    record(&mut a, dom.render_immediate_to_vec().edits);
    assert_eq!(
        state.peek().search_query,
        query,
        "Clear is staged until Apply in the typed panel"
    );
    assert!(
        state
            .peek()
            .search_filters
            .as_ref()
            .unwrap()
            .reactions
            .is_none()
    );
    assert_eq!(io.reads.borrow().len(), 2);
    let reply = io.expect("release", 1, SearchSort::Newest);
    click(
        &dom,
        mounted_element(&dom, "data-file-filter-apply", "true"),
    );
    pump(&mut dom, &mut a, || io.reads.borrow().len() == 3).await;
    reply.send(Ok(fixture::response(1))).unwrap();
    pump(&mut dom, &mut a, || pages::active(&state.peek())).await;
    assert_eq!(state.peek().search_query.trim(), "release");
    open(&mut dom, &mut a);
    let old_input = mounted_element(&dom, "aria-label", "Has reaction");
    let old_apply = mounted_element(&dom, "data-file-filter-apply", "true");
    let before = state.peek().search_filters.clone();
    state.write().core.account_epoch += 1;
    input(&dom, old_input, "rocket");
    click(&dom, old_apply);
    assert_eq!(
        state.peek().search_filters,
        before,
        "old rendered account cannot stage or submit a reaction"
    );
    assert_eq!(state.peek().search_query.trim(), "release");
    assert_eq!(io.reads.borrow().len(), 3);
    assert!(state.peek().viewer.is_none());
}
