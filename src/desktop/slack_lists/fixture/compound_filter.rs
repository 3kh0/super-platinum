//! Static editing, or exact opt-in disconnected dynamic Filter capability.
use super::*;
pub(crate) fn install(shell: &mut ShellState, name: &str) {
    assert!(matches!(
        name,
        "slack-list-compound-filter" | "slack-list-compound-filter-narrow"
    ));
    let server = super::view_filter::seed(shell, name);
    if super::super::rename::fixture::compound_filter_check_enabled() {
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
    let p = super::super::rename::presentation(shell).unwrap();
    let f = p.filter.unwrap();
    let checkbox = f
        .choices
        .iter()
        .find(|(id, _)| !f.numbers.contains(id))
        .unwrap()
        .0
        .clone();
    let number = f.numbers[0].clone();
    use super_platinum_core::slack_lists::{NumberFilterOperator as Op, ViewFilterChoice as C};
    assert!(super::super::rename::filter::change(
        shell,
        p.serial,
        visit,
        f.choice,
        &f.desired,
        C::All(vec![
            C::Checkbox {
                column_id: checkbox,
                checked: true
            },
            C::Number {
                column_id: number,
                operator: Op::LessThan,
                value: Some("12.50".into())
            }
        ])
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn both_named_drafts_keep_complete_disconnected_population_and_sort() {
        for name in [
            "slack-list-compound-filter",
            "slack-list-compound-filter-narrow",
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
            assert_eq!(
                f.document["list_metadata"]["views"][0]["filters"],
                json!([])
            );
            assert_eq!(
                f.document["list_metadata"]["views"][0]["sorts"],
                json!([{"key":"estimate","ascending":true}])
            );
            let p = super::super::super::rename::presentation(&s)
                .unwrap()
                .filter
                .unwrap();
            assert_eq!(p.desired.conditions().len(), 2);
            assert_eq!(p.rows.len(), 2);
            assert!(super::super::super::rename::filter::close_fixture(&s));
            assert!(super::super::super::rename::presentation(&s).is_none());
        }
    }
}
