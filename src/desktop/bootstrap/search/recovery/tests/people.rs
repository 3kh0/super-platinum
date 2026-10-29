use super::super::*;
use crate::bootstrap::test_support::{harness, users};
use serde_json::json;

#[tokio::test]
async fn failed_people_lookup_recovers_one_current_batch_without_history_or_membership_side_effects()
 {
    let h = harness();
    let mut state = h.state;
    let before = users(state);
    let client = SlackClient::default();
    let query = state.peek().search_query.clone();
    let request = super::super::super::begin(&mut state.write(), "T1", &query, false).unwrap();
    super::super::super::people::load_with(
        state,
        request,
        &h.transport,
        &client,
        &h.session,
        None,
        |_| async { Err(Error::Offline("synthetic".into())) },
    )
    .await;
    assert!(state.peek().search_pager.failed);
    let history =
        serde_json::to_value(&state.peek().core.workspaces["T1"].recent_searches).unwrap();
    let visit = super::super::super::super::reconnect::Visit::begin(&mut state.write());
    let candidate = Candidate::capture(&state.peek()).unwrap();
    recover_with(state,visit,candidate,|request|async move {
        assert!(request.url.contains("users/search"));
        Ok(json!({"ok":true,"results":[{"id":"U_RECOVERED","profile":{"display_name":"Recovered person"}}]}))
    }).await;
    assert_eq!(
        super::super::super::people::rows(&state.peek())[0].id,
        "U_RECOVERED"
    );
    assert_eq!(
        (
            state.peek().search_pager.page,
            state.peek().search_pager.total
        ),
        (1, 0)
    );
    assert!(!state.peek().search_pager.failed && !state.peek().search_pager.has_more());
    assert_eq!(
        serde_json::to_value(&state.peek().core.workspaces["T1"].recent_searches).unwrap(),
        history
    );
    assert_eq!(users(state), before);
    assert_eq!(state.peek().core.composer.text, "Keep draft");
}
