//! Exact retained jEXa/uGgu and e6np request forms. No real Slack fixture calls.
use super::*;
mod share;
use crate::later::SavedItem;
use crate::slack_lists::ListReminderEdit;
pub use share::*;
use std::collections::HashSet;

pub fn list_assignee_access(
    c: &SlackClient,
    w: &WorkspaceSession,
    list_id: &str,
    user_id: &str,
) -> PreparedRequest {
    c.rest_form(
        w,
        "files.canSee",
        vec![
            ("file_id", list_id.into()),
            ("user_id", user_id.into()),
            ("_x_reason", "canShare".into()),
        ],
    )
}
pub fn list_reminder_change(
    c: &SlackClient,
    w: &WorkspaceSession,
    edit: &ListReminderEdit,
) -> PreparedRequest {
    c.rest_form(w, edit.method(), edit.fields())
}
fn saved_record_page(
    c: &SlackClient,
    w: &WorkspaceSession,
    filter: &str,
    cursor: Option<String>,
) -> PreparedRequest {
    let mut fields = vec![
        ("limit", "50".into()),
        ("filter", filter.into()),
        ("include_tombstones", "true".into()),
        ("_x_reason", "saved-api/savedList".into()),
    ];
    if let Some(cursor) = cursor {
        fields.push(("cursor", cursor));
    }
    c.rest_form(w, "saved.list", fields)
}
/// A complete positive saved item supplies the date_created sentinel used when
/// clearing a To-do reminder. Do not use the first Later page as absence proof.
/// Any pagination bound, cursor cycle, malformed result or read failure is an
/// error. The caller also checks current owner/visit after this awaited read.
pub async fn fetch_saved_list_record(
    t: &Transport,
    c: &SlackClient,
    w: &WorkspaceSession,
    list_id: &str,
    row: &str,
) -> Result<Option<SavedItem>, Error> {
    for filter in ["todo", "saved", "archived", "completed"] {
        let mut cursor = None;
        let mut seen = HashSet::new();
        for _ in 0..200 {
            let response = t
                .execute(saved_record_page(c, w, filter, cursor.take()))
                .await?;
            let (found, next) = saved_record_page_result(&response, list_id, row)?;
            if let Some(item) = found {
                return Ok(Some(item));
            }
            if next.is_empty() {
                break;
            }
            if !seen.insert(next.clone()) || seen.len() == 200 {
                return Err(Error::Api(
                    "Saved-item pagination could not establish this reminder.".into(),
                ));
            }
            cursor = Some(next);
        }
    }
    Ok(None)
}
fn saved_record_page_result(
    response: &Value,
    list_id: &str,
    row: &str,
) -> Result<(Option<SavedItem>, String), Error> {
    let invalid =
        || Error::Transport("The saved List item response is incomplete or ambiguous.".into());
    if response.get("ok") != Some(&Value::Bool(true))
        || response.get("error").is_some_and(|v| !v.is_null())
    {
        return Err(invalid());
    }
    let items = response
        .get("saved_items")
        .and_then(Value::as_array)
        .ok_or_else(invalid)?;
    let mut found = None;
    for value in items {
        if !value.is_object()
            || value.get("item_type").and_then(Value::as_str).is_none()
            || value.get("item_id").and_then(Value::as_str).is_none()
        {
            return Err(invalid());
        }
        if value["item_type"] == "list_record" && value["item_id"] == list_id && value["ts"] == row
        {
            if found.is_some()
                || value
                    .get("date_created")
                    .and_then(Value::as_i64)
                    .is_none_or(|n| n <= 0)
                || value.get("state").and_then(Value::as_str).is_none()
            {
                return Err(invalid());
            }
            found = Some(serde_json::from_value(value.clone()).map_err(|_| invalid())?);
        }
    }
    let cursor = match response.get("response_metadata") {
        None => String::new(),
        Some(value) => match value.get("next_cursor") {
            None if value.is_object() => String::new(),
            Some(Value::String(s)) => s.clone(),
            _ => return Err(invalid()),
        },
    };
    Ok((found, cursor))
}
#[cfg(test)]
mod tests;
