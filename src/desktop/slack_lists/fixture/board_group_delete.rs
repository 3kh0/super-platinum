//! Static offline confirmation fixture. Dynamic request receipts are separate.
use super::*;
pub(crate) fn install(shell: &mut ShellState, name: &str) {
    assert!(matches!(
        name,
        "slack-list-board-group-delete" | "slack-list-board-group-delete-narrow"
    ));
    super::board_lane_order::install(shell, "slack-list-board-lane-order");
    let fixture = shell.slack_lists.fixture.as_mut().unwrap();
    fixture.document["title"] = json!("Delete Board group");
    let loaded = super::loaded(fixture);
    let view = shell.slack_lists.view.as_mut().unwrap();
    view.title = loaded.document.title.clone();
    view.loaded = Some(loaded);
    view.revision = next_id();
    super::super::rebuild_rows(shell);
    if super::super::rename::fixture::group_delete::check_enabled() {
        super::super::rename::fixture::group_delete::seed(shell);
    }
}
