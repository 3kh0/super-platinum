//! `saved.*` request shapes verified against the Slack web client.
use super::*;
use crate::later::{LaterFilter, SavedAction, SavedItem, SavedKey, SavedPage};

pub fn saved_reminder(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    text: &str,
    date_due: i64,
) -> PreparedRequest {
    let description = serde_json::json!([{"type":"rich_text","elements":[{
        "type":"rich_text_section","elements":[{"type":"text","text":text}]
    }]}]);
    saved_reminder_blocks(client, workspace, None, &description, date_due)
}

pub fn saved_reminder_blocks(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    item: Option<&SavedKey>,
    description: &serde_json::Value,
    date_due: i64,
) -> PreparedRequest {
    let mut fields = vec![
        ("item_type", "reminder".into()),
        ("description", description.to_string()),
        ("date_due", date_due.to_string()),
    ];
    let method = if let Some(item) = item {
        fields.push(("item_id", item.item_id.clone()));
        "saved.update"
    } else {
        fields.push(("client_id", uuid::Uuid::new_v4().to_string()));
        "saved.add"
    };
    client.rest_form(workspace, method, fields)
}

pub async fn save_saved_reminder(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    item: Option<&SavedKey>,
    description: &[serde_json::Value],
    date_due: i64,
) -> Result<SavedItem, Error> {
    #[derive(serde::Deserialize)]
    struct Created {
        item: SavedItem,
    }
    let response = transport
        .execute(saved_reminder_blocks(
            client,
            workspace,
            item,
            &serde_json::Value::Array(description.to_vec()),
            date_due,
        ))
        .await?;
    Ok(decode::<Created>(response, "save reminder")?.item)
}

pub fn saved_list(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    filter: LaterFilter,
    cursor: Option<String>,
) -> PreparedRequest {
    let mut fields = vec![
        ("filter", filter.api().into()),
        ("limit", "50".into()),
        ("include_tombstones", "true".into()),
    ];
    if let Some(cursor) = cursor {
        fields.push(("cursor", cursor));
    }
    client.rest_form(workspace, "saved.list", fields)
}
pub fn saved_change(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    key: &SavedKey,
    action: SavedAction,
) -> PreparedRequest {
    let mut fields = vec![
        ("item_type", key.item_type.clone()),
        ("item_id", key.item_id.clone()),
    ];
    if !key.ts.is_empty() {
        fields.push(("ts", key.ts.clone()));
    }
    let method = match action {
        SavedAction::Save => "saved.add",
        SavedAction::Remove => "saved.delete",
        SavedAction::Archive => {
            fields.push(("mark", "archived".into()));
            "saved.update"
        }
        SavedAction::Unarchive => {
            fields.push(("mark", "unarchived".into()));
            "saved.update"
        }
        SavedAction::Complete => {
            fields.extend([
                ("mark", "completed".into()),
                ("todo_state", "completed".into()),
                ("date_due", "0".into()),
            ]);
            "saved.update"
        }
        SavedAction::Restore => {
            fields.extend([
                ("mark", "uncompleted".into()),
                ("todo_state", "saved".into()),
                ("date_due", "0".into()),
            ]);
            "saved.update"
        }
        SavedAction::Reminder(due) => {
            fields.push(("date_due", due.to_string()));
            "saved.update"
        }
    };
    client.rest_form(workspace, method, fields)
}
pub async fn fetch_saved(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    filter: LaterFilter,
    cursor: Option<String>,
) -> Result<SavedPage, Error> {
    decode(
        transport
            .execute(saved_list(client, workspace, filter, cursor))
            .await?,
        "saved.list",
    )
}
pub async fn change_saved(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    key: &SavedKey,
    action: SavedAction,
) -> Result<Option<SavedItem>, Error> {
    #[derive(serde::Deserialize)]
    struct Updated {
        item: SavedItem,
    }
    let value = transport
        .execute(saved_change(client, workspace, key, action))
        .await?;
    if matches!(action, SavedAction::Remove) {
        Ok(None)
    } else {
        Ok(Some(decode::<Updated>(value, "saved mutation")?.item))
    }
}

