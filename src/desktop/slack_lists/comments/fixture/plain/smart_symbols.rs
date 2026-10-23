//! Exact closed smart-symbol assignments and preferences; no live capability.
use super::*;
pub(crate) const FUTURE: &str = include_str!("smart_symbols-future.json");
fn admitted(protected: bool, compose: bool, plain: &str, flag: &str, case: &str) -> bool {
    protected
        && compose
        && plain == "1"
        && flag == "1"
        && matches!(
            case,
            "own-edit-plain-smart-symbols"
                | "own-edit-plain-smart-symbols-off"
                | "own-edit-plain-smart-symbols-disabled"
        )
}
pub(crate) fn named() -> bool {
    admitted(
        cfg!(any(test, feature = "protected-native-fixture")),
        compose::check_enabled(),
        &std::env::var("SUPER_PLATINUM_LIST_OWN_EDIT_PLAIN_CHECK").unwrap_or_default(),
        &std::env::var("SUPER_PLATINUM_LIST_OWN_EDIT_SMART_SYMBOLS_CHECK").unwrap_or_default(),
        &std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").unwrap_or_default(),
    )
}
pub(crate) fn disabled() -> bool {
    named()
        && std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref()
            == Ok("own-edit-plain-smart-symbols-disabled")
}
pub(crate) fn enabled() -> bool {
    named()
        && std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref()
            == Ok("own-edit-plain-smart-symbols")
}
pub(crate) fn apply_preferences(raw: &mut Value) {
    if named() {
        raw["prefs"]["disable_smart_symbols"] = json!(disabled());
    }
}
pub(crate) fn apply_assignments(raw: &mut Value) {
    if named() {
        raw["assignments"]["composer_smart_symbols"] =
            json!({"group":if enabled()||disabled(){"on"}else{"off"}});
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn each_smart_case_requires_original_protected_compose_plain_and_exact_flag() {
        for case in [
            "own-edit-plain-smart-symbols",
            "own-edit-plain-smart-symbols-off",
            "own-edit-plain-smart-symbols-disabled",
        ] {
            assert!(admitted(true, true, "1", "1", case));
            assert!(!admitted(false, true, "1", "1", case));
            assert!(!admitted(true, false, "1", "1", case));
            for wrong in ["", "0", "true", "2"] {
                assert!(!admitted(true, true, wrong, "1", case));
                assert!(!admitted(true, true, "1", wrong, case));
            }
        }
        assert!(!admitted(true, true, "1", "1", "own-edit-plain"));
    }
}
