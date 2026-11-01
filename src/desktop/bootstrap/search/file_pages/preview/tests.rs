use super::*;
use crate::app_home::channel_fetches::calendar_member_test_support as renderer;
use serde_json::json;

async fn picture() -> super::super::tests::Fixture {
    let mut f = super::super::tests::setup(true).await;
    let mut state = f.h.state;
    let entry = EntrySource::capture(&state.peek()).unwrap();
    let job = plan_entry(&mut state.write(), entry, api::Sort::Newest).unwrap();
    let raw = json!({"ok":true,"items":[{"id":"F12345678","title":"First"},{"id":"F12345678","title":"Second"}],"pagination":{"page":1,"page_count":1,"total_count":2}});
    renderer::run_pumped(
        &mut f.h._dom,
        load::run_with(state, job, |_, current, observe| {
            assert!(current());
            let result = Ok(raw);
            observe(&result);
            std::future::ready(result)
        }),
    )
    .await;
    f
}
#[tokio::test]
async fn isolated_image_occurrence_control_page_reentry_event_socket_and_cooldown_fences() {
    let f = picture().await;
    let mut state = f.h.state;
    let original = Source::capture(&state.peek(), 0).unwrap();
    let job = begin(&mut state.write(), original.clone()).unwrap();
    assert!(job.current(&state.peek()));
    assert!(begin(&mut state.write(), original.clone()).is_none());
    close(state, original);
    assert!(
        job.current(&state.peek()),
        "queued pre-attempt Close cannot own its successor"
    );
    let peer = Source::capture(&state.peek(), 1).unwrap();
    let peer_job = begin(&mut state.write(), peer).unwrap();
    assert!(!job.current(&state.peek()));
    assert!(peer_job.current(&state.peek()));
    assert!(display(&state.peek(), 0).is_none());
    let close_source = Source::capture(&state.peek(), 1).unwrap();
    close(state, close_source);
    assert!(!peer_job.current(&state.peek()));
    let source = Source::capture(&state.peek(), 0).unwrap();
    {
        let mut s = state.write();
        let scene = owned_mut(&mut s, &source.frame).unwrap();
        assert!(matches!(scene.book.select(1), Plan::Cached));
    }
    assert!(
        !source.current(&state.peek()),
        "same-page cached reentry has a new Activation"
    );
    let source = Source::capture(&state.peek(), 0).unwrap();
    let rate = state
        .peek()
        .search_file_module
        .as_ref()
        .unwrap()
        .rate
        .clone();
    rate.observe(
        &state.peek(),
        &Err(Error::RateLimited {
            retry_after_secs: Some(30),
        }),
    );
    assert!(cooling(&state.peek()));
    assert!(begin(&mut state.write(), source.clone()).is_none());
    let team = source.frame.owner.team().to_owned();
    state
        .write()
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .rt_generation += 1;
    assert!(
        !source.current(&state.peek()),
        "original socket generation cannot be borrowed after reconnect"
    );
    let independent = picture().await;
    let mut state = independent.h.state;
    let source = Source::capture(&state.peek(), 0).unwrap();
    assert!(source.current(&state.peek()));
    crate::file_events::invalidate(&mut state.write(), source.frame.owner.team(), "F12345678");
    assert!(
        !source.current(&state.peek()),
        "an original-file event independently retires its page occurrence"
    );
}
