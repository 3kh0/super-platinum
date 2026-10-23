//! Ordinary plain Workflow Share content, not a destination permission grant.
//! Retained ShareWorkflowModalPane/gzYT append an actual trigger URL to the
//! local delta and send rich blocks plus explicit unfurl metadata. This bounded
//! plain path does not implement state-dependent editor preprocessing.
use crate::{
    config::WorkspaceSession,
    slack::{Error, PreparedRequest, SlackClient, client::RequestBody},
};
use serde_json::{Value, json};

mod destination;
mod dm;
pub mod group;
pub mod person;
pub mod posting_refresh;
mod receipt;
pub mod resend;
pub mod search;
pub use destination::{joined_channel_candidate, joined_channel_eligible};
pub use dm::existing_dm_eligible;
pub use receipt::{Attempt, Correlation, Outcome, Receipt, acknowledged};
#[cfg(test)]
mod tests;

pub const METHOD: &str = "chat.postMessage";
pub const REASON: &str = "webapp_message_send";
const MAX_TEXT: usize = 256 * 1024;

/// Immutable content/identity. The host separately owns credentials, original
/// trigger custody and actual canonical destination permission.
#[derive(Clone, PartialEq)]
pub struct Message {
    trigger: Value,
    url: String,
    blocks: Value,
    text: String,
    channel: String,
    client_msg_id: String,
    local_ts: String,
}
impl Message {
    pub fn new(
        trigger: &Value,
        text: &str,
        channel: &str,
        client_msg_id: &str,
        local_ts: &str,
    ) -> Result<Self, Error> {
        if !trigger.is_object()
            || text.len() > MAX_TEXT
            || !destination(channel)
            || !crate::slack_lists::comments::timestamp(local_ts)
            || !uuid::Uuid::parse_str(client_msg_id)
                .is_ok_and(|id| !id.is_nil() && id.to_string() == client_msg_id)
        {
            return Err(invalid());
        }
        // JS truthy precedence: a malformed truthy first value must not silently
        // select the fallback. Missing/null/false/zero/empty string are false.
        let selected = trigger
            .get("share_url")
            .filter(|v| truthy(v))
            .or_else(|| trigger.get("shortcut_url").filter(|v| truthy(v)));
        let selected = match selected {
            Some(value) => value.as_str().ok_or_else(invalid)?,
            None => "",
        };
        let text = text.trim_start_matches(js_whitespace);
        let plain = if selected.is_empty() {
            text.trim_end_matches(js_whitespace).to_owned()
        } else if text.is_empty() {
            String::new()
        } else {
            format!("{text}\n")
        };
        let plain = strip_before_newlines(&plain);
        let mut elements = Vec::new();
        if !plain.is_empty() {
            elements.push(json!({"type":"text","text":plain}));
        }
        if !selected.is_empty() {
            elements.push(json!({"type":"link","url":selected}));
        }
        // JQxa convertEmpty=false returns no blocks for the empty delta.
        let blocks = if elements.is_empty() {
            json!([])
        } else {
            json!([{"type":"rich_text","elements":[{"type":"rich_text_section","elements":elements}]}])
        };
        let fallback = format!("{plain}{selected}");
        Ok(Self {
            trigger: trigger.clone(),
            url: selected.into(),
            blocks,
            text: fallback,
            channel: channel.into(),
            client_msg_id: client_msg_id.into(),
            local_ts: local_ts.into(),
        })
    }
    pub fn trigger(&self) -> &Value {
        &self.trigger
    }
    pub fn blocks(&self) -> &Value {
        &self.blocks
    }
    pub fn url(&self) -> &str {
        &self.url
    }
    /// Local display fallback only. Never included as a top-level wire field.
    pub fn text_fallback(&self) -> &str {
        &self.text
    }
    pub fn channel(&self) -> &str {
        &self.channel
    }
    pub fn client_msg_id(&self) -> &str {
        &self.client_msg_id
    }
    pub fn local_ts(&self) -> &str {
        &self.local_ts
    }
    pub fn request(
        &self,
        client: &SlackClient,
        session: &WorkspaceSession,
    ) -> Result<Request, Error> {
        if session.token.trim().is_empty()
            || session.team_id.is_empty()
            || session.user_id.is_empty()
        {
            return Err(invalid());
        }
        Ok(Request {
            request: client.avatar_multipart(session, METHOD, self.fields()),
            message: self.clone(),
            resend_user: None,
        })
    }
    fn fields(&self) -> Vec<(&'static str, String)> {
        let mut fields = vec![
            ("channel", self.channel.clone()),
            ("ts", self.local_ts.clone()),
            ("type", "message".into()),
        ];
        if self.blocks.as_array().is_some_and(|b| !b.is_empty()) {
            fields.push(("blocks", self.blocks.to_string()));
        }
        fields.extend([
            ("unfurl", json!([{"url":self.url}]).to_string()),
            ("client_msg_id", self.client_msg_id.clone()),
            ("include_channel_perm_error", "true".into()),
            ("_x_reason", REASON.into()),
        ]);
        fields
    }
}

