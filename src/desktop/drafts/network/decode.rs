//! Preserve Slack's raw draft blocks/destinations; never parse display text.
use super::*;
use super_platinum_core::drafts::sync::{DraftSync, OperationKind, RemoteDraft, Snapshot};

fn uncertain(issue: Issue) -> (Outcome, Option<Issue>) {
    (Outcome::Uncertain, Some(issue))
}
pub(super) fn timestamp(value: &str) -> bool {
    value.split_once('.').is_some_and(|(seconds, fraction)| {
        seconds.parse::<u64>().is_ok_and(|seconds| seconds > 0)
            && seconds.bytes().all(|byte| byte.is_ascii_digit())
            && fraction.len() == 6
            && fraction.bytes().all(|byte| byte.is_ascii_digit())
    })
}
fn epoch(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| value.as_str()?.parse().ok())
}
fn optional_matches(value: &Value, key: &str, expected: Option<&str>) -> bool {
    value
        .get(key)
        .is_none_or(|actual| expected.is_some_and(|expected| actual.as_str() == Some(expected)))
}

pub(super) fn response(
    operation: &Operation,
    workspace: &WorkspaceSession,
    value: &Value,
) -> (Outcome, Option<Issue>) {
    if value.get("ok").and_then(Value::as_bool) != Some(true)
        || value.get("error").is_some_and(|error| !error.is_null())
    {
        return uncertain(Issue::MalformedResponse);
    }
    if operation.kind != OperationKind::Send {
        let fields: &[&str] = if operation.kind == OperationKind::Delete {
            &["draft_id", "client_msg_id", "team_id", "user", "user_id"]
        } else {
            &[
                "draft",
                "draft_id",
                "client_msg_id",
                "team_id",
                "user",
                "user_id",
            ]
        };
        if !acknowledgement::envelope(value, fields)
            || !optional_matches(value, "team_id", Some(&workspace.team_id))
            || !optional_matches(value, "user", Some(&workspace.user_id))
            || !optional_matches(value, "user_id", Some(&workspace.user_id))
            || !optional_matches(value, "client_msg_id", Some(&operation.client_msg_id))
        {
            return uncertain(Issue::MalformedResponse);
        }
    }
    match operation.kind {
        OperationKind::Create | OperationKind::Update | OperationKind::Schedule { .. } => {
            draft(operation, workspace, value)
        }
        OperationKind::Delete => {
            let Some(base) = &operation.base else {
                return uncertain(Issue::MalformedResponse);
            };
            if !optional_matches(value, "draft_id", Some(&base.id))
                || !optional_matches(value, "client_msg_id", Some(&operation.client_msg_id))
            {
                return uncertain(Issue::MalformedResponse);
            }
            // This is the positive response to the captured exact draft delete,
            // never an inference from drafts.list absence or draft_delete RTM.
            (
                Outcome::Deleted {
                    draft_id: base.id.clone(),
                },
                None,
            )
        }
        OperationKind::Send => sent(operation, workspace, value),
    }
}

