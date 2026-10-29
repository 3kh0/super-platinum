//! Actual opener admission/metadata/navigation helpers without polling Slack IO.
use super::*;
use crate::bootstrap::search_message_pages as pages;

#[tokio::test]
async fn original_page_proof_survives_only_its_own_open_serial_and_guards_metadata_and_navigation()
{
    let h = pages::tests::loaded_harness().await;
    let mut state = h.state;
    let source = pages::RowSource::capture(&state.peek(), 0).unwrap();
    let hit = source.resolve(&state.peek()).unwrap();
    let old_generation = state.peek().search_open_generation;
    let mut visit = OpenVisit::begin(&mut state.write(), &hit);
    visit.message_page = Some(source.proof());
    assert_eq!(
        state.peek().search_open_generation,
        old_generation.wrapping_add(1)
    );
    assert!(visit.source_current(&state.peek()));
    let channel = state.peek().core.workspaces["T1"].channels[&hit.channel_id].clone();
    let revision = state.peek().core.workspaces["T1"]
        .channel_lifecycle
        .revision();
    assert!(visit.resolve(&mut state.write(), &hit.channel_id, channel, revision));
    visit.release(&mut state.write());
    assert!(state.write().open_search_result(&hit.channel_id, &hit.ts));
    assert!(
        visit.current(&state.peek()),
        "own navigation may close Search without borrowing a new dialog"
    );
    assert_eq!(state.peek().main_view, MainView::Home);
    assert_eq!(
        state.peek().core.active_channel.as_deref(),
        Some(hit.channel_id.as_str())
    );
    state.write().open_search(false);
    assert!(
        !visit.current(&state.peek()),
        "even reopening the same cache retires the original continuation"
    );

    let h = pages::tests::loaded_harness().await;
    let mut state = h.state;
    let source = pages::RowSource::capture(&state.peek(), 0).unwrap();
    let hit = source.resolve(&state.peek()).unwrap();
    let mut visit = OpenVisit::begin(&mut state.write(), &hit);
    visit.message_page = Some(source.proof());
    let channel = state.peek().core.workspaces["T1"].channels[&hit.channel_id].clone();
    let revision = state.peek().core.workspaces["T1"]
        .channel_lifecycle
        .revision();
    state
        .write()
        .core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut("T1")
        .unwrap()
        .token = "successor synthetic token".into();
    assert!(!visit.source_current(&state.peek()));
    assert!(!visit.current(&state.peek()));
    assert!(!visit.resolve(&mut state.write(), &hit.channel_id, channel, revision));
}
