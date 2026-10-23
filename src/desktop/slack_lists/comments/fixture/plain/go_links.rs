//! Exact positively observed go-link assignment in the closed plain fixture.
use super::*;
pub(crate) const FUTURE: &str = include_str!("go_links-future.json");
pub(crate) const TEXT: &str = "Visit go/team";
fn admitted(protected: bool, compose: bool, plain: &str, flag: &str, case: &str) -> bool {
    protected && compose && plain == "1" && flag == "1" && case == "own-edit-plain-go-links"
}
pub(crate) fn named() -> bool {
    admitted(
        cfg!(any(test, feature = "protected-native-fixture")),
        compose::check_enabled(),
        &std::env::var("SUPER_PLATINUM_LIST_OWN_EDIT_PLAIN_CHECK").unwrap_or_default(),
        &std::env::var("SUPER_PLATINUM_LIST_OWN_EDIT_GO_LINKS_CHECK").unwrap_or_default(),
        &std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").unwrap_or_default(),
    )
}
pub(crate) fn apply_assignments(raw: &mut Value) {
    if named() {
        raw["assignments"]["detect_go_links"] = json!({"group":"on"});
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn go_links_requires_original_protected_compose_plain_and_exact_flag() {
        assert!(admitted(true, true, "1", "1", "own-edit-plain-go-links"));
        assert!(!admitted(false, true, "1", "1", "own-edit-plain-go-links"));
        assert!(!admitted(true, false, "1", "1", "own-edit-plain-go-links"));
        for wrong in ["", "0", "true", "2"] {
            assert!(!admitted(true, true, wrong, "1", "own-edit-plain-go-links"));
            assert!(!admitted(true, true, "1", wrong, "own-edit-plain-go-links"));
        }
        assert!(!admitted(true, true, "1", "1", "own-edit-plain"));
    }
}