#[derive(serde::Deserialize)]
struct WireDraft {
    id: String,
    client_msg_id: String,
    last_updated_ts: String,
    blocks: Vec<Value>,
    file_ids: Vec<String>,
    destinations: Vec<Value>,
    is_deleted: bool,
    is_sent: bool,
    date_scheduled: Value,
}
fn draft(
    operation: &Operation,
    workspace: &WorkspaceSession,
    value: &Value,
) -> (Outcome, Option<Issue>) {
    let Some(raw) = value.get("draft") else {
        return uncertain(Issue::MalformedResponse);
    };
    // Match the conservative positive draft-review shape. A new lifecycle or
    // routing field is not safe to discard when acknowledging a mutation.
    // Retained B/S hashes: docs/slack-external-select-protocol.md. B xgGU
    // 6823050 carries the server flag into the unified draft; S Hxeg 2374208
    // selects false for channel drafts. B Id6n 6810796 checks the Schedule
    // response flag is true. Typed optional booleans below are conservative;
    // absence keeps the existing supported response shape.
    const FIELDS: &[&str] = &[
        "id",
        "client_msg_id",
        "team_id",
        "user",
        "user_id",
        "last_updated_ts",
        "blocks",
        "file_ids",
        "destinations",
        "is_deleted",
        "is_sent",
        "date_scheduled",
        "date_created",
        "attachments",
        "is_from_composer",
    ];
    if raw
        .as_object()
        .is_some_and(|object| object.keys().any(|key| !FIELDS.contains(&key.as_str())))
    {
        return uncertain(Issue::UnsupportedResponse);
    }
    let Ok(wire) = serde_json::from_value::<WireDraft>(raw.clone()) else {
        return uncertain(Issue::MalformedResponse);
    };
    if wire.is_deleted
        || wire.is_sent
        || wire.client_msg_id != operation.client_msg_id
        || operation
            .base
            .as_ref()
            .is_some_and(|base| wire.id != base.id)
        || !optional_matches(raw, "user_id", Some(&workspace.user_id))
        || !optional_matches(raw, "user", Some(&workspace.user_id))
        || !optional_matches(raw, "team_id", Some(&workspace.team_id))
        || !optional_matches(value, "draft_id", Some(&wire.id))
        || raw.get("is_from_composer").is_some_and(|value| {
            value.as_bool() != Some(matches!(operation.kind, OperationKind::Schedule { .. }))
        })
        || raw
            .get("date_created")
            .is_some_and(|value| epoch(value).is_none_or(|created| created < 0))
    {
        return uncertain(Issue::MalformedResponse);
    }
    // Snapshot cannot express message attachments. Preserve raw evidence and
    // stop rather than falsely acknowledging a narrowed representation.
    if raw
        .get("attachments")
        .is_some_and(|attachments| !matches!(attachments, Value::Array(items) if items.is_empty()))
    {
        return uncertain(Issue::UnsupportedResponse);
    }
    let Some(post_at) = epoch(&wire.date_scheduled) else {
        return uncertain(Issue::MalformedResponse);
    };
    let remote = RemoteDraft {
        scope: operation.scope.clone(),
        id: wire.id,
        client_msg_id: wire.client_msg_id,
        last_updated_ts: wire.last_updated_ts,
        snapshot: Snapshot {
            blocks: wire.blocks,
            file_ids: wire.file_ids,
            destinations: wire.destinations,
        },
    };
    if DraftSync::adopt_remote(&operation.scope, remote.clone()).is_err() {
        return uncertain(Issue::MalformedResponse);
    }
    match operation.kind {
        OperationKind::Schedule { post_at: expected } if post_at == expected && post_at > 0 => (
            Outcome::Scheduled {
                draft: remote,
                post_at,
            },
            None,
        ),
        OperationKind::Create | OperationKind::Update if post_at == 0 => {
            (Outcome::Draft(remote), None)
        }
        _ => uncertain(Issue::MalformedResponse),
    }
}

fn sent(
    operation: &Operation,
    workspace: &WorkspaceSession,
    value: &Value,
) -> (Outcome, Option<Issue>) {
    let Some(snapshot) = &operation.snapshot else {
        return uncertain(Issue::MalformedResponse);
    };
    let draft_id = operation.base.as_ref().map(|base| base.id.as_str());
    if !optional_matches(value, "draft_id", draft_id)
        || !optional_matches(value, "client_msg_id", Some(&operation.client_msg_id))
        || !optional_matches(value, "channel", Some(&operation.scope.key.channel))
    {
        return uncertain(Issue::MalformedResponse);
    }
    let ts = if snapshot.file_ids.is_empty() {
        if value.get("channel").and_then(Value::as_str)
            != Some(operation.scope.key.channel.as_str())
        {
            return uncertain(Issue::MalformedResponse);
        }
        let Some(message) = value.get("message").filter(|message| message.is_object()) else {
            return uncertain(Issue::MalformedResponse);
        };
        let ts = value.get("ts").and_then(Value::as_str).unwrap_or("");
        if message.get("client_msg_id").and_then(Value::as_str)
            != Some(operation.client_msg_id.as_str())
            || message.get("user").and_then(Value::as_str) != Some(workspace.user_id.as_str())
            || !optional_matches(message, "channel", Some(&operation.scope.key.channel))
            || !optional_matches(message, "ts", Some(ts))
            || match &operation.scope.key.thread {
                Some(root) => {
                    message.get("thread_ts").and_then(Value::as_str) != Some(root.as_str())
                }
                None => message
                    .get("thread_ts")
                    .is_some_and(|root| root.as_str() != Some(ts)),
            }
        {
            return uncertain(Issue::MalformedResponse);
        }
        ts
    } else {
        // Existing captured files.share proof provides file_msg_ts. Require an
        // exact file receipt too; sparse responses remain safely unconfirmed.
        let Some(files) = value.get("files").and_then(Value::as_array) else {
            return uncertain(Issue::MalformedResponse);
        };
        let actual: std::collections::BTreeSet<_> = files
            .iter()
            .filter_map(|file| file.get("id").and_then(Value::as_str))
            .collect();
        let expected: std::collections::BTreeSet<_> =
            snapshot.file_ids.iter().map(String::as_str).collect();
        if actual != expected
            || actual.len() != files.len()
            || !optional_matches(value, "user", Some(&workspace.user_id))
            || !optional_matches(value, "thread_ts", operation.scope.key.thread.as_deref())
        {
            return uncertain(Issue::MalformedResponse);
        }
        value
            .get("file_msg_ts")
            .and_then(Value::as_str)
            .unwrap_or("")
    };
    if !timestamp(ts) {
        return uncertain(Issue::MalformedResponse);
    }
    (
        Outcome::Sent {
            client_msg_id: operation.client_msg_id.clone(),
            draft_id: draft_id.map(str::to_owned),
            ts: ts.into(),
        },
        None,
    )
}
