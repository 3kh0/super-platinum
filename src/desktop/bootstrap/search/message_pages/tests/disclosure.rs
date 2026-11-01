use super::*;

#[tokio::test]
async fn local_show_more_preserves_original_open_cache_and_owner_isolation_without_io() {
    let h = rich::rich_harness().await;
    let mut state = h.state;
    let source = RowSource::capture(&state.peek(), 0).unwrap();
    let before = (
        rows(&state.peek())[0].clone(),
        body(&state.peek(), 0).to_vec(),
        status(&state.peek()),
    );
    assert!(can_expand(&state.peek(), 0));
    assert!(!is_expanded(&state.peek(), 0));
    assert!(expand(&mut state.write(), &source));
    assert!(is_expanded(&state.peek(), 0));
    assert!(!can_expand(&state.peek(), 0));
    assert!(!expand(&mut state.write(), &source));
    assert!(source.resolve(&state.peek()).is_some());
    assert_eq!(
        (
            rows(&state.peek())[0].clone(),
            body(&state.peek(), 0).to_vec(),
            status(&state.peek())
        ),
        before
    );
    let other = rich::rich_harness().await;
    let mut other_state = other.state;
    assert!(!expand(&mut other_state.write(), &source));
    assert!(!is_expanded(&other_state.peek(), 0));
    let calls = Cell::new(0);
    let lookups = Cell::new(0);
    let Plan::Fetch(work) = planned(state, 2, false) else {
        panic!()
    };
    finish(
        &h,
        work,
        Ok(response(state, 2, 7, 121, false)),
        &calls,
        &lookups,
    )
    .await;
    assert!(source.resolve(&state.peek()).is_none());
    assert!(!expand(&mut state.write(), &source));
    assert!(!is_expanded(&state.peek(), 0));
    assert!(!can_expand(&state.peek(), 0));
    assert!(matches!(planned(state, 1, false), Plan::Cached));
    assert!(source.resolve(&state.peek()).is_none());
    assert!(is_expanded(&state.peek(), 0));
    assert!(!expand(&mut state.write(), &source));
    assert_eq!(calls.get(), 1);
    let row = rows(&state.peek())[0].clone();
    let generation = state.peek().core.workspaces["T1"].rt_generation;
    let event = super_platinum_core::slack::realtime::parse_event(&json!({"type":"message","subtype":"message_deleted","channel":row.channel_id,"deleted_ts":row.ts}).to_string()).unwrap();
    crate::realtime::apply_in_view(&mut state.write().core, "T1", generation, event, false);
    assert!(!is_expanded(&state.peek(), 0));
    assert!(!can_expand(&state.peek(), 0));
    assert!(body(&state.peek(), 0).is_empty());
    assert_eq!(calls.get(), 1);
}
