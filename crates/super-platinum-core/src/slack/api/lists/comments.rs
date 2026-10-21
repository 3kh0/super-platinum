//! Read-only List item replies; retained cYdQ -> gOq3 -> rGEL.
use super::*;
use crate::slack_lists::comments::{ReplyQuery, Target, Tombstones};
mod own_edit;
pub use own_edit::list_record_comment_update;
mod own_delete;
pub use own_delete::{list_comment_delete_permissions, list_record_comment_delete};

pub fn list_record_comments_replies(
    c: &SlackClient,
    w: &WorkspaceSession,
    target: &Target,
    query: &ReplyQuery,
    tombstones: Tombstones,
) -> Result<PreparedRequest, Error> {
    query
        .validate(target)
        .map_err(|e| Error::Api(e.to_string()))?;
    let mut fields = vec![
        ("channel", target.channel_id().into()),
        ("ts", target.request_ts()),
        ("inclusive", "true".into()),
        ("limit", "28".into()),
        ("cached_latest_updates", "{}".into()),
        ("_x_reason", "history-api/fetchReplies".into()),
    ];
    if let Some(cursor) = &query.cursor {
        fields.push(("cursor", cursor.clone()));
    }
    if let Some(oldest) = &query.oldest {
        fields.push(("oldest", oldest.clone()));
    }
    // gOq3 removes latest when both bounds equal the requested root.
    if let Some(latest) = &query.latest {
        if !(Some(latest.as_str()) == target.root_ts() && query.oldest == query.latest) {
            fields.push(("latest", latest.clone()));
        }
    }
    if tombstones == Tombstones::Include {
        fields.push(("include_tombstones", "true".into()));
    }
    Ok(c.rest_form(w, "conversations.replies", fields))
}

/// k+EA -> ro6C -> gzYT normal rich send. Optional undefined arguments are
/// omitted by qVKj; text fallback is not sent alongside blocks.
pub fn list_record_comment_reply(
    c: &SlackClient,
    w: &WorkspaceSession,
    intent: &crate::slack_lists::comments::ReplyIntent,
) -> Result<PreparedRequest, Error> {
    let r = intent.record();
    r.validate().map_err(|e| Error::Api(e.to_string()))?;
    if r.workspace() != w.team_id || r.user() != w.user_id {
        return Err(Error::Api("list_comment_owner_changed".into()));
    }
    Ok(c.rest_form(
        w,
        "chat.postMessage",
        vec![
            ("channel", r.target().channel_id().into()),
            ("ts", r.client_ts().into()),
            ("type", "message".into()),
            ("reply_broadcast", "false".into()),
            ("thread_ts", r.target().root_ts().unwrap().into()),
            ("include_channel_perm_error", "true".into()),
            ("client_msg_id", r.client_msg_id().into()),
            (
                "blocks",
                serde_json::to_string(r.blocks())
                    .map_err(|_| Error::Api("invalid_list_comment_blocks".into()))?,
            ),
            ("_x_reason", "webapp_message_send".into()),
        ],
    ))
}

#[cfg(test)]
mod tests;
