//! One ordinary Checkbox condition alongside an existing saved Number sort.
use super::*;

pub(crate) fn install(shell: &mut ShellState, name: &str) {
    assert!(matches!(
        name,
        "slack-list-view-filter" | "slack-list-view-filter-narrow"
    ));
    let server = seed(shell, name);
    if super::super::rename::fixture::filter_check_enabled() {
        super::super::rename::fixture::seed_filter(shell, server);
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
    assert!(super::super::rename::filter::open_shell(
        shell, visit, selection
    ));
}
pub(super) fn seed(shell: &mut ShellState, name: &str) -> Value {
    super::view_sort::seed(shell, name);
    let f = shell.slack_lists.fixture.as_mut().unwrap();
    f.document["title"] = json!("Shared Checkbox filter");
    let view = &mut f.document["list_metadata"]["views"][0];
    view["id"] = json!("V_FILTER");
    view["name"] = json!("Shared checklist");
    view["sorts"] = json!([{"key":"estimate","ascending":true}]);
    view["info_column_filters"] = json!([]);
    f.document["list_metadata"]["default_view"] = json!("V_FILTER");
    // Row 6 has neither a Checkbox value nor a Number value. False and missing
    // remain distinct raw input; the existing projection decides emptiness.
    f.records["records"][6]["fields"]
        .as_array_mut()
        .unwrap()
        .retain(|cell| cell["key"] != "done");
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
    fn both_aliases_keep_sorted_full_raw_population_and_open_only_static_filter() {
        for name in ["slack-list-view-filter", "slack-list-view-filter-narrow"] {
            let mut shell =
                ShellState::fixture_variant(crate::media::MediaRegistry::default(), name);
            assert!(shell.core.session.is_none() && shell.core.transport.is_none());
            let f = shell.slack_lists.fixture.as_ref().unwrap();
            assert_eq!(f.records["records"].as_array().unwrap().len(), 73);
            assert_eq!(f.records["records"][0]["fields"][1]["checkbox"], false);
            assert_eq!(f.records["records"][1]["fields"][1]["checkbox"], true);
            assert!(
                !f.records["records"][6]["fields"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|cell| cell["key"] == "done")
            );
            assert_eq!(f.records["records"][11]["parent_record_id"], "R_LIST_010");
            assert_eq!(
                f.records["response_metadata"]["next_cursor"],
                "precision-opaque+/="
            );
            assert_eq!(
                f.document["list_metadata"]["views"][0]["sorts"],
                json!([{"key":"estimate","ascending":true}])
            );
            assert_eq!(f.attempts, 0);
            let p = super::super::super::rename::presentation(&shell).unwrap();
            assert_eq!(
                p.filter.unwrap().desired,
                super_platinum_core::slack_lists::ViewFilterChoice::None
            );
            super::super::super::rename::precision::discard_fixture(&mut shell);
        }
    }
}
