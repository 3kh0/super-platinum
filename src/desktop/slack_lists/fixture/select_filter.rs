//! Static mixed Select/Number/Checkbox authoring; no dynamic executor is armed.
use super::*;
use super_platinum_core::slack_lists::{
    NumberFilterOperator, SelectFilterOperator, ViewFilterChoice,
};
pub(crate) fn install(shell: &mut ShellState, name: &str) {
    assert!(matches!(
        name,
        "slack-list-select-filter" | "slack-list-select-filter-narrow"
    ));
    super::view_filter::seed(shell, name);
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
    let desired = ViewFilterChoice::All(vec![
        ViewFilterChoice::Checkbox {
            column_id: "C_CHECK".into(),
            checked: false,
        },
        ViewFilterChoice::Select {
            column_id: "C_SELECT".into(),
            operator: SelectFilterOperator::Contains,
            values: vec!["OPT_DONE".into(), "OPT_PROGRESS".into()],
        },
        ViewFilterChoice::Number {
            column_id: "C_NUMBER".into(),
            operator: NumberFilterOperator::LessThan,
            value: Some("12.50".into()),
        },
    ]);
    assert!(super::super::rename::filter::change(
        shell, p.serial, visit, f.choice, &f.desired, desired
    ));
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normal_and_narrow_keep_raw73_cursor_and_only_staged_mixed_conditions() {
        for name in [
            "slack-list-select-filter",
            "slack-list-select-filter-narrow",
        ] {
            let s = ShellState::fixture_variant(crate::media::MediaRegistry::default(), name);
            assert!(s.core.session.is_none() && s.core.transport.is_none());
            let f = s.slack_lists.fixture.as_ref().unwrap();
            assert_eq!(f.records["records"].as_array().unwrap().len(), 73);
            assert_eq!(
                f.records["response_metadata"]["next_cursor"],
                "precision-opaque+/="
            );
            assert_eq!(f.attempts, 0);
            assert_eq!(
                f.document["list_metadata"]["views"][0]["filters"],
                json!([])
            );
            let p = super::super::super::rename::presentation(&s).unwrap();
            assert_eq!(p.filter.unwrap().desired.conditions().len(), 3);
            assert!(super::super::super::rename::filter::close_fixture(&s));
        }
    }
}