/// Token-first Form fields are prepared for the dedicated multipart executor;
/// passing these to a generic URL-encoded Form executor is not source parity.
#[derive(Clone)]
pub struct Request {
    pub(crate) request: PreparedRequest,
    message: Message,
    resend_user: Option<String>,
}
impl Request {
    pub fn prepared(&self) -> &PreparedRequest {
        &self.request
    }
    pub fn message(&self) -> &Message {
        &self.message
    }
    pub fn inspect(&self) -> Value {
        let RequestBody::Form(fields) = &self.request.body else {
            unreachable!("closed factory")
        };
        json!({"method":METHOD,"url":self.request.url,"fields":fields.iter().filter(|(k,_)|k!="token").collect::<Vec<_>>(),
            "tokenPresent":fields.first().is_some_and(|(k,v)|k=="token"&&!v.is_empty())})
    }
    fn fields(&self) -> Vec<(&'static str, String)> {
        match &self.resend_user {
            Some(user) => resend::fields(&self.message, user),
            None => self.message.fields(),
        }
    }
    pub(crate) fn fields_current(&self) -> bool {
        let RequestBody::Form(fields) = &self.request.body else {
            return false;
        };
        fields
            .first()
            .is_some_and(|(k, v)| k == "token" && !v.trim().is_empty())
            && fields.len() == self.fields().len() + 1
            && fields
                .iter()
                .skip(1)
                .zip(self.fields())
                .all(|((k, v), (ek, ev))| k == ek && v == &ev)
    }
}
impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkflowShareRequest")
            .field("method", &METHOD)
            .finish_non_exhaustive()
    }
}
fn destination(s: &str) -> bool {
    (2..=64).contains(&s.len())
        && matches!(s.as_bytes().first(), Some(b'C' | b'G' | b'D' | b'Z'))
        && s.bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(n) => n.as_f64() != Some(0.),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}
fn js_whitespace(c: char) -> bool {
    matches!(c,'\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{00a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}')
}
fn invalid() -> Error {
    Error::Api("workflow_share_invalid_input".into())
}

// Exact ordinary JQxa /(^|\S)( +)(\n)/gm replacement. Only ASCII spaces
// immediately before LF with start/non-whitespace predecessor are removed.
fn strip_before_newlines(text: &str) -> String {
    let mut out = String::new();
    for part in text.split_inclusive('\n') {
        if let Some(line) = part.strip_suffix('\n') {
            let trimmed = line.trim_end_matches(' ');
            let previous = trimmed.chars().next_back();
            let eligible = previous.is_none_or(|c| {
                !js_whitespace(c) || matches!(c, '\r' | '\n' | '\u{2028}' | '\u{2029}')
            });
            out.push_str(if eligible { trimmed } else { line });
            out.push('\n');
        } else {
            out.push_str(part);
        }
    }
    out
}
