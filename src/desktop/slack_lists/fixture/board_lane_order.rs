//! Named controlled offline Board seed. No fabricated production read callback.
use super::*;
pub(crate) fn install(shell: &mut ShellState, name: &str) {
    assert!(matches!(
        name,
        "slack-list-board-choice"
            | "slack-list-board-choice-narrow"
            | "slack-list-board-lane-order"
            | "slack-list-board-lane-order-narrow"
            | "slack-list-board-local-order"
            | "slack-list-board-local-order-narrow"
    ));
    super::column_precision::seed(shell, name);
    let f = shell.slack_lists.fixture.as_mut().unwrap();
    f.document["title"] = json!(if name.starts_with("slack-list-board-choice") {
        "Board group options"
    } else if name.starts_with("slack-list-board-local-order") {
        "Local Board lane order"
    } else {
        "Shared Board lane order"
    });
    let mut board = f.document["list_metadata"]["views"][1].clone();
    board["id"] = json!("V_LANE");
    board["name"] = json!("Delivery lanes");
    board["grouping"] = json!({"group_by":"status","group_by_column_id":"C_SELECT","opaque":{"retained":[1,null,true]},"order":[{"select":[]},{"select":["OPT_PROGRESS"]},{"select":["OPT_DONE"]}]});
    if name.starts_with("slack-list-board-local-order") {
        board["grouping"]
            .as_object_mut()
            .unwrap()
            .remove("group_by_column_id");
    }
    f.document["list_metadata"]["views"]
        .as_array_mut()
        .unwrap()
        .insert(1, board);
    f.document["list_metadata"]["default_view"] = json!("V_LANE");
    let loaded = super::loaded(f);
    let server = loaded.document.raw.clone();
    let v = shell.slack_lists.view.as_mut().unwrap();
    v.title = loaded.document.title.clone();
    v.loaded = Some(loaded);
    v.loading = false;
    v.stale = false;
    v.unavailable = false;
    v.revision = next_id();
    v.layout = Default::default();
    super::super::rebuild_rows(shell);
    if super::super::rename::fixture::board_choice::check_enabled() {
        super::super::rename::fixture::board_choice::seed(shell, server);
    } else if super::super::rename::fixture::lane_order::check_enabled() {
        super::super::rename::fixture::lane_order::seed(shell, server);
    }
}
