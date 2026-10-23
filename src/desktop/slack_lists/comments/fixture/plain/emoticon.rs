//! Exact closed keyboard-emoticon cases; original preferences and replies factories.
use super::*;
pub(crate) const FUTURE: &str = include_str!("emoticon-future.json");
fn admitted(protected: bool, compose: bool, flag: &str, plain: &str, case: &str) -> bool {
    protected
        && compose
        && flag == "1"
        && plain == "1"
        && matches!(
            case,
            "own-edit-plain-emoticon" | "own-edit-plain-emoticon-off"
        )
}
pub(crate) fn named() -> bool {
    admitted(
        cfg!(any(test, feature = "protected-native-fixture")),
        compose::check_enabled(),
        &std::env::var("SUPER_PLATINUM_LIST_OWN_EDIT_EMOTICON_CHECK").unwrap_or_default(),
        &std::env::var("SUPER_PLATINUM_LIST_OWN_EDIT_PLAIN_CHECK").unwrap_or_default(),
        &std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").unwrap_or_default(),
    )
}
pub(crate) fn enabled() -> bool {
    named()
        && std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref()
            == Ok("own-edit-plain-emoticon")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn both_cases_require_every_original_protected_flag() {
        for case in ["own-edit-plain-emoticon", "own-edit-plain-emoticon-off"] {
            assert!(admitted(true, true, "1", "1", case));
            assert!(!admitted(false, true, "1", "1", case));
            assert!(!admitted(true, false, "1", "1", case));
            for missing in ["", "0", "true", "2"] {
                assert!(!admitted(true, true, missing, "1", case));
                assert!(!admitted(true, true, "1", missing, case));
            }
        }
        assert!(!admitted(true, true, "1", "1", "own-edit-plain"));
    }
}
