//! p4nx -> Xhlw -> gzYT, bounded to the original plain Workflow Share.
//! Failed source row metadata survives into Xhlw's wire W; its replacement F
//! alone resets showResend/showCancel. Native display text is not source input.
use super::*;

pub const SUPPRESSED: &[&str] = &[
    "restricted_action_read_only_channel",
    "restricted_action_thread_only_channel",
    "restricted_action_non_threadable_channel",
    "ekm_access_denied",
    "message_limit_exceeded",
    "slack_connect_file_link_sharing_blocked",
    "slack_connect_canvas_contain_file_sharing_blocked",
    "slack_connect_list_contain_file_sharing_blocked",
    "slack_connect_canvas_sharing_blocked",
    "slack_connect_file_upload_sharing_blocked",
    "slack_connect_team_blocked",
    "dlp_user_warning",
];

/// Only a policy reduction; caller must prove the original unresolved row,
/// account, finished failure and no concurrent retry/cancellation.
pub fn eligible(failure: Option<&Value>) -> bool {
    let Some(raw) = failure else { return true };
    if raw.get("ok") == Some(&json!(true)) {
        return false;
    }
    if raw.get("dlp_custom_message").is_some_and(|v| !v.is_null()) {
        return false; // separate DLP renderer/editor is not this plain path
    }
    match raw.get("error") {
        Some(Value::Number(n)) => !matches!(n.as_f64(), Some(6.0 | 7.0 | 8.0)),
        Some(Value::String(code)) => !SUPPRESSED.contains(&code.as_str()),
        Some(Value::Null) | None => true,
        _ => false,
    }
}

impl Message {
    pub fn resend_request(
        &self,
        client: &SlackClient,
        session: &WorkspaceSession,
        failure: Option<&Value>,
    ) -> Result<Request, Error> {
        if !eligible(failure) {
            return Err(invalid());
        }
        // Original factory still owns credential/domain validation.
        self.request(client, session)?;
        Ok(Request {
            request: client.avatar_multipart(session, METHOD, fields(self, &session.user_id)),
            message: self.clone(),
            resend_user: Some(session.user_id.clone()),
        })
    }
}

pub(super) fn fields(message: &Message, user: &str) -> Vec<(&'static str, String)> {
    let mut fields = message.fields();
    fields.extend([
        ("user", user.to_owned()),
        ("showResend", "true".into()),
        ("showCancel", "true".into()),
        ("is_pending", "true".into()),
        ("isEKMError", "false".into()),
        ("isDLPUserWarningError", "false".into()),
        ("skip_dlp_user_warning", "false".into()),
    ]);
    fields
}

#[cfg(test)]
mod tests;
