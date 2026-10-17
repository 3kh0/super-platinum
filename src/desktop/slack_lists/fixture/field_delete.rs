//! Named disconnected field deletion: hidden schema, full raw population and cursor.
use super::*;

pub(crate) fn install(shell: &mut ShellState, name: &str) {
    assert!(matches!(
        name,
        "slack-list-field-delete" | "slack-list-field-delete-narrow"
    ));
    let server = seed(shell, name);
    if super::super::rename::fixture::field_delete::check_enabled() {
        super::super::rename::fixture::field_delete::seed(shell, server);
    } else {
        assert!(super::super::rename::fixture::field_delete::open_static(
            shell
        ));
    }
}

pub(super) fn seed(shell: &mut ShellState, name: &str) -> Value {
    super::column_precision::seed(shell, name);
    let f = shell.slack_lists.fixture.as_mut().unwrap();
    f.document["title"] = json!("Field deletion review");
    // Authenticated fixture user U1 is an ordinary collaborator; the actual
    // owner-only assignment is explicitly off, not inferred from write access.
    f.document["user"] = json!("U_OWNER");
    f.experiments.schema_owner_only = false;
    f.document["list_metadata"]["schema"].as_array_mut().unwrap().extend([
        json!({"id":"C_HIDDEN","key":"hidden","name":"Hidden flag","type":"checkbox","opaque":{"keep":"deleted-field"}}),
        json!({"id":"C_ABSENT","key":"absent","name":"Absent notes","type":"text","options":{"format":"text"},"opaque":{"keep":"absent-field"}}),
    ]);
    f.document["list_metadata"]["views"] = json!([
        {"id":"V_FIELD_DELETE","name":"Deletion table","type":"table","columns":[
            {"id":"C_NAME","key":"NAME","visible":true},{"id":"C_NUMBER","key":"estimate","visible":true},
            {"id":"C_CHECK","key":"done","visible":true},{"id":"C_HIDDEN","key":"hidden","visible":false,"opaque":"hidden-table-entry"}],
            "grouping":{},"filters":[],"sorts":[],"opaque":{"keep":"table"}},
        {"id":"V_FIELD_DELETE_BOARD","name":"Other board","type":"board","columns":[
            {"id":"C_NAME","key":"NAME","visible":true},{"id":"C_HIDDEN","key":"hidden","visible":true},
            {"id":"C_CHECK","key":"done","visible":true}],"grouping":{"group_by":"done"},
            "filters":[{"key":"hidden","operator":"is_not_empty","values":[]}],"sorts":[{"key":"hidden","ascending":true}],"opaque":{"keep":"board"}},
        {"id":"V_FIELD_DELETE_OTHER","name":"Target absent","type":"table","columns":[
            {"id":"C_NAME","key":"NAME","visible":true},{"id":"C_CHECK","key":"done","visible":true}],
            "grouping":{"group_by":"hidden","opaque":"retained-stale-reference"},"filters":[],"sorts":[],"opaque":{"keep":"target-absent"}}
    ]);
    f.document["list_metadata"]["integrations"] =
        json!([{"column_key":"hidden","opaque":{"keep":"integration"}}]);
    f.document["list_metadata"]["default_view"] = json!("V_FIELD_DELETE");
    for (i, row) in f.records["records"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        row["fields"].as_array_mut().unwrap().extend([
            json!({"key":"hidden","checkbox":i%2==0,"opaque":{"keep":"raw-cell"}}),
            json!({"key":"absent","text":format!("Keep item {i:02}"),"opaque":{"keep":i}}),
        ]);
    }
    let server = f.document.clone();
    assert!(super::super::open_shell(
        shell,
        serde_json::from_value(server.clone()).unwrap()
    ));
    refresh(shell);
    shell
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .expanded
        .insert("R_LIST_010".into());
    super::super::rebuild_rows(shell);
    server
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn named_static_aliases_retain_hidden_cells_and_incomplete_population() {
        for name in ["slack-list-field-delete", "slack-list-field-delete-narrow"] {
            let shell = ShellState::fixture_variant(crate::media::MediaRegistry::default(), name);
            assert!(enabled(&shell));
            let f = shell.slack_lists.fixture.as_ref().unwrap();
            assert_eq!(f.attempts, 0);
            assert_eq!(f.records["records"].as_array().unwrap().len(), 73);
            assert_eq!(
                f.records["response_metadata"]["next_cursor"],
                "precision-opaque+/="
            );
            assert_eq!(f.records["records"][11]["parent_record_id"], "R_LIST_010");
            assert_eq!(
                f.records["records"][5]["fields"][2]["number"],
                json!([12.5])
            );
            assert_eq!(
                f.records["records"][20]["fields"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|c| c["key"] == "NAME")
                    .count(),
                0
            );
            let loaded = shell
                .slack_lists
                .view
                .as_ref()
                .unwrap()
                .loaded
                .as_ref()
                .unwrap();
            assert_eq!(loaded.records.raw, f.records);
            assert!(!loaded.records.complete());
            assert_eq!(loaded.document.raw, f.document);
            assert_eq!(loaded.document.views.len(), 3);
            assert_eq!(loaded.list_experiments.unwrap().schema_owner_only, false);
            let p = super::super::super::rename::presentation(&shell).unwrap();
            assert_eq!(p.stage, super::super::super::rename::Stage::Editing);
            assert_eq!(p.deletion.unwrap().column, "C_HIDDEN");
            assert!(p.can_confirm);
            assert!(
                super::super::super::rename::fixture::field_delete::close_static_for_test(&shell)
            );
        }
    }
}
