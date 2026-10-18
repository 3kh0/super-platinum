use super::*;
use crate::slack::api::ScheduleMessage;

pub(super) fn mutation(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    record: &Record,
    now_ms: u64,
) -> Result<PreparedRequest, AppError> {
    record.validate()?;
    let intent = &record.intent;
    if record.phase != Phase::Unknown || record.attempt.is_none() {
        return Err(invalid("No reserved scheduled-forward attempt"));
    }
    if record.action == Action::Delete {
        let server = record
            .server_id()
            .ok_or_else(|| invalid("No reviewed server draft"))?;
        return Ok(client.rest_form(
            workspace,
            "drafts.delete",
            vec![
                ("draft_id", server.into()),
                ("client_last_updated_ts", now_ms.to_string()),
                ("skip_file_deletion", "false".into()),
                ("_x_reason", "DeleteDraftModal".into()),
            ],
        ));
    }
    let base = record.reviewed.as_ref().and_then(|v| v.get("draft"));
    let attachments = base
        .and_then(|v| v.get("attachments"))
        .cloned()
        .unwrap_or(json!([]));
    if !attachments.is_array() {
        return Err(invalid("Malformed retained draft attachments"));
    }
    // The actual form encoder serializes an empty simple array as an empty value.
    let attachments = if attachments.as_array().is_some_and(Vec::is_empty) {
        String::new()
    } else {
        serde_json::to_string(&attachments)?
    };
    let mut fields = vec![
        ("blocks", serde_json::to_string(&intent.blocks)?),
        (
            "destinations",
            serde_json::to_string(&intent.destination.wire())?,
        ),
        ("file_ids", "[]".into()),
        ("attachments", attachments),
        ("date_scheduled", intent.post_at.to_string()),
        ("is_from_composer", "true".into()),
        ("_x_reason", "schedule-draft".into()),
    ];
    let mut unfurls = match base.and_then(|v| v.get("unfurl")).filter(|v| !v.is_null()) {
        Some(Value::Array(items)) => items.clone(),
        Some(_) => return Err(invalid("Malformed retained draft unfurls")),
        None => vec![],
    };
    if intent.source.private {
        if let Some(own) = unfurls.iter_mut().find(|v| {
            v.get("url").and_then(Value::as_str) == Some(intent.source.permalink.as_str())
        }) {
            own["acknowledged_private_share"] = true.into();
        } else {
            unfurls.push(json!({"url":intent.source.permalink,"acknowledged_private_share":true}));
        }
    }
    if !unfurls.is_empty() {
        fields.push(("unfurl", serde_json::to_string(&unfurls)?));
    }
    for key in ["file_permissions", "date_stashed"] {
        if let Some(value) = base.and_then(|v| v.get(key)).filter(|v| !v.is_null()) {
            fields.push((
                key,
                match value {
                    Value::String(s) => s.clone(),
                    _ => serde_json::to_string(value)?,
                },
            ));
        }
    }
    let method = if let Some(server) = record.server_id() {
        fields.push(("draft_id", server.into()));
        fields.push(("client_last_updated_ts", now_ms.to_string()));
        "drafts.update"
    } else {
        fields.push(("client_msg_id", intent.client_msg_id.clone()));
        "drafts.create"
    };
    Ok(client.rest_form(workspace, method, fields))
}

pub(super) fn matching(record: &Record, response: &Value) -> bool {
    let Some(server) = response
        .get("draft")
        .and_then(|v| v.get("id"))
        .and_then(Value::as_str)
        .filter(|v| id(v))
    else {
        return false;
    };
    if record.server_id().is_some_and(|known| known != server) {
        return false;
    }
    let Some(draft) = crate::scheduled::edit::complete_readback(response, server) else {
        return false;
    };
    if draft.client_msg_id != record.intent.client_msg_id {
        return false;
    }
    if record.action == Action::Delete {
        // ACK-only or list absence never proves deletion. Positive full tombstone only.
        return draft.is_deleted
            && !draft.is_sent
            && draft.blocks == record.intent.blocks
            && draft.destinations == record.intent.destination.wire()
            && draft.file_ids.is_empty();
    }
    let payload = ScheduleMessage {
        destinations: record.intent.destination.wire(),
        blocks: record.intent.blocks.clone(),
        file_ids: vec![],
        post_at: record.intent.post_at,
        client_msg_id: record.intent.client_msg_id.clone(),
    };
    crate::scheduled::edit::confirms_submission(response, server, &payload)
}

/// A complete positive differing response is retained as conflict evidence.
/// An arbitrary partial/default-filled row is not a review.
pub(super) fn complete_for(record: &Record, response: &Value) -> bool {
    let Some(server) = response
        .get("draft")
        .and_then(|v| v.get("id"))
        .and_then(Value::as_str)
    else {
        return false;
    };
    record.server_id().is_none_or(|known| known == server)
        && crate::scheduled::edit::complete_readback(response, server)
            .is_some_and(|d| d.client_msg_id == record.intent.client_msg_id)
}

/// Bounded list pages can discover a lost create reply only through one complete
/// client-ID/payload match. Absence, duplicates and has_more cannot prove failure.
pub fn list_match(record: &Record, response: &Value) -> Option<Value> {
    if response.get("ok") != Some(&Value::Bool(true))
        || response.get("error").is_some_and(|v| !v.is_null())
    {
        return None;
    }
    let rows = response.get("drafts")?.as_array()?;
    let owned: Vec<_> = rows
        .iter()
        .filter(|v| {
            v.get("client_msg_id").and_then(Value::as_str)
                == Some(record.intent.client_msg_id.as_str())
        })
        .collect();
    if owned.len() != 1 {
        return None;
    }
    let envelope = json!({"ok":true,"draft":owned[0]});
    matching(record, &envelope).then_some(envelope)
}

/// Shape/identity admission for a read receipt before retaining storage retry.
/// This is not a mutation or review decision; Journal rechecks its exact record.
pub fn valid_readback(record: &Record, response: &Value) -> bool {
    complete_for(record, response)
}
