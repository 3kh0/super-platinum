//! Ordinary draft wire builders. These neither authorize nor execute writes.
//!
//! Evidence: parity-worklog's October draft lifecycle captures and Slack's
//! bundled createDraftApi/updateDraftApi/deleteDraftApi/sendMessage/shareFile.
//! A caller must durably prepare the operation before dispatch, then revalidate
//! the captured account/session. WorkspaceSession alone cannot prove account
//! ownership. Mutation milliseconds are advisory, never server-side CAS.
use super::*;
use crate::drafts::sync::{Operation, OperationKind, Scope, Snapshot};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DraftRequestIdentity {
    pub operation_id: String,
    pub scope: Scope,
    pub client_msg_id: String,
    pub draft_id: Option<String>,
    pub kind: OperationKind,
    pub consume_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct DraftOperationRequest {
    pub request: PreparedRequest,
    /// Correlation only: neither a request UUID nor a draft deletion event
    /// constitutes positive proof of a send or schedule.
    pub identity: DraftRequestIdentity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DraftRequestError {
    #[error("draft operation does not match the authenticated workspace")]
    InvalidScope,
    #[error("draft operation identity is missing or invalid")]
    InvalidIdentity,
    #[error("draft operation's remote base is missing or inconsistent")]
    InvalidBase,
    #[error("draft operation's immutable content is missing or invalid")]
    InvalidSnapshot,
    #[error("draft destination metadata is unsupported or inconsistent")]
    UnsupportedDestination,
    #[error("draft mutation or schedule time is invalid")]
    InvalidTime,
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && !value
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == ',')
}
fn uuid(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|id| !id.is_nil())
}
fn timestamp(value: &str) -> bool {
    let (seconds, fraction) = value.split_once('.').unwrap_or((value, "0"));
    !seconds.is_empty()
        && seconds.bytes().all(|c| c.is_ascii_digit())
        && seconds.parse::<u64>().is_ok_and(|seconds| seconds > 0)
        && !fraction.is_empty()
        && fraction.bytes().all(|c| c.is_ascii_digit())
}
fn validate_snapshot(snapshot: &Snapshot, scope: &Scope) -> Result<(), DraftRequestError> {
    let [destination] = snapshot.destinations.as_slice() else {
        return Err(DraftRequestError::UnsupportedDestination);
    };
    let destination = destination
        .as_object()
        .ok_or(DraftRequestError::UnsupportedDestination)?;
    // Unknown routing metadata must not silently disappear on Send.
    if destination
        .keys()
        .any(|key| !matches!(key.as_str(), "channel_id" | "thread_ts" | "broadcast"))
        || destination
            .get("channel_id")
            .and_then(serde_json::Value::as_str)
            != Some(scope.key.channel.as_str())
        || match &scope.key.thread {
            Some(thread) => {
                destination
                    .get("thread_ts")
                    .and_then(serde_json::Value::as_str)
                    != Some(thread.as_str())
            }
            None => destination.contains_key("thread_ts"),
        }
        || destination
            .get("broadcast")
            .is_some_and(|value| !value.is_boolean())
        || (destination.contains_key("broadcast") && scope.key.thread.is_none())
    {
        return Err(DraftRequestError::UnsupportedDestination);
    }
    if snapshot.blocks.iter().any(|block| {
        !block.is_object()
            || !block
                .get("type")
                .and_then(serde_json::Value::as_str)
                .is_some_and(identifier)
    }) || snapshot.file_ids.iter().any(|id| !identifier(id))
    {
        return Err(DraftRequestError::InvalidSnapshot);
    }
    Ok(())
}

