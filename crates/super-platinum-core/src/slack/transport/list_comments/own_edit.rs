//! One original rich chat.update dispatch. Unlike source 4Tc8, there is no
//! message_not_found retry: an uncertain write remains an inert receipt.
use super::*;
use crate::slack_lists::comments::{OwnEditAccepted, OwnEditIntent};

#[derive(Debug)]
pub enum ListCommentUpdateFailure {
    NotRequested(Error),
    Rejected(Value),
    Unconfirmed {
        error: Error,
        receipt: Option<Value>,
    },
}
impl Transport {
    pub async fn execute_list_comment_update(
        &self,
        request: PreparedRequest,
        intent: OwnEditIntent,
        current: impl FnMut() -> bool,
    ) -> Result<OwnEditAccepted, ListCommentUpdateFailure> {
        if !matches_intent(&request, &intent) {
            return Err(ListCommentUpdateFailure::NotRequested(Error::Api(
                "invalid_list_comment_edit_request".into(),
            )));
        }
        // Checks original ownership immediately before actual dispatch, never
        // drops a possibly accepted response merely because the visit changed.
        match self
            .execute_list_rename_once(request, current, allowed)
            .await
        {
            Ok(raw) => settle(intent, raw),
            Err(ListColumnRenameFailure::NotRequested(error)) => {
                Err(ListCommentUpdateFailure::NotRequested(error))
            }
            Err(ListColumnRenameFailure::Unconfirmed(error)) => {
                Err(ListCommentUpdateFailure::Unconfirmed {
                    error,
                    receipt: None,
                })
            }
        }
    }
}
fn settle(intent: OwnEditIntent, raw: Value) -> Result<OwnEditAccepted, ListCommentUpdateFailure> {
    // A bounded rejection list; internal/fatal/server/unknown errors may have
    // applied. DLP warnings never trigger a second skip-warning request here.
    if raw["ok"] == false
        && matches!(
            raw["error"].as_str(),
            Some(
                "cant_update_message"
                    | "edit_window_closed"
                    | "message_not_found"
                    | "is_inactive"
                    | "not_authed"
                    | "invalid_auth"
                    | "token_revoked"
                    | "account_inactive"
                    | "missing_scope"
                    | "no_permission"
                    | "ratelimited"
            )
        )
    {
        return Err(ListCommentUpdateFailure::Rejected(raw));
    }
    intent
        .accept(raw.clone())
        .map_err(|_| ListCommentUpdateFailure::Unconfirmed {
            error: Error::Transport("List comment edit acknowledgement did not match".into()),
            receipt: Some(raw),
        })
}
fn matches_intent(request: &PreparedRequest, intent: &OwnEditIntent) -> bool {
    if !allowed(request) || intent.record().validate().is_err() {
        return false;
    }
    let Some(fields) = fields(request, "/api/chat.update") else {
        return false;
    };
    let record = intent.record();
    fields.get("channel") == Some(&record.target().channel_id())
        && fields.get("ts") == Some(&record.ts())
        && serde_json::to_string(record.blocks())
            .is_ok_and(|blocks| fields.get("blocks") == Some(&blocks.as_str()))
}
fn allowed(request: &PreparedRequest) -> bool {
    let Some(fields) = fields(request, "/api/chat.update") else {
        return false;
    };
    let keys = [
        "token",
        "channel",
        "ts",
        "blocks",
        "skip_dlp_user_warning",
        "_x_reason",
    ];
    if fields.len() != keys.len()
        || keys.iter().any(|k| !fields.contains_key(k))
        || fields.get("channel").is_none_or(|c| !is_channel(c))
        || fields.get("ts").is_none_or(|ts| !comments::timestamp(ts))
        || fields.get("skip_dlp_user_warning") != Some(&"false")
        || fields.get("_x_reason") != Some(&"saveMessageEdit")
    {
        return false;
    }
    let encoded = fields["blocks"];
    if encoded.len() > crate::canvas::comments::content::MAX_BLOCK_BYTES {
        return false;
    }
    let Ok(blocks) = serde_json::from_str::<Vec<Value>>(encoded) else {
        return false;
    };
    let split = blocks
        .iter()
        .position(|b| b["type"] != "rich_text")
        .unwrap_or(blocks.len());
    blocks.len() <= 50
        && blocks[split..].iter().all(|b| b["type"] == "context")
        && crate::canvas::comments::content::fallback(&blocks[..split])
            .is_ok_and(|text| !text.trim().is_empty())
        && serde_json::to_string(&blocks).is_ok_and(|canonical| canonical == encoded)
}
#[cfg(test)]
mod tests;
