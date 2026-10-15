use super::*;
pub(crate) fn install(shell: &mut ShellState, name: &str) -> (Value, Value) {
    super::view_rename::install(shell, name);
    let f = shell.slack_lists.fixture.as_mut().unwrap();
    f.experiments.sub_tasks = true;
    f.document["title"] = json!("Direct subitem review");
    f.document["old_document_unknown"] = json!({"retained_in_receipt":true});
    f.document["list_limits"]["over_row_maximum"] = json!(false);
    f.records["records"][0]["old_parent_unknown"] = json!({"keep_evidence":true});
    for (id, parent, title, pos) in [
        (
            "R_CHILD_A",
            "R_HYDRATE_PARENT",
            "Earlier child A",
            "0000000002",
        ),
        (
            "R_CHILD_B",
            "R_HYDRATE_PARENT",
            "Earlier child B",
            "0000000003",
        ),
        (
            "R_GRANDCHILD",
            "R_CHILD_A",
            "Retained deeper child",
            "0000000004",
        ),
    ] {
        f.records["records"].as_array_mut().unwrap().push(json!({"id":id,"list_id":"F_LIST_FIXTURE","parent_record_id":parent,"position":pos,"date_created":3,"created_by":"U1","fields":[{"key":"NAME","text":title},{"key":"done","checkbox":false}],"opaque":{"retain":id}}));
    }
    f.records["opaque_population"] = json!({"keep":[1,2,3]});
    let result = (f.document.clone(), f.records.clone());
    super::refresh(shell);
    result
}

pub(crate) fn reopen(shell: &mut ShellState) -> Result<u64, String> {
    let doc = shell
        .slack_lists
        .fixture
        .as_ref()
        .ok_or("No fixture")?
        .document
        .clone();
    if !super::super::open_shell(
        shell,
        serde_json::from_value(doc).map_err(|_| "fixture file")?,
    ) {
        return Err("Could not reopen fixture".into());
    }
    super::refresh(shell);
    super::super::generation(shell).ok_or("No new visit".into())
}
