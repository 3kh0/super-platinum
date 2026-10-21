//! Consume one confirmed-user-action intent; never replay an uncertain delete.
use super::*;
use crate::slack_lists::comments::{OwnDeleteAccepted, OwnDeleteIntent};
mod permission;
#[cfg(test)]
mod tests;

#[derive(Debug)]
pub enum ListCommentDeleteFailure {
    NotRequested(Error),
    Rejected(Value),
    Unconfirmed {
        error: Error,
        receipt: Option<Value>,
    },
}
impl Transport {
    pub async fn execute_list_comment_delete(
        &self,
        request: PreparedRequest,
        intent: OwnDeleteIntent,
        current: impl FnMut() -> bool,
    ) -> Result<OwnDeleteAccepted, ListCommentDeleteFailure> {
        if !matches_intent(&request, &intent) {
            return Err(ListCommentDeleteFailure::NotRequested(Error::Api(
                "invalid_list_comment_delete_request".into(),
            )));
        }
        match self
            .execute_list_rename_once(request, current, allowed)
            .await
        {
            Ok(raw) => settle(intent, raw),
            Err(ListColumnRenameFailure::NotRequested(error)) => {
                Err(ListCommentDeleteFailure::NotRequested(error))
            }
            Err(ListColumnRenameFailure::Unconfirmed(error)) => {
                Err(ListCommentDeleteFailure::Unconfirmed {
                    error,
                    receipt: None,
                })
            }
        }
    }
}
fn settle(
    intent: OwnDeleteIntent,
    raw: Value,
) -> Result<OwnDeleteAccepted, ListCommentDeleteFailure> {
    // Internal/fatal/unknown errors may follow a partially successful operation.
    // A missing message is a rejection receipt, not proof this request deleted it.
    if crate::slack_lists::comments::OwnDeleteRecord::rejected(&raw) {
        return Err(ListCommentDeleteFailure::Rejected(raw));
    }
    intent
        .accept(raw.clone())
        .map_err(|_| ListCommentDeleteFailure::Unconfirmed {
            error: Error::Transport("List comment delete acknowledgement did not match".into()),
            receipt: Some(raw),
        })
}
fn matches_intent(request: &PreparedRequest, intent: &OwnDeleteIntent) -> bool {
    if !allowed(request) || intent.record().validate().is_err() {
        return false;
    }
    let f = fields(request, "/api/chat.delete").unwrap();
    f["channel"] == intent.record().target().channel_id() && f["ts"] == intent.record().ts()
}
fn allowed(request: &PreparedRequest) -> bool {
    let Some(f) = fields(request, "/api/chat.delete") else {
        return false;
    };
    f.len() == 4
        && f.get("channel").is_some_and(|v| is_channel(v))
        && f.get("ts").is_some_and(|v| comments::timestamp(v))
        && f.get("_x_reason") == Some(&"animateAndDeleteMessageApi")
}
