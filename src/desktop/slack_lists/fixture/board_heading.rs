//! Static disconnected capture: true, explicit false, and missing checkbox cells.
use super::*;

pub(crate) fn install(shell: &mut ShellState, name: &str) {
    super::install(shell, name);
    let fixture = shell.slack_lists.fixture.as_mut().unwrap();
    fixture.document["title"] = json!("Delivery verification");
    fixture.document["list_metadata"]["schema"] = json!([
        {"id":"C_NAME","key":"NAME","name":"Item","type":"text","is_primary_column":true},
        {"id":"C_CHECK","key":"done","name":"Delivery verified","type":"checkbox"}
    ]);
    fixture.document["list_metadata"]["views"] = json!([
        {"id":"V_CHECKBOX_HEADINGS","name":"Verification board","type":"board","position":"001",
         "columns":[{"id":"C_NAME","key":"NAME","visible":true},{"id":"C_CHECK","key":"done","visible":true}],
         "filters":[],"sorts":[],"grouping":{"group_by":"done"}}
    ]);
    fixture.document["list_metadata"]["default_view"] = json!("V_CHECKBOX_HEADINGS");
    if name == "slack-list-board-heading-checked-narrow" {
        fixture.document["list_metadata"]["views"][0]["grouping"]["order"] =
            json!([{"checkbox":true},{"checkbox":false}]);
    }
    fixture.records = json!({"ok":true,"records":[
        {"id":"R_HEADING_TRUE","list_id":"F_LIST_FIXTURE","position":1,"fields":[{"key":"NAME","text":"Verified delivery"},{"key":"done","checkbox":true}]},
        {"id":"R_HEADING_FALSE","list_id":"F_LIST_FIXTURE","position":2,"fields":[{"key":"NAME","text":"Awaiting verification"},{"key":"done","checkbox":false}]},
        {"id":"R_HEADING_MISSING","list_id":"F_LIST_FIXTURE","position":3,"fields":[{"key":"NAME","text":"Verification not recorded"}]}
    ]});
    shell.slack_lists.view.as_mut().unwrap().layout = Default::default();
    super::refresh(shell);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_heading_fixture_keeps_false_and_missing_together_without_changing_labels() {
        for name in [
            "slack-list-board-heading",
            "slack-list-board-heading-narrow",
            "slack-list-board-heading-checked-narrow",
        ] {
            let shell = ShellState::fixture_variant(crate::media::MediaRegistry::default(), name);
            assert!(shell.core.session.is_none() && shell.core.transport.is_none());
            let view = crate::slack_lists::presentation(&shell).unwrap();
            assert_eq!(view.lanes.len(), 2);
            let empty = view.lanes.iter().find(|lane| lane.key.is_empty()).unwrap();
            let checked = view.lanes.iter().find(|lane| lane.key == "true").unwrap();
            assert_eq!(empty.label, "Unchecked");
            assert_eq!(checked.label, "Checked");
            assert_eq!(
                empty
                    .rows
                    .iter()
                    .map(|row| row.id.as_str())
                    .collect::<Vec<_>>(),
                ["R_HEADING_FALSE", "R_HEADING_MISSING"]
            );
            assert_eq!(checked.rows[0].id, "R_HEADING_TRUE");
            assert_eq!(
                empty.checkbox_heading.as_ref().unwrap().accessible_label,
                "Ungrouped"
            );
            assert_eq!(
                checked.checkbox_heading.as_ref().unwrap().accessible_label,
                "Delivery verified: true"
            );
        }
    }
}