/// Build only from the immutable operation returned by the durable draft store.
/// No fresh operation/message UUID, hidden clock, retry, or network activity is
/// introduced here; SlackClient still supplies its normal transport metadata.
pub fn prepare_draft_operation(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    operation: &Operation,
    mutation_time_ms: u64,
) -> Result<DraftOperationRequest, DraftRequestError> {
    use DraftRequestError as Invalid;
    if !uuid(&operation.scope.account_id)
        || operation.scope.key.team != workspace.team_id
        || !identifier(&workspace.user_id)
        || !identifier(&operation.scope.key.team)
        || !identifier(&operation.scope.key.channel)
        || operation
            .scope
            .key
            .thread
            .as_deref()
            .is_some_and(|ts| !timestamp(ts))
    {
        return Err(Invalid::InvalidScope);
    }
    if !uuid(&operation.id) || !uuid(&operation.client_msg_id) {
        return Err(Invalid::InvalidIdentity);
    }
    if let Some(base) = &operation.base {
        if base.scope != operation.scope
            || base.client_msg_id != operation.client_msg_id
            || !identifier(&base.id)
        {
            return Err(Invalid::InvalidBase);
        }
        validate_snapshot(&base.snapshot, &operation.scope)?;
    }
    let id = operation.base.as_ref().map(|base| base.id.clone());
    let mut fields = vec![];
    let method;
    if operation.kind == OperationKind::Delete {
        if operation.snapshot.is_some() {
            return Err(Invalid::InvalidSnapshot);
        }
        fields.push(("draft_id", id.clone().ok_or(Invalid::InvalidBase)?));
        if mutation_time_ms == 0 {
            return Err(Invalid::InvalidTime);
        }
        fields.push(("client_last_updated_ts", mutation_time_ms.to_string()));
        fields.push(("skip_file_deletion", "true".into()));
        method = "drafts.delete";
    } else {
        let snapshot = operation
            .snapshot
            .as_ref()
            .ok_or(Invalid::InvalidSnapshot)?;
        validate_snapshot(snapshot, &operation.scope)?;
        fields.push(("blocks", serde_json::to_string(&snapshot.blocks).unwrap()));
        if operation.kind == OperationKind::Send {
            fields.push(("channel", operation.scope.key.channel.clone()));
            fields.push(("client_msg_id", operation.client_msg_id.clone()));
            if let Some(id) = &id {
                fields.push(("draft_id", id.clone()));
            }
            if let Some(thread) = &operation.scope.key.thread {
                fields.push(("thread_ts", thread.clone()));
            }
            let files = !snapshot.file_ids.is_empty();
            if let Some(broadcast) = snapshot.destinations[0].get("broadcast") {
                fields.push((
                    if files {
                        "broadcast"
                    } else {
                        "reply_broadcast"
                    },
                    broadcast.to_string(),
                ));
            }
            method = if files {
                fields.push(("files", snapshot.file_ids.join(",")));
                fields.push(("resharing_aware", "true".into()));
                fields.push(("skip_dlp_user_warning", "false".into()));
                fields.push(("from_share_modal", "false".into()));
                "files.share"
            } else {
                "chat.postMessage"
            };
        } else {
            fields.push((
                "destinations",
                serde_json::to_string(&snapshot.destinations).unwrap(),
            ));
            fields.push((
                "file_ids",
                serde_json::to_string(&snapshot.file_ids).unwrap(),
            ));
            let schedule = matches!(operation.kind, OperationKind::Schedule { .. });
            fields.push(("is_from_composer", schedule.to_string()));
            if let OperationKind::Schedule { post_at } = operation.kind {
                if post_at <= 0 {
                    return Err(Invalid::InvalidTime);
                }
                fields.push(("date_scheduled", post_at.to_string()));
            }
            match (operation.kind, &id) {
                (OperationKind::Create, None) | (OperationKind::Schedule { .. }, None) => {
                    fields.push(("client_msg_id", operation.client_msg_id.clone()));
                    method = "drafts.create";
                }
                (OperationKind::Update, Some(id)) | (OperationKind::Schedule { .. }, Some(id)) => {
                    if mutation_time_ms == 0 {
                        return Err(Invalid::InvalidTime);
                    }
                    fields.push(("draft_id", id.clone()));
                    fields.push(("client_last_updated_ts", mutation_time_ms.to_string()));
                    method = "drafts.update";
                }
                _ => return Err(Invalid::InvalidBase),
            }
        }
    }
    Ok(DraftOperationRequest {
        request: client.rest_form(workspace, method, fields),
        identity: DraftRequestIdentity {
            operation_id: operation.id.clone(),
            scope: operation.scope.clone(),
            client_msg_id: operation.client_msg_id.clone(),
            draft_id: id,
            kind: operation.kind,
            consume_id: operation.consume_id.clone(),
        },
    })
}

#[cfg(test)]
mod tests;
