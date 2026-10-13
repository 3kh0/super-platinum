use super::ListError;
use serde_json::Value;

/// Authenticated user preferences identifying the personal Lists whose completion
/// handler may also update a referenced List. Never infer these from `todo_mode`.
/// Keep this scoped to the account that supplied the response and refresh before writes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersonalTaskLists {
    first_task_list: Option<String>,
    assigned_to_list: Option<String>,
}

impl PersonalTaskLists {
    /// Captured Nu4c reads response.prefs; 4iNl parses task_lists as a JSON string,
    /// and j2uc compares only its first entry plus assigned_to_list. Missing or
    /// malformed requested preferences do not establish an ordinary List.
    pub fn from_response(response: &Value) -> Result<Self, ListError> {
        if response.get("ok") != Some(&Value::Bool(true)) {
            return Err(ListError("Personal To-dos preferences were not verified."));
        }
        let prefs = response
            .get("prefs")
            .and_then(Value::as_object)
            .ok_or(ListError("Personal To-dos preferences are missing."))?;
        let tasks = prefs
            .get("task_lists")
            .and_then(Value::as_str)
            .ok_or(ListError(
                "Personal To-dos preferences are missing or malformed.",
            ))?;
        let assigned = prefs
            .get("assigned_to_list")
            .and_then(Value::as_str)
            .ok_or(ListError(
                "Personal To-dos preferences are missing or malformed.",
            ))?;
        // Empty string is the captured selector's explicit empty preference.
        let tasks: Vec<String> = if tasks.is_empty() {
            Vec::new()
        } else {
            serde_json::from_str(tasks)
                .map_err(|_| ListError("Personal To-dos list identities are malformed."))?
        };
        if tasks.iter().any(|id| id.is_empty() || id.trim() != id) || assigned.trim() != assigned {
            return Err(ListError("Personal To-dos list identities are malformed."));
        }
        Ok(Self {
            first_task_list: tasks.into_iter().next(),
            assigned_to_list: (!assigned.is_empty()).then(|| assigned.to_owned()),
        })
    }

    pub fn contains(&self, list_id: &str) -> bool {
        self.first_task_list.as_deref() == Some(list_id)
            || self.assigned_to_list.as_deref() == Some(list_id)
    }
}

pub(super) fn ordinary_boolean_kind(
    kind: &str,
    list_id: &str,
    personal_tasks: Option<&PersonalTaskLists>,
) -> bool {
    match kind {
        "checkbox" => true,
        "completed" | "todo_completed" => {
            personal_tasks.is_some_and(|policy| !policy.contains(list_id))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn personal_ids_follow_first_task_list_and_assigned_list_only() {
        let policy = PersonalTaskLists::from_response(&json!({"ok":true,"prefs":{
            "task_lists":"[\"F_MY\",\"F_OTHER\"]","assigned_to_list":"F_ASSIGNED"
        }}))
        .unwrap();
        assert!(policy.contains("F_MY"));
        assert!(policy.contains("F_ASSIGNED"));
        assert!(!policy.contains("F_OTHER"));
        assert!(!ordinary_boolean_kind(
            "todo_completed",
            "F_MY",
            Some(&policy)
        ));
        assert!(ordinary_boolean_kind(
            "todo_completed",
            "F_OTHER",
            Some(&policy)
        ));
        assert!(!ordinary_boolean_kind("todo_completed", "F_OTHER", None));
    }

    #[test]
    fn verified_empty_is_distinct_from_unavailable_or_malformed_preferences() {
        for task_lists in ["", "[]"] {
            let policy = PersonalTaskLists::from_response(&json!({"ok":true,"prefs":{
                "task_lists":task_lists,"assigned_to_list":""
            }}))
            .unwrap();
            assert!(!policy.contains("F_LIST"));
        }
        for response in [
            json!({"ok":false,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
            json!({"ok":true}),
            json!({"ok":true,"prefs":{}}),
            json!({"ok":true,"prefs":{"task_lists":"[]"}}),
            json!({"ok":true,"prefs":{"task_lists":[],"assigned_to_list":""}}),
            json!({"ok":true,"prefs":{"task_lists":"null","assigned_to_list":""}}),
            json!({"ok":true,"prefs":{"task_lists":"[1]","assigned_to_list":""}}),
            json!({"ok":true,"prefs":{"task_lists":"[\"\"]","assigned_to_list":""}}),
            json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":false}}),
        ] {
            assert!(PersonalTaskLists::from_response(&response).is_err());
        }
    }
}