/// Positive-only file lookup. A missing entry is deliberately not evidence that
/// a file is unsaved: filters, pagination and tombstones have distinct meanings.
pub async fn find_saved_file(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    file_id: &str,
) -> Result<Option<SavedItem>, Error> {
    for filter in LaterFilter::ALL {
        let value = transport
            .execute(saved_list(client, workspace, filter, None))
            .await?;
        require_saved_ok(&value)?;
        if !value
            .get("saved_items")
            .is_some_and(serde_json::Value::is_array)
        {
            return Err(Error::Transport("invalid saved file list".into()));
        }
        let page: SavedPage = decode(value, "saved file list")?;
        let mut matches = page.saved_items.into_iter().filter(|item| {
            item.item_type == "file" && item.item_id == file_id && item.ts.is_empty()
        });
        if let Some(item) = matches.next() {
            if matches.next().is_some() {
                return Err(Error::Transport("ambiguous saved file identity".into()));
            }
            return Ok(Some(item));
        }
    }
    Ok(None)
}

fn require_saved_ok(value: &serde_json::Value) -> Result<(), Error> {
    if value.get("ok").and_then(serde_json::Value::as_bool) != Some(true) {
        return Err(Error::Transport(
            "saved response did not confirm success".into(),
        ));
    }
    Ok(())
}

/// Validate the exact captured request identity. Delete has no required item in
/// Slack's response; ok:true is acknowledgement of this request, not readback.
pub fn decode_saved_file_mutation(
    value: serde_json::Value,
    file_id: &str,
    action: SavedAction,
) -> Result<Option<SavedItem>, Error> {
    require_saved_ok(&value)?;
    if file_id.is_empty() {
        return Err(Error::Transport("invalid saved file identity".into()));
    }
    if let Some(item) = value.get("item") {
        let contradicted = match action {
            SavedAction::Complete => {
                item.get("date_completed")
                    .and_then(serde_json::Value::as_i64)
                    .is_some_and(|at| at <= 0)
                    || item
                        .get("todo_state")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|state| state != "completed")
            }
            SavedAction::Restore => {
                item.get("date_completed")
                    .and_then(serde_json::Value::as_i64)
                    .is_some_and(|at| at > 0)
                    || item
                        .get("todo_state")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|state| state != "saved")
            }
            SavedAction::Archive => {
                item.get("is_archived").and_then(serde_json::Value::as_bool) == Some(false)
            }
            SavedAction::Unarchive => {
                item.get("is_archived").and_then(serde_json::Value::as_bool) == Some(true)
            }
            SavedAction::Reminder(due) => item
                .get("date_due")
                .and_then(serde_json::Value::as_i64)
                .is_some_and(|actual| actual != due),
            SavedAction::Save => {
                item.get("state").and_then(serde_json::Value::as_str) == Some("deleted")
            }
            SavedAction::Remove => false,
        };
        let state = item.get("state").and_then(serde_json::Value::as_str);
        let state_contradicted = match action {
            SavedAction::Complete => matches!(
                state,
                Some("in_progress" | "saved" | "uncompleted" | "deleted")
            ),
            SavedAction::Restore => matches!(state, Some("completed" | "deleted")),
            SavedAction::Archive => matches!(
                state,
                Some("in_progress" | "saved" | "completed" | "deleted")
            ),
            SavedAction::Unarchive => matches!(state, Some("archived" | "deleted")),
            _ => false,
        };
        if contradicted || state_contradicted {
            return Err(Error::Transport(
                "saved response contradicts requested file change".into(),
            ));
        }
        let item: SavedItem = decode(item.clone(), "saved file item")?;
        if item.item_type != "file" || item.item_id != file_id || !item.ts.is_empty() {
            return Err(Error::Transport(
                "saved response file identity mismatch".into(),
            ));
        }
        return Ok((!matches!(action, SavedAction::Remove)).then_some(item));
    }
    if matches!(action, SavedAction::Remove) {
        Ok(None)
    } else {
        Err(Error::Transport("saved response missing file item".into()))
    }
}

