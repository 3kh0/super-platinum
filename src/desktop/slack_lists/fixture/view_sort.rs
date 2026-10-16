//! Disconnected saved Table with distinct values, ties, missing values and children.
use super::*;

pub(crate) fn install(shell: &mut ShellState, name: &str) {
    assert!(matches!(
        name,
        "slack-list-view-sort" | "slack-list-view-sort-narrow"
    ));
    let server = seed(shell, name);
    if super::super::rename::fixture::sort_check_enabled() {
        super::super::rename::fixture::seed_sort(shell, server);
        return;
    }
    let visit = generation(shell).unwrap();
    let selection = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .layout
        .selection_generation;
    assert!(super::super::rename::sort::open_shell(
        shell, visit, selection
    ));
}

pub(super) fn seed(shell: &mut ShellState, name: &str) -> Value {
    super::column_precision::seed(shell, name);
    let f = shell.slack_lists.fixture.as_mut().unwrap();
    f.document["title"] = json!("Shared Number order");
    f.document["list_metadata"]["views"][0]["id"] = json!("V_SORT");
    f.document["list_metadata"]["views"][0]["name"] = json!("Shared estimates");
    f.document["list_metadata"]["default_view"] = json!("V_SORT");
    f.document["list_metadata"]["schema"][2]["options"] = json!({"precision":2});
    for (i, n) in [12.5, 2.0, -3.0, 0.0, 2.0, 12.5].into_iter().enumerate() {
        f.records["records"][i]["fields"][2]["number"] = json!([n]);
    }
    f.records["records"][6]["fields"][2]["number"] = json!([]);
    let server = f.document.clone();
    assert!(super::super::open_shell(
        shell,
        serde_json::from_value(server.clone()).unwrap()
    ));
    refresh(shell);
    server
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn both_static_aliases_preserve_raw_population_hierarchy_and_cursor() {
        for name in ["slack-list-view-sort", "slack-list-view-sort-narrow"] {
            let mut shell =
                ShellState::fixture_variant(crate::media::MediaRegistry::default(), name);
            assert!(shell.core.session.is_none() && shell.core.transport.is_none());
            let f = shell.slack_lists.fixture.as_ref().unwrap();
            assert_eq!(f.records["records"].as_array().unwrap().len(), 73);
            assert_eq!(
                f.records["records"][2]["fields"][2]["number"],
                json!([-3.0])
            );
            assert_eq!(f.records["records"][6]["fields"][2]["number"], json!([]));
            assert_eq!(f.records["records"][11]["parent_record_id"], "R_LIST_010");
            assert_eq!(
                f.records["response_metadata"]["next_cursor"],
                "precision-opaque+/="
            );
            assert_eq!(f.attempts, 0);
            let p = super::super::super::rename::presentation(&shell).unwrap();
            assert_eq!(
                p.sort.unwrap().desired,
                super_platinum_core::slack_lists::ViewSortChoice::None
            );
            super::super::super::rename::precision::discard_fixture(&mut shell);
        }
    }
}
