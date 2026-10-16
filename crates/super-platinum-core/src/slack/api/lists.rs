//! Ordinary Lists requests from the captured desktop client, with no guessed pagination.
use super::*;
use crate::slack_lists::{CellEdit, CheckboxEdit, Document, PersonalTaskLists, RecordsPage};
use serde_json::Value;

mod column_create;
mod column_precision;
mod column_rename;
mod field_visibility;
mod view_filter;
mod view_grouping;
pub use column_create::*;
mod view_sort;
pub use view_filter::*;
pub use view_grouping::*;
pub use view_sort::*;
mod view_create;
mod view_rename;
pub use column_precision::*;
pub use column_rename::*;
pub use field_visibility::*;
pub use view_create::*;
pub use view_rename::*;
mod subitem;
pub use subitem::*;
mod creation;
pub use creation::*;
mod deletion;
pub use deletion::*;
mod hydration;
pub use hydration::*;
mod records;
pub use records::*;
mod viewing;
pub use viewing::*;
mod defaults;
pub use defaults::*;

pub fn lists_records_list(c: &SlackClient, w: &WorkspaceSession, id: &str) -> PreparedRequest {
    c.rest_form(
        w,
        "lists.records.list",
        vec![
            ("list_id", id.into()),
            ("include_subtasks", "true".into()),
            ("archived", "false".into()),
            ("include_suggested", "false".into()),
            ("_x_reason", "get-list-records".into()),
        ],
    )
}
pub async fn fetch_list_document(
    t: &Transport,
    c: &SlackClient,
    w: &WorkspaceSession,
    id: &str,
) -> Result<Document, Error> {
    let response = t.execute(file_info(c, w, id)).await?;
    let raw = response
        .get("file")
        .cloned()
        .ok_or_else(|| Error::Api("List file is missing.".into()))?;
    let document = Document::parse(raw).map_err(|e| Error::Api(e.to_string()))?;
    if document.id != id {
        return Err(Error::Api(
            "List file identity did not match the request.".into(),
        ));
    }
    Ok(document)
}
pub async fn fetch_list_records(
    t: &Transport,
    c: &SlackClient,
    w: &WorkspaceSession,
    id: &str,
) -> Result<RecordsPage, Error> {
    let response = t.execute(lists_records_list(c, w, id)).await?;
    parse_list_records(response, id)
}
pub fn parse_list_records(response: Value, id: &str) -> Result<RecordsPage, Error> {
    if response
        .get("list_id")
        .is_some_and(|v| v.as_str() != Some(id))
    {
        return Err(Error::Api(
            "List records identity did not match the request.".into(),
        ));
    }
    let page = RecordsPage::parse(response).map_err(|e| Error::Api(e.to_string()))?;
    if page.records.iter().any(|record| {
        record
            .raw
            .get("list_id")
            .is_some_and(|v| v.as_str() != Some(id))
    }) {
        return Err(Error::Api(
            "List record owner did not match the request.".into(),
        ));
    }
    Ok(page)
}
pub fn list_checkbox_update(
    c: &SlackClient,
    w: &WorkspaceSession,
    edit: &CheckboxEdit,
) -> PreparedRequest {
    // Absent from PreparedRequest::retry_safe's read allowlist: one transport attempt.
    c.rest_form(
        w,
        "lists.cells.update",
        vec![
            ("list_id", edit.list_id().into()),
            ("cells", edit.cells().to_string()),
            ("_x_reason", "bulk-update-cells".into()),
        ],
    )
}
pub fn list_cell_update(c: &SlackClient, w: &WorkspaceSession, edit: &CellEdit) -> PreparedRequest {
    // Same captured ordinary existing-cell endpoint; intentionally one attempt.
    c.rest_form(
        w,
        "lists.cells.update",
        vec![
            ("list_id", edit.list_id().into()),
            ("cells", edit.cells().to_string()),
            ("_x_reason", "bulk-update-cells".into()),
        ],
    )
}
pub fn list_allow_lists(c: &SlackClient, w: &WorkspaceSession) -> PreparedRequest {
    if enterprise_lists_permission(w) {
        // Captured fetchEnterprisePrefsData/gBKr: no target_team argument.
        // nxvY -> CtFl.SI reads the top-level locked_prefs.allow_lists value.
        return c.rest_form(
            w,
            "enterprise.prefs.get",
            vec![("_x_reason", "native-list-permission".into())],
        );
    }
    // Captured mL3A -> PAdb: team.prefs.get({targetTeam}), response.prefs.
    c.rest_form(
        w,
        "team.prefs.get",
        vec![
            ("target_team", w.team_id.clone()),
            ("_x_reason", "native-list-permission".into()),
        ],
    )
}
/// Captured Nu4c getUserPrefsByApi: prefs is a comma-separated selection and
/// response.prefs retains task_lists as its JSON-encoded string.
pub fn list_personal_task_preferences(c: &SlackClient, w: &WorkspaceSession) -> PreparedRequest {
    c.rest_form(
        w,
        "users.prefs.get",
        vec![
            ("prefs", "task_lists,assigned_to_list".into()),
            ("_x_reason", "native-list-completion-permission".into()),
        ],
    )
}
pub async fn fetch_list_personal_task_lists(
    t: &Transport,
    c: &SlackClient,
    w: &WorkspaceSession,
) -> Result<PersonalTaskLists, Error> {
    let response = t.execute(list_personal_task_preferences(c, w)).await?;
    parse_list_personal_task_lists(&response)
}
pub fn parse_list_personal_task_lists(response: &Value) -> Result<PersonalTaskLists, Error> {
    PersonalTaskLists::from_response(response).map_err(|e| Error::Api(e.to_string()))
}
pub async fn fetch_list_allow_lists(
    t: &Transport,
    c: &SlackClient,
    w: &WorkspaceSession,
) -> Result<Option<String>, Error> {
    let response = t.execute(list_allow_lists(c, w)).await?;
    if enterprise_lists_permission(w) {
        parse_enterprise_list_allow_lists(&response)
    } else {
        parse_list_allow_lists(&response)
    }
}
fn enterprise_lists_permission(w: &WorkspaceSession) -> bool {
    w.team_id.starts_with('E') || w.enterprise_id.as_deref().is_some_and(|id| !id.is_empty())
}
pub fn parse_list_allow_lists(response: &Value) -> Result<Option<String>, Error> {
    parse_list_permission(response, "prefs")
}
pub fn parse_enterprise_list_allow_lists(response: &Value) -> Result<Option<String>, Error> {
    parse_list_permission(response, "locked_prefs")
}
fn parse_list_permission(response: &Value, field: &str) -> Result<Option<String>, Error> {
    if response["ok"] != true {
        return Err(Error::Api(
            "List permission lookup was not successful.".into(),
        ));
    }
    let prefs = response
        .get(field)
        .and_then(Value::as_object)
        .ok_or_else(|| Error::Api("Workspace preferences are missing or malformed.".into()))?;
    match prefs.get("allow_lists") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        _ => Err(Error::Api("List editing permission is malformed.".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slack::client::RequestBody;
    use serde_json::json;
    fn workspace() -> WorkspaceSession {
        WorkspaceSession {
            team_id: "T_LIST_FIXTURE".into(),
            enterprise_id: None,
            user_id: "U_LIST_FIXTURE".into(),
            name: "Fixture".into(),
            url: "https://example.test".into(),
            token: "fixture-not-a-token".into(),
        }
    }
    #[test]
    fn list_requests_use_captured_fields_and_never_open_or_guess_cursor() {
        let c = SlackClient::default();
        let w = workspace();
        let req = lists_records_list(&c, &w, "F_LIST");
        let RequestBody::Form(fields) = req.body else {
            panic!("expected form")
        };
        assert_eq!(
            fields.iter().find(|(k, _)| k == "list_id").unwrap().1,
            "F_LIST"
        );
        assert!(fields.contains(&("include_subtasks".into(), "true".into())));
        assert!(!fields.iter().any(|(k, _)| k == "cursor" || k == "limit"));
        assert!(req.url.contains("/api/lists.records.list?"));
        let RequestBody::Form(fields) = list_allow_lists(&c, &w).body else {
            panic!()
        };
        assert!(fields.contains(&("target_team".into(), w.team_id)));
    }
    #[test]
    fn malformed_permissions_or_mismatched_record_owner_are_not_accepted() {
        assert_eq!(
            parse_list_allow_lists(&json!({"ok":true,"prefs":{}})).unwrap(),
            None
        );
        assert_eq!(
            parse_list_allow_lists(&json!({"ok":true,"prefs":{"allow_lists":"full"}}))
                .unwrap()
                .as_deref(),
            Some("full")
        );
        assert!(parse_list_allow_lists(&json!({"ok":true,"prefs":{"allow_lists":true}})).is_err());
        assert!(
            parse_list_records(
                json!({"ok":true,"list_id":"F_OTHER","records":[]}),
                "F_LIST"
            )
            .is_err()
        );
        assert!(
            parse_list_records(
                json!({"ok":true,"records":[{"id":"R1","list_id":"F_OTHER","fields":[]}]}),
                "F_LIST"
            )
            .is_err()
        );
    }
    #[test]
    fn enterprise_permissions_use_org_endpoint_without_a_guessed_child_team() {
        let client = SlackClient::default();
        for imported_enterprise_id in [false, true] {
            let mut workspace = workspace();
            if imported_enterprise_id {
                // Grid child sessions explicitly carry their enterprise ID.
                workspace.enterprise_id = Some("E_LIST_FIXTURE".into());
            } else {
                // Imported unified sessions may only identify the organization.
                workspace.team_id = "E_LIST_FIXTURE".into();
            }
            let request = list_allow_lists(&client, &workspace);
            assert!(request.url.contains("/api/enterprise.prefs.get?"));
            let RequestBody::Form(fields) = request.body else {
                panic!()
            };
            assert!(!fields.iter().any(|(key, _)| key == "target_team"));
        }
    }
    #[test]
    fn personal_task_preferences_use_authenticated_named_pref_lookup() {
        let client = SlackClient::default();
        let request = list_personal_task_preferences(&client, &workspace());
        assert!(request.url.contains("/api/users.prefs.get?"));
        let RequestBody::Form(fields) = request.body else {
            panic!()
        };
        assert!(fields.contains(&("prefs".into(), "task_lists,assigned_to_list".into())));
        assert!(
            !fields
                .iter()
                .any(|(key, _)| matches!(key.as_str(), "user" | "team" | "target_team"))
        );
        assert!(
            parse_list_personal_task_lists(&json!({"ok":true,"prefs":{
                "task_lists":"[]","assigned_to_list":""
            }}))
            .is_ok()
        );
        assert!(parse_list_personal_task_lists(&json!({"ok":true,"prefs":{}})).is_err());
    }
    #[test]
    fn enterprise_permission_reads_only_locked_value_and_never_defaults_to_full() {
        let response = json!({"ok":true,
            "locked_prefs":{"allow_lists":"read_only"},
            "unlocked_prefs":{"allow_lists":"full"},
            "org_prefs":{"allow_lists":"full"},
            "prefs":{"allow_lists":"full"}});
        assert_eq!(
            parse_enterprise_list_allow_lists(&response)
                .unwrap()
                .as_deref(),
            Some("read_only")
        );
        assert_eq!(
            parse_enterprise_list_allow_lists(
                &json!({"ok":true,"locked_prefs":{"allow_lists":"full"}})
            )
            .unwrap()
            .as_deref(),
            Some("full")
        );
        assert_eq!(
            parse_enterprise_list_allow_lists(
                &json!({"ok":true,"locked_prefs":{},"prefs":{"allow_lists":"full"}})
            )
            .unwrap(),
            None
        );
        assert!(
            parse_enterprise_list_allow_lists(&json!({"ok":true,"prefs":{"allow_lists":"full"}}))
                .is_err()
        );
        assert!(
            parse_enterprise_list_allow_lists(
                &json!({"ok":true,"locked_prefs":{"allow_lists":true}})
            )
            .is_err()
        );
        assert!(
            parse_list_allow_lists(&json!({"ok":true,"locked_prefs":{"allow_lists":"full"}}))
                .is_err()
        );
    }
}
