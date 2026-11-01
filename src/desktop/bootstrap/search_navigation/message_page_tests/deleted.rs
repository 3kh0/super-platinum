use super::*;

#[tokio::test]
async fn native_deleted_owned_hit_retires_original_open_proof_without_mutating_index_or_navigation()
{
    let h = pages::tests::loaded_harness().await;
    let mut state = h.state;
    let source = pages::RowSource::capture(&state.peek(), 0).unwrap();
    let hit = source.resolve(&state.peek()).unwrap();
    assert!(!pages::row_was_deleted(&state.peek(), 0));
    let before = {
        let s = state.peek();
        (
            s.search_results[0].text.clone(),
            pages::status(&s),
            s.core.active_channel.clone(),
            s.channel_generation,
            s.thread_generation,
            s.core.composer.text.clone(),
        )
    };
    let mut visit = OpenVisit::begin(&mut state.write(), &hit);
    visit.message_page = Some(source.proof());
    assert!(visit.source_current(&state.peek()));
    let generation = state.peek().core.workspaces["T1"].rt_generation;
    let event=super_platinum_core::slack::realtime::parse_event(&serde_json::json!({"type":"message","subtype":"message_deleted","channel":hit.channel_id,"deleted_ts":hit.ts}).to_string()).unwrap();
    crate::realtime::apply_in_view(&mut state.write().core, "T1", generation, event, false);
    assert!(pages::row_was_deleted(&state.peek(), 0));
    assert!(pages::RowSource::capture(&state.peek(), 0).is_none());
    assert!(source.resolve(&state.peek()).is_none());
    assert!(!visit.source_current(&state.peek()));
    assert!(!visit.current(&state.peek()));
    let channel = state.peek().core.workspaces["T1"].channels[&hit.channel_id].clone();
    let revision = state.peek().core.workspaces["T1"]
        .channel_lifecycle
        .revision();
    assert!(!visit.resolve(&mut state.write(), &hit.channel_id, channel, revision));
    visit.release(&mut state.write());
    assert!(state.peek().search_opening.is_none());
    let s = state.peek();
    assert_eq!(
        (
            s.search_results[0].text.clone(),
            pages::status(&s),
            s.core.active_channel.clone(),
            s.channel_generation,
            s.thread_generation,
            s.core.composer.text.clone()
        ),
        before
    );
    assert!(s.core.workspaces["T1"].messages[&hit.channel_id].was_deleted(&hit.ts));
}