pub async fn change_saved_file(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    file_id: &str,
    action: SavedAction,
) -> Result<Option<SavedItem>, Error> {
    let key = SavedKey {
        item_type: "file".into(),
        item_id: file_id.into(),
        ts: String::new(),
    };
    let value = transport
        .execute(saved_change(client, workspace, &key, action))
        .await?;
    decode_saved_file_mutation(value, file_id, action)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn file_mutations_require_success_and_exact_identity() {
        let item = serde_json::json!({"item_type":"file","item_id":"F1"});
        for ok in [
            serde_json::Value::Null,
            serde_json::json!(false),
            serde_json::json!("true"),
            serde_json::json!(1),
        ] {
            assert!(
                decode_saved_file_mutation(
                    serde_json::json!({"ok":ok,"item":item}),
                    "F1",
                    SavedAction::Save
                )
                .is_err()
            );
        }
        assert!(
            decode_saved_file_mutation(
                serde_json::json!({"ok":true,"item":item}),
                "F1",
                SavedAction::Save
            )
            .unwrap()
            .is_some()
        );
        for wrong in [
            serde_json::json!({"item_type":"message","item_id":"F1"}),
            serde_json::json!({"item_type":"file","item_id":"F2"}),
            serde_json::json!({"item_type":"file","item_id":"F1","ts":"1.2"}),
        ] {
            assert!(
                decode_saved_file_mutation(
                    serde_json::json!({"ok":true,"item":wrong}),
                    "F1",
                    SavedAction::Save
                )
                .is_err()
            );
            assert!(
                decode_saved_file_mutation(
                    serde_json::json!({"ok":true,"item":wrong}),
                    "F1",
                    SavedAction::Remove
                )
                .is_err()
            );
        }
        assert!(
            decode_saved_file_mutation(serde_json::json!({"ok":true}), "F1", SavedAction::Save)
                .is_err()
        );
        assert!(
            decode_saved_file_mutation(serde_json::json!({"ok":true}), "F1", SavedAction::Remove)
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn contradictory_file_state_does_not_confirm_requested_change() {
        for (action, fields) in [
            (
                SavedAction::Complete,
                serde_json::json!({"state":"in_progress"}),
            ),
            (
                SavedAction::Restore,
                serde_json::json!({"state":"completed"}),
            ),
            (
                SavedAction::Archive,
                serde_json::json!({"state":"in_progress"}),
            ),
            (
                SavedAction::Unarchive,
                serde_json::json!({"state":"archived"}),
            ),
            (
                SavedAction::Complete,
                serde_json::json!({"date_completed":0,"todo_state":"saved"}),
            ),
            (
                SavedAction::Restore,
                serde_json::json!({"date_completed":123}),
            ),
            (
                SavedAction::Archive,
                serde_json::json!({"is_archived":false}),
            ),
            (
                SavedAction::Unarchive,
                serde_json::json!({"is_archived":true}),
            ),
            (
                SavedAction::Reminder(123),
                serde_json::json!({"date_due":456}),
            ),
        ] {
            let mut item = serde_json::json!({"item_type":"file","item_id":"F1"});
            item.as_object_mut()
                .unwrap()
                .extend(fields.as_object().unwrap().clone());
            assert!(
                decode_saved_file_mutation(
                    serde_json::json!({"ok":true,"item":item}),
                    "F1",
                    action
                )
                .is_err()
            );
        }
    }
    #[test]
    fn file_requests_have_no_message_timestamp() {
        let workspace = WorkspaceSession {
            team_id: "T1".into(),
            enterprise_id: None,
            user_id: "U1".into(),
            name: "Test".into(),
            url: "https://test.slack.com".into(),
            token: "fixture".into(),
        };
        for action in [
            SavedAction::Save,
            SavedAction::Remove,
            SavedAction::Complete,
            SavedAction::Archive,
            SavedAction::Reminder(123),
        ] {
            let request = saved_change(
                &SlackClient::default(),
                &workspace,
                &SavedKey::file("F1"),
                action,
            );
            let crate::slack::client::RequestBody::Form(fields) = request.body else {
                panic!()
            };
            assert!(fields.contains(&("item_type".into(), "file".into())));
            assert!(fields.contains(&("item_id".into(), "F1".into())));
            assert!(!fields.iter().any(|(key, _)| key == "ts"));
        }
    }
    #[test]
    fn editing_a_reminder_preserves_rich_blocks_and_uses_its_existing_identity() {
        let workspace = WorkspaceSession {
            team_id: "T1".into(),
            enterprise_id: None,
            user_id: "U1".into(),
            name: "Test".into(),
            url: "https://test.slack.com".into(),
            token: "fixture".into(),
        };
        let description = serde_json::json!([{"type":"rich_text","block_id":"keep","elements":[{"type":"rich_text_section","elements":[{"type":"user","user_id":"U1"},{"type":"text","text":"review","style":{"bold":true}}]}]}]);
        let key = SavedKey {
            item_type: "reminder".into(),
            item_id: "Sa1".into(),
            ts: String::new(),
        };
        let request = saved_reminder_blocks(
            &SlackClient::default(),
            &workspace,
            Some(&key),
            &description,
            0,
        );
        let super::super::super::client::RequestBody::Form(fields) = request.body else {
            panic!("form expected")
        };
        assert!(fields.contains(&("item_id".into(), "Sa1".into())));
        assert!(fields.contains(&("date_due".into(), "0".into())));
        assert!(
            !fields
                .iter()
                .any(|(key, _)| key == "client_id" || key == "ts")
        );
        let encoded = &fields
            .iter()
            .find(|(key, _)| key == "description")
            .unwrap()
            .1;
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(encoded).unwrap(),
            description
        );
    }
    #[test]
    fn new_reminder_encodes_unicode_lines_as_rich_text_and_has_retry_identity() {
        let workspace = WorkspaceSession {
            team_id: "T1".into(),
            enterprise_id: None,
            user_id: "U1".into(),
            name: "Test".into(),
            url: "https://test.slack.com".into(),
            token: "fixture".into(),
        };
        let request = saved_reminder(&SlackClient::default(), &workspace, "Review 🚀\nnotes", 123);
        let super::super::super::client::RequestBody::Form(fields) = request.body else {
            panic!("form expected")
        };
        let field = |key: &str| fields.iter().find(|(k, _)| k == key).unwrap().1.clone();
        assert_eq!(field("item_type"), "reminder");
        assert_eq!(field("date_due"), "123");
        assert!(uuid::Uuid::parse_str(&field("client_id")).is_ok());
        let description: serde_json::Value = serde_json::from_str(&field("description")).unwrap();
        assert_eq!(
            description[0]["elements"][0]["elements"][0]["text"],
            "Review 🚀\nnotes"
        );
        assert!(
            !fields
                .iter()
                .any(|(key, _)| key == "item_id" || key == "ts")
        );
    }
    #[test]
    fn complete_clears_reminder_and_preserves_message_identity() {
        let workspace = WorkspaceSession {
            team_id: "T1".into(),
            enterprise_id: None,
            user_id: "U1".into(),
            name: "Test".into(),
            url: "https://test.slack.com".into(),
            token: "test-secret".into(),
        };
        let request = saved_change(
            &SlackClient::default(),
            &workspace,
            &SavedKey::message("C1", "123.456"),
            SavedAction::Complete,
        );
        let super::super::super::client::RequestBody::Form(fields) = request.body else {
            panic!("form expected")
        };
        for field in [
            ("mark", "completed"),
            ("todo_state", "completed"),
            ("date_due", "0"),
            ("item_id", "C1"),
            ("ts", "123.456"),
        ] {
            assert!(fields.contains(&(field.0.into(), field.1.into())));
        }
    }
}
