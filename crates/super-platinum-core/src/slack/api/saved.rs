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

#[cfg(test)]
mod tests {
    use super::*;
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
