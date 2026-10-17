//! Authenticated experiment assignments, captured fetchExperimentsData S:1028162.
//! Keep only caller policy flags; no exposure IDs or unrelated assignments.
mod badges;
use super::*;
pub use badges::*;
use serde_json::Value;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ListExperiments {
    pub schema_owner_only: bool,
    pub sub_tasks: bool,
    pub grid_todos: bool,
    pub todos: bool,
    pub todos_rollback: bool,
    pub default_view_dual_write: bool,
}

pub fn list_experiments(c: &SlackClient, w: &WorkspaceSession) -> PreparedRequest {
    // Platform fields match the captured desktop caller. Do not send an invented
    // Slack web build timestamp on behalf of this native client.
    c.rest_form(
        w,
        "experiments.getByUser",
        vec![
            (
                "ssb_platform",
                std::env::consts::OS.replace("macos", "darwin"),
            ),
            ("ssb_release_channel", "prod".into()),
        ],
    )
}

pub fn parse_list_experiments(response: &Value) -> Result<ListExperiments, Error> {
    if response.get("ok") != Some(&Value::Bool(true))
        || response.get("error").is_some_and(|v| !v.is_null())
    {
        return Err(Error::Api(
            "List experiment lookup was not successful.".into(),
        ));
    }
    let assignments = response
        .get("assignments")
        .and_then(Value::as_object)
        .ok_or_else(|| Error::Api("List experiment assignments are unavailable.".into()))?;
    let enabled = |name| -> Result<bool, Error> {
        match assignments.get(name) {
            None => Ok(false),
            Some(value) => value
                .get("group")
                .and_then(Value::as_str)
                .map(|group| group == "on")
                .ok_or_else(|| Error::Api("A List experiment assignment is malformed.".into())),
        }
    };
    Ok(ListExperiments {
        schema_owner_only: enabled("lists_schema_owner_only")?,
        sub_tasks: enabled("lists_sub_tasks")?,
        grid_todos: enabled("lists_grid_todos")?,
        todos: enabled("todos")?,
        todos_rollback: enabled("lists_todos_rollback")?,
        default_view_dual_write: enabled("lists_default_view_dual_write")?,
    })
}

pub async fn fetch_list_experiments(
    t: &Transport,
    c: &SlackClient,
    w: &WorkspaceSession,
) -> Result<ListExperiments, Error> {
    parse_list_experiments(&t.execute(list_experiments(c, w)).await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn schema_owner_only_requires_exact_on_assignment_and_rejects_malformed_evidence() {
        for group in ["on", "off", "control", "ON", ""] {
            let flags = parse_list_experiments(
                &json!({"ok":true,"assignments":{"lists_schema_owner_only":{"group":group}}}),
            )
            .unwrap();
            assert_eq!(flags.schema_owner_only, group == "on");
        }
        assert!(
            !parse_list_experiments(&json!({"ok":true,"assignments":{}}))
                .unwrap()
                .schema_owner_only
        );
        for value in [
            Value::Null,
            json!({}),
            json!({"group":true}),
            json!({"group":null}),
            json!({"group":2}),
        ] {
            assert!(
                parse_list_experiments(
                    &json!({"ok":true,"assignments":{"lists_schema_owner_only":value}})
                )
                .is_err()
            );
        }
        for raw in [
            json!({"ok":true}),
            json!({"ok":false,"assignments":{}}),
            json!({"ok":true,"error":"denied","assignments":{}}),
        ] {
            assert!(parse_list_experiments(&raw).is_err());
        }
    }
    #[test]
    fn absent_assignments_are_unknown_but_absent_flags_in_complete_response_are_off() {
        for value in [
            json!({"ok":true}),
            json!({"ok":true,"assignments":null}),
            json!({"ok":false,"assignments":{}}),
        ] {
            assert!(parse_list_experiments(&value).is_err());
        }
        assert_eq!(
            parse_list_experiments(&json!({"ok":true,"assignments":{}})).unwrap(),
            ListExperiments::default()
        );
        let flags = parse_list_experiments(&json!({"ok":true,"assignments":{
            "lists_grid_todos":{"group":"on"}, "lists_todos_rollback":{"group":"control"},
            "lists_default_view_dual_write":{"group":"on"}, "unrelated":null
        }}))
        .unwrap();
        assert!(
            flags.grid_todos
                && flags.default_view_dual_write
                && !flags.todos_rollback
                && !flags.todos
        );
        let personal =
            parse_list_experiments(&json!({"ok":true,"assignments":{"todos":{"group":"on"}}}))
                .unwrap();
        assert!(personal.todos && !personal.grid_todos);
        assert!(
            parse_list_experiments(&json!({"ok":true,"assignments":{"todos":{"group":true}}}))
                .is_err()
        );
        for group in [Value::Null, json!(true), json!(4)] {
            assert!(
                parse_list_experiments(
                    &json!({"ok":true,"assignments":{"lists_grid_todos":{"group":group}}})
                )
                .is_err()
            );
        }
    }
    #[test]
    fn subitems_require_exact_assignment_and_positive_uncontradicted_envelope() {
        let flags = parse_list_experiments(
            &json!({"ok":true,"assignments":{"lists_sub_tasks":{"group":"on"}}}),
        )
        .unwrap();
        assert!(flags.sub_tasks);
        for group in ["control", "off", "ON"] {
            assert!(
                !parse_list_experiments(
                    &json!({"ok":true,"assignments":{"lists_sub_tasks":{"group":group}}})
                )
                .unwrap()
                .sub_tasks
            );
        }
        for raw in [
            json!({"ok":true,"error":"denied","assignments":{"lists_sub_tasks":{"group":"on"}}}),
            json!({"ok":true,"assignments":{"lists_sub_tasks":null}}),
            json!({"ok":true,"assignments":{"lists_sub_tasks":{"group":true}}}),
        ] {
            assert!(parse_list_experiments(&raw).is_err());
        }
    }
}
