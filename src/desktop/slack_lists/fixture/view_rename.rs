use super::*;
pub(crate) fn install(shell: &mut ShellState, name: &str) -> Value {
    super::column_rename::install(shell, name);
    let f = shell.slack_lists.fixture.as_mut().unwrap();
    f.document["title"] = json!("Saved view rename review");
    let kind = if name.ends_with("-narrow") {
        "board"
    } else {
        "table"
    };
    f.document["list_metadata"]["views"] = json!([
      {"id":"V_CUSTOM","name":"Project delivery","type":kind,"position":"001","columns":[{"key":"NAME","id":"C_NAME","visible":true},{"key":"done","id":"C_CHECK","visible":true},{"key":"notes","id":"C_RICH","visible":true}],"grouping":{"group_by":"done","group_by_column_id":"C_CHECK"},"filters":[],"sorts":[],"opaque":{"preserve":[1,{"future":true}]}},
      {"id":"V_OTHER","name":"Other shared view","type":"table","position":"002","columns":[{"key":"NAME","id":"C_NAME","visible":true}],"grouping":{},"filters":[],"sorts":[],"opaque":"sibling"}
    ]);
    f.document["list_metadata"]["default_view"] = json!("V_CUSTOM");
    let raw = f.document.clone();
    shell.slack_lists.view.as_mut().unwrap().layout = Default::default();
    super::refresh(shell);
    raw
}
