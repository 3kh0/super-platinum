//! Named disconnected destination modal; existing cell fixture dispatch only.
use super::*;
pub(crate) mod check;
pub(crate) fn install(shell: &mut ShellState, name: &str) {
    assert!(matches!(
        name,
        "slack-list-board-move"
            | "slack-list-board-move-narrow"
            | "slack-list-board-move-checkbox"
            | "slack-list-board-move-checkbox-narrow"
    ));
    super::column_precision::seed(shell, name);
    let checkbox = name.contains("checkbox");
    let fixture = shell.slack_lists.fixture.as_mut().unwrap();
    fixture.document["title"] = json!("Move an existing card");
    fixture.document["list_metadata"]["views"] = json!([{"id":"V_MOVE","name":"Work lanes","type":"board","columns":[{"id":"C_NAME","key":"NAME","visible":true},{"id":"C_NUMBER","key":"estimate","visible":true}],"grouping":{"group_by":if checkbox{"done"}else{"status"}},"filters":[],"sorts":[{"key":"estimate","ascending":true}]}]);
    fixture.document["list_metadata"]["default_view"] = json!("V_MOVE");
    if check::check_enabled() {
        if checkbox {
            let row = fixture.records["records"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|r| r["id"] == check::ROW)
                .unwrap();
            row["fields"]
                .as_array_mut()
                .unwrap()
                .retain(|f| f["key"] != "done");
        } else {
            fixture.document["list_metadata"]["views"][0]["filters"] =
                json!([{"key":"status","operator":"does_not_contain","values":["OPT_DONE"]}]);
        }
    }
    let file = serde_json::from_value(fixture.document.clone()).unwrap();
    assert!(super::super::open_shell(shell, file));
    refresh(shell);
    let view = shell.slack_lists.view.as_ref().unwrap();
    let instance = view
        .layout
        .lanes
        .iter()
        .flat_map(|l| l.rows.iter())
        .find(|r| r.id == "R_LIST_000")
        .unwrap()
        .instance_id
        .clone();
    let visit = view.generation;
    let layout = view.layout.generation;
    super::super::board_move::open_shell(shell, &instance, visit, layout).unwrap();
}
