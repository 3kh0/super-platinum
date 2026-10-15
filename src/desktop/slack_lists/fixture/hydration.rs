//! Exact named disconnected seed. No real account/session/transport is admitted.
use super::*;

pub(crate) fn install(shell: &mut ShellState, name: &str) {
    assert!(shell.core.transport.is_none() && shell.core.session.is_none());
    super::install(shell, name);
    let fixture = shell.slack_lists.fixture.as_mut().unwrap();
    fixture.document["title"] = json!("Incomplete project checklist");
    fixture.document["list_limits"] = json!({"over_row_maximum":false});
    fixture.document["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
        .retain(|column| {
            ["C_NAME", "C_CHECK", "C_RICH"]
                .iter()
                .any(|id| column["id"] == *id)
        });
    fixture.records = json!({"ok":true,"list_id":"F_LIST_FIXTURE","records":[
        {"id":"R_HYDRATE_PARENT","list_id":"F_LIST_FIXTURE","position":"0000000001",
            "date_created":1,"created_by":"U1","fields":[{"key":"NAME","text":"Project parent"},
                {"key":"done","checkbox":false},{"key":"notes","text":"Known parent; children are not loaded yet."}]},
        {"id":"R_HYDRATE_OTHER","list_id":"F_LIST_FIXTURE","position":"9999999999",
            "date_created":2,"created_by":"U1","fields":[{"key":"NAME","text":"Unrelated retained item"},
                {"key":"done","checkbox":false}]}
    ],"response_metadata":{"next_cursor":"fixture-opaque-unloaded+/=","keep":"exact"}});
    refresh(shell);
}
