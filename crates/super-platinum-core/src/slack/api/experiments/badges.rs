//! Complete assignment read: captured fetchExperimentsData S1028162.
use super::*;
pub use crate::state::BadgeExperiments;

pub fn badge_experiments(c: &SlackClient, w: &WorkspaceSession) -> PreparedRequest {
    list_experiments(c, w)
}

pub fn parse_badge_experiments(response: &Value) -> Result<BadgeExperiments, Error> {
    if response.get("ok") != Some(&Value::Bool(true)) {
        return Err(Error::Api(
            "Badge experiment lookup was not successful.".into(),
        ));
    }
    let assignments = response
        .get("assignments")
        .and_then(Value::as_object)
        .ok_or_else(|| Error::Api("Badge experiment assignments are unavailable.".into()))?;
    let mut groups = std::collections::BTreeMap::new();
    for name in crate::state::BADGE_EXPERIMENT_NAMES {
        if let Some(assignment) = assignments.get(*name) {
            let group = assignment
                .get("group")
                .and_then(Value::as_str)
                .ok_or_else(|| Error::Api("A badge experiment assignment is malformed.".into()))?;
            groups.insert((*name).to_owned(), group.to_owned());
        }
    }
    Ok(BadgeExperiments::from_groups(groups))
}

pub async fn fetch_badge_experiments(
    t: &Transport,
    c: &SlackClient,
    w: &WorkspaceSession,
) -> Result<BadgeExperiments, Error> {
    parse_badge_experiments(&t.execute(badge_experiments(c, w)).await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn complete_missing_groups_are_off_but_unknown_group_strings_survive() {
        let empty = parse_badge_experiments(&json!({"ok":true,"assignments":{}})).unwrap();
        assert!(!empty.inbox_enabled());
        let policy = parse_badge_experiments(&json!({"ok":true,"assignments":{
            "activity_inbox":{"group":"on"}, "lists_monster_step":{"group":"future"},
            "irrelevant":null
        }}))
        .unwrap();
        assert!(policy.inbox_enabled());
        assert_eq!(policy.group("lists_monster_step"), Some("future"));
        assert_eq!(policy.group("approvals_in_todos"), None);
    }

    #[test]
    fn unavailable_or_malformed_complete_response_is_not_an_off_snapshot() {
        for value in [
            json!({"ok":true}),
            json!({"ok":true,"assignments":null}),
            json!({"ok":false,"assignments":{}}),
            json!({"ok":true,"assignments":{"activity_inbox":{"group":null}}}),
            json!({"ok":true,"assignments":{"activity_inbox":{"group":true}}}),
        ] {
            assert!(parse_badge_experiments(&value).is_err());
        }
    }
}
