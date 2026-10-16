//! Named disconnected static Number draft; no write or dynamic checker implied.
use super::*;
use super_platinum_core::slack_lists::{NumberFilterOperator, ViewFilterChoice};
pub(crate) fn install(shell: &mut ShellState, name: &str) {
    assert!(matches!(
        name,
        "slack-list-number-filter" | "slack-list-number-filter-narrow"
    ));
    let server = super::view_filter::seed(shell, name);
    if super::super::rename::fixture::number_filter_check_enabled() {
        super::super::rename::fixture::seed_filter(shell, server);
        return;
    }
    let visit = generation(shell).unwrap();
    let selection = super::super::rename::selection(shell).unwrap();
    assert!(super::super::rename::filter::open_shell(
        shell, visit, selection
    ));
    let p = super::super::rename::presentation(shell).unwrap();
    let f = p.filter.unwrap();
    assert!(super::super::rename::filter::change(
        shell,
        p.serial,
        visit,
        f.choice,
        &f.desired,
        ViewFilterChoice::Number {
            column_id: "C_NUMBER".into(),
            operator: NumberFilterOperator::Equal,
            value: Some("12.50".into())
        }
    ));
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn both_static_aliases_retain_sort_population_and_disconnected_draft() {
        for name in [
            "slack-list-number-filter",
            "slack-list-number-filter-narrow",
        ] {
            let mut s = ShellState::fixture_variant(crate::media::MediaRegistry::default(), name);
            assert!(s.core.session.is_none() && s.core.transport.is_none());
            let f = s.slack_lists.fixture.as_ref().unwrap();
            assert_eq!(f.attempts, 0);
            assert_eq!(f.records["records"].as_array().unwrap().len(), 73);
            assert_eq!(f.records["records"][11]["parent_record_id"], "R_LIST_010");
            assert_eq!(
                f.records["response_metadata"]["next_cursor"],
                "precision-opaque+/="
            );
            assert_eq!(
                f.document["list_metadata"]["views"][0]["filters"],
                json!([])
            );
            assert_eq!(
                f.document["list_metadata"]["views"][0]["sorts"],
                json!([{"key":"estimate","ascending":true}])
            );
            let p = super::super::super::rename::presentation(&s).unwrap();
            assert_eq!(
                p.filter.unwrap().desired,
                ViewFilterChoice::Number {
                    column_id: "C_NUMBER".into(),
                    operator: NumberFilterOperator::Equal,
                    value: Some("12.50".into())
                }
            );
            super::super::super::rename::precision::discard_fixture(&mut s);
        }
    }
}
