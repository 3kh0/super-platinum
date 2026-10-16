//! Static review or exact owned dynamic Group by fixture.
use super::*;
pub(crate) fn install(shell: &mut ShellState, name: &str) {
    assert!(matches!(
        name,
        "slack-list-view-grouping"
            | "slack-list-view-grouping-narrow"
            | "slack-list-board-grouping"
            | "slack-list-board-grouping-narrow"
    ));
    seed(shell, name);
    if super::super::rename::fixture::grouping::check_enabled() {
        dynamic(shell);
        let server = shell.slack_lists.fixture.as_ref().unwrap().document.clone();
        super::super::rename::fixture::grouping::seed(shell, server);
        return;
    }
    let visit = generation(shell).unwrap();
    let selection = super::super::rename::selection(shell).unwrap();
    assert!(super::super::rename::grouping::open_shell(
        shell, visit, selection
    ));
}
pub(super) fn seed(shell: &mut ShellState, name: &str) {
    super::column_precision::seed(shell, name);
    let f = shell.slack_lists.fixture.as_mut().unwrap();
    f.document["title"] = json!("Shared grouping");
    let kind = if name.starts_with("slack-list-board-grouping") {
        "board"
    } else {
        "table"
    };
    for (i, row) in f.records["records"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
        .take(12)
    {
        row["fields"][5]["select"] = json!([if i % 2 == 0 {
            "OPT_PROGRESS"
        } else {
            "OPT_DONE"
        }]);
    }
    f.document["list_metadata"]["views"] = json!([
        {"id":"V_GROUP","name":"Shared groups","type":kind,"columns":[{"id":"C_NAME","key":"NAME","visible":true},{"id":"C_CHECK","key":"done","visible":true},{"id":"C_NUMBER","key":"estimate","visible":true},{"id":"C_SELECT","key":"status","visible":true}],"grouping":{"group_by":"done"},"filters":[],"sorts":[],"opaque":{"keep":true}},
        {"id":"V_OTHER","name":"Other view","type":"table","columns":[{"id":"C_NAME","key":"NAME","visible":true}],"grouping":{},"filters":[],"sorts":[]}
    ]);
    f.document["list_metadata"]["default_view"] = json!("V_GROUP");
    let file = serde_json::from_value(f.document.clone()).unwrap();
    assert!(super::super::open_shell(shell, file));
    refresh(shell);
}
/// Bounded dynamic setup only; all later request/readback checks preserve this raw baseline.
fn dynamic(shell: &mut ShellState) {
    let f = shell.slack_lists.fixture.as_mut().unwrap();
    f.document["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"C_OTHER","key":"other","name":"Other done","type":"checkbox"}));
    f.document["list_metadata"]["schema"][2]["options"] = json!({"precision":2});
    let view = &mut f.document["list_metadata"]["views"][0];
    view["filters"] = json!([{"key":"estimate","operator":"greater_than","values":["-4"]}]);
    view["info_column_filters"] = json!([]);
    view["sorts"] = json!([{"key":"estimate","ascending":true}]);
    for (i, row) in f.records["records"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        let done = row["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|field| field["key"] == "done")
            .expect("Group by fixture must retain its Done field")["checkbox"]
            .clone();
        row["fields"]
            .as_array_mut()
            .unwrap()
            .push(json!({"key":"other","checkbox":done}));
        if i < 6 {
            row["fields"][2]["number"] = json!([([12.5, 2.0, -3.0, 0.0, 2.0, 12.5][i])]);
        }
        if i == 6 {
            row["fields"][2]["number"] = json!([]);
        }
        if i < 9 {
            row["fields"][5]["select"] = json!([if i % 4 < 2 {
                "OPT_PROGRESS"
            } else {
                "OPT_DONE"
            }]);
        }
    }
    let file = serde_json::from_value(f.document.clone()).unwrap();
    assert!(super::super::open_shell(shell, file));
    refresh(shell);
    shell
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .expanded
        .insert("R_LIST_010".into());
    super::super::rebuild_rows(shell);
}
#[cfg(test)]
pub(crate) fn dynamic_for_test(shell: &mut ShellState) {
    assert!(super::super::rename::fixture::grouping::close_static_for_test(shell));
    dynamic(shell);
    let visit = generation(shell).unwrap();
    let selection = super::super::rename::selection(shell).unwrap();
    assert!(super::super::rename::grouping::open_shell(
        shell, visit, selection
    ));
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_named_static_aliases_keep_raw_population_and_show_owned_grouping_editor() {
        for name in [
            "slack-list-view-grouping",
            "slack-list-view-grouping-narrow",
            "slack-list-board-grouping",
            "slack-list-board-grouping-narrow",
        ] {
            let s = ShellState::fixture_variant(crate::media::MediaRegistry::default(), name);
            assert!(s.core.session.is_none() && s.core.transport.is_none());
            let f = s.slack_lists.fixture.as_ref().unwrap();
            assert_eq!(f.attempts, 0);
            assert_eq!(f.records["records"].as_array().unwrap().len(), 73);
            assert_eq!(
                f.records["response_metadata"]["next_cursor"],
                "precision-opaque+/="
            );
            assert_eq!(f.records["records"][11]["parent_record_id"], "R_LIST_010");
            let p = super::super::super::rename::presentation(&s).unwrap();
            assert_eq!(p.stage, super::super::super::rename::Stage::Editing);
            let p = p.view_grouping.unwrap();
            assert_eq!(p.choices.len(), 2);
            assert_eq!(
                p.desired,
                super_platinum_core::slack_lists::ViewGroupingChoice::Column {
                    column_id: "C_CHECK".into()
                }
            );
            assert!(s.slack_lists.view.as_ref().unwrap().layout.notice.is_none());
            assert!(super::super::super::rename::fixture::grouping::close_static_for_test(&s));
        }
    }

    #[test]
    fn dynamic_mirrors_done_when_primary_is_absent_or_fields_are_reordered() {
        for name in ["slack-list-view-grouping", "slack-list-board-grouping"] {
            let mut s = ShellState::fixture_variant(crate::media::MediaRegistry::default(), name);
            let (mut absent_primary, mut reordered) = {
                let f = s.slack_lists.fixture.as_mut().unwrap();
                let records = f.records["records"].as_array_mut().unwrap();
                assert!(
                    records[20]["fields"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .all(|f| f["key"] != "NAME")
                );
                assert_eq!(records[20]["fields"][1]["key"], "estimate");
                records[21]["fields"].as_array_mut().unwrap().rotate_left(4);
                assert_ne!(records[21]["fields"][1]["key"], "done");
                (records[20].clone(), records[21].clone())
            };
            absent_primary["fields"]
                .as_array_mut()
                .unwrap()
                .push(json!({"key":"other","checkbox":false}));
            reordered["fields"]
                .as_array_mut()
                .unwrap()
                .push(json!({"key":"other","checkbox":true}));
            dynamic_for_test(&mut s);
            let f = s.slack_lists.fixture.as_ref().unwrap();
            let records = f.records["records"].as_array().unwrap();
            assert_eq!(records.len(), 73);
            assert_eq!(records[20], absent_primary);
            assert_eq!(records[21], reordered);
            for row in records {
                let fields = row["fields"].as_array().unwrap();
                let done = fields.iter().find(|f| f["key"] == "done").unwrap();
                let other = fields.iter().find(|f| f["key"] == "other").unwrap();
                assert!(done["checkbox"].is_boolean());
                assert_eq!(other["checkbox"], done["checkbox"]);
            }
            assert_eq!(
                s.slack_lists
                    .view
                    .as_ref()
                    .unwrap()
                    .loaded
                    .as_ref()
                    .unwrap()
                    .records
                    .raw,
                f.records
            );
            assert!(super::super::super::rename::fixture::grouping::close_static_for_test(&s));
        }
    }
}
