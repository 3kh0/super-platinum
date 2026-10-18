//! Clean named seed; local edits and writes belong to the actual controller.
use super::*;
pub(crate) fn install(shell: &mut ShellState, name: &str) {
    assert!(super::super::rename::fixture::refinement::case_for(name).is_some());
    super::board_lane_order::install(shell, "slack-list-board-local-order");
    let f = shell.slack_lists.fixture.as_mut().unwrap();
    f.document["title"] = json!("Save Board refinements");
    // Positive disconnected capacity observation, before server/read capture.
    f.document["list_limits"] = json!({"over_view_maximum":false});
    let loaded = super::loaded(f);
    let server = loaded.document.raw.clone();
    let view = shell.slack_lists.view.as_mut().unwrap();
    view.title = loaded.document.title.clone();
    view.loaded = Some(loaded);
    // This is controlled setup, never a synthetic successful async completion.
    view.loading = false;
    view.stale = false;
    view.unavailable = false;
    super::super::rebuild_rows(shell);
    if super::super::rename::fixture::refinement::check_enabled() {
        super::super::rename::fixture::refinement::seed(shell, server);
    }
}
