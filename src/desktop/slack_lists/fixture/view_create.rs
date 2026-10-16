//! Named disconnected seed. Existing raw views and partial records stay exact.
use super::*;
use super_platinum_core::slack_lists::ViewLayout;
pub(crate) fn install(shell: &mut ShellState, name: &str) -> Value {
    super::view_rename::install(shell, name);
    let f = shell.slack_lists.fixture.as_mut().unwrap();
    f.document["title"] = json!("Add shared view review");
    f.document["list_limits"]["over_view_maximum"] = json!(false);
    f.document["list_metadata"]["schema"].as_array_mut().unwrap().push(json!({"id":"C_STATUS","key":"status","name":"Status","type":"select","options":{"format":"single_select","choices":[{"value":"todo","label":"To do"},{"value":"done","label":"Done"}]},"opaque":{"keep":3}}));
    f.records["records"][0]["fields"]
        .as_array_mut()
        .unwrap()
        .push(json!({"key":"status","select":["todo"]}));
    f.records["records"][1]["fields"]
        .as_array_mut()
        .unwrap()
        .push(json!({"key":"status","select":["done"]}));
    f.records["records"][1]["fields"][1]["checkbox"] = json!(true);
    let raw = f.document.clone();
    shell.slack_lists.view.as_mut().unwrap().layout = Default::default();
    super::refresh(shell);
    raw
}
pub(crate) fn add(server: &mut Value, name: &str, layout: ViewLayout, grouping: Option<&str>) {
    server["list_metadata"]["views"].as_array_mut().unwrap().insert(1,json!({"id":"V_SERVER_NEW","name":name,"type":layout.as_str(),"position":"0015","columns":[{"key":"NAME","id":"C_NAME","visible":true,"width":240},{"key":"done","id":"C_CHECK","visible":true},{"key":"notes","id":"C_RICH","visible":true}],"filters":[],"sorts":[],"show_completed_items":true,"opaque":{"server_defaults":[1,{"future":true}]}}));
    if let Some(key) = grouping {
        server["list_metadata"]["views"][1]["grouping"] = json!({"group_by":key});
    }
}
