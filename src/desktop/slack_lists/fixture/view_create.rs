//! Named disconnected seed. Existing raw views and partial records stay exact.
use super::*;
use super_platinum_core::slack_lists::ViewLayout;
pub(crate) fn install(shell: &mut ShellState, name: &str) -> Value {
    super::view_rename::install(shell, name);
    let f = shell.slack_lists.fixture.as_mut().unwrap();
    f.document["title"] = json!("Add shared view review");
    f.document["list_limits"]["over_view_maximum"] = json!(false);
    let raw = f.document.clone();
    shell.slack_lists.view.as_mut().unwrap().layout = Default::default();
    super::refresh(shell);
    raw
}
pub(crate) fn add(server: &mut Value, name: &str, layout: ViewLayout) {
    server["list_metadata"]["views"].as_array_mut().unwrap().insert(1,json!({"id":"V_SERVER_NEW","name":name,"type":layout.as_str(),"position":"0015","columns":[{"key":"NAME","id":"C_NAME","visible":true,"width":240},{"key":"done","id":"C_CHECK","visible":true},{"key":"notes","id":"C_RICH","visible":true}],"filters":[],"sorts":[],"show_completed_items":true,"opaque":{"server_defaults":[1,{"future":true}]}}));
}
