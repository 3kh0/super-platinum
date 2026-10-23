//! Original local-empty and explicit whitespace-search acceptance inputs.
use super::*;
pub(crate) const CASE_ENV: &str = "SUPER_PLATINUM_WORKFLOW_SHARE_EMPTY_CASE";
pub(crate) fn case() -> String {
    std::env::var(CASE_ENV).unwrap_or_default()
}
pub(crate) fn requested() -> bool {
    std::env::var(CHECK_ENV).as_deref() == Ok("1")
        && std::env::var(super::CASE_ENV).as_deref() == Ok("empty")
        && matches!(
            case().as_str(),
            "initial"
                | "clear"
                | "prefixes"
                | "cold-failure"
                | "cold-missing"
                | "cold-offline"
                | "warm-offline"
        )
}
pub(super) fn observation(scenario: Option<&str>, method: &str, raw: Value) -> Value {
    if scenario == Some("cold-missing") && method == "users.prefs.get" {
        json!({"ok":true,"opaque_receipt":"original-empty-query-missing-prefs"})
    } else {
        raw
    }
}
pub(super) fn reply(
    kind: wire::Kind,
    query: &str,
    marker: Option<&str>,
) -> Result<(Value, Option<Vec<String>>), Error> {
    if !requested() || !matches!(query, "review" | " ") || marker.is_some() {
        return Err(closed());
    }
    Ok((
        json!({"ok":true,"results":[],"next_marker":"",
        "opaque_receipt":{"fixture":"original-empty-query-search","query":query,"method":kind.method()}}),
        None,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_complete_preferences_is_not_an_empty_observation() {
        use super_platinum_core::block_actions::name_policy::UserPrefs;
        let original = json!({"ok":true,"prefs":{"display_real_names_override":1}});
        assert!(UserPrefs::decode(&original).is_ok());
        let missing = observation(Some("cold-missing"), "users.prefs.get", original.clone());
        assert!(UserPrefs::decode(&missing).is_err());
        for scenario in [None, Some("initial"), Some("warm-offline")] {
            assert_eq!(
                observation(scenario, "users.prefs.get", original.clone()),
                original
            );
        }
        assert_eq!(
            observation(Some("cold-missing"), "client.gantryBoot", original.clone()),
            original
        );
    }
}
