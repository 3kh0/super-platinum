//! Existing-root comment POST only. No retry, read cancellation, or late-owner
//! projection: the original caller must retain the response as an inert receipt.
use super::*;
use crate::slack_lists::comments::{ReplyAccepted, ReplyIntent};

#[derive(Debug)]
pub enum ListCommentReplyFailure {
    NotRequested(Error),
    Rejected(Value),
    Unconfirmed {
        error: Error,
        receipt: Option<Value>,
    },
}
impl Transport {
    pub async fn execute_list_comment_reply(
        &self,
        request: PreparedRequest,
        intent: ReplyIntent,
        current: impl FnMut() -> bool,
    ) -> Result<ReplyAccepted, ListCommentReplyFailure> {
        if !matches_intent(&request, &intent) {
            return Err(ListCommentReplyFailure::NotRequested(Error::Api(
                "invalid_list_comment_request".into(),
            )));
        }
        // The shared single-dispatch executor checks health/owner before HTTP,
        // has bounded response/body and no redirect/retry. It deliberately does
        // not discard an already-issued response after the owner is retired.
        let result = self
            .execute_list_rename_once(request, current, allowed)
            .await;
        match result {
            Ok(raw) => settle(intent, raw),
            Err(ListColumnRenameFailure::NotRequested(e)) => {
                Err(ListCommentReplyFailure::NotRequested(e))
            }
            Err(ListColumnRenameFailure::Unconfirmed(e)) => {
                Err(ListCommentReplyFailure::Unconfirmed {
                    error: e,
                    receipt: None,
                })
            }
        }
    }
}
fn settle(intent: ReplyIntent, raw: Value) -> Result<ReplyAccepted, ListCommentReplyFailure> {
    if raw["ok"] == false && raw["error"].as_str().is_some_and(|e| !e.is_empty()) {
        return Err(ListCommentReplyFailure::Rejected(raw));
    }
    intent
        .accept(raw.clone())
        .map_err(|_| ListCommentReplyFailure::Unconfirmed {
            error: Error::Transport("List comment acknowledgement did not match".into()),
            receipt: Some(raw),
        })
}
fn matches_intent(request: &PreparedRequest, intent: &ReplyIntent) -> bool {
    if !allowed(request) || intent.record().validate().is_err() {
        return false;
    }
    let Some(f) = fields(request, "/api/chat.postMessage") else {
        return false;
    };
    let r = intent.record();
    f.get("channel") == Some(&r.target().channel_id())
        && f.get("thread_ts") == r.target().root_ts().as_ref()
        && f.get("ts") == Some(&r.client_ts())
        && f.get("client_msg_id") == Some(&r.client_msg_id())
        && serde_json::to_string(r.blocks()).is_ok_and(|b| f.get("blocks") == Some(&b.as_str()))
}
fn allowed(request: &PreparedRequest) -> bool {
    let Some(f) = fields(request, "/api/chat.postMessage") else {
        return false;
    };
    let keys = [
        "token",
        "channel",
        "ts",
        "type",
        "reply_broadcast",
        "thread_ts",
        "include_channel_perm_error",
        "client_msg_id",
        "blocks",
        "_x_reason",
    ];
    if f.len() != keys.len()
        || keys.iter().any(|k| !f.contains_key(k))
        || f.get("channel").is_none_or(|c| !is_channel(c))
        || ["ts", "thread_ts"]
            .iter()
            .any(|k| f.get(k).is_none_or(|ts| !comments::timestamp(ts)))
        || f.get("type") != Some(&"message")
        || f.get("reply_broadcast") != Some(&"false")
        || f.get("include_channel_perm_error") != Some(&"true")
        || f.get("_x_reason") != Some(&"webapp_message_send")
        || f.get("client_msg_id")
            .is_none_or(|v| uuid::Uuid::parse_str(v).is_err())
    {
        return false;
    }
    let encoded = f["blocks"];
    let Ok(blocks) = serde_json::from_str::<Vec<Value>>(encoded) else {
        return false;
    };
    // Canonical serializer equality also refuses duplicate hidden object keys.
    serde_json::to_string(&blocks).is_ok_and(|s| s == encoded)
        && crate::canvas::comments::content::fallback(&blocks).is_ok_and(|s| !s.trim().is_empty())
}

#[cfg(test)]
mod tests;
