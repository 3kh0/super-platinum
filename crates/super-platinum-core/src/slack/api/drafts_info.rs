//! Positive readback of one existing server draft; no ownership or mutation authority.
//!
//! Retained public client evidence (decoded JavaScript character offsets):
//! B UYdH 6816000, call 6818245: zZt+.oV({draftId, reason}).
//! V zZt+ 9756496 binds drafts.info. S uGgu 1594178 strips reason/timeout/
//! abortSignal and applies LYxw.VA (738282), yielding draft_id only.
//! B/V/S filenames and SHA-256 values are recorded in the external-select
//! protocol source table. This module is deliberately unreferenced until gated.
use super::{Error, PreparedRequest, SlackClient};
use crate::config::WorkspaceSession;
use serde_json::Value;

fn valid_id(id: &str) -> bool {
    // A bounded native identifier policy, not a claimed server length limit.
    !id.is_empty()
        && id.len() <= 255
        && !id
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == ',')
}

/// The captured authenticated account/transport and durable owner must be
/// checked by the caller. No destination, local UUID, or mutation timestamp is
/// inferred. `reason` is client instrumentation, not an API argument.
///
/// The existing standard Transport::execute whitelist does not include
/// drafts.info, so this POST currently makes one attempt (including on 429).
/// This builder does not broaden that global retry policy or execute a request.
pub fn drafts_info(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    draft_id: &str,
) -> Result<PreparedRequest, Error> {
    if !valid_id(draft_id) {
        return Err(Error::Api("draft_info_identity_invalid".into()));
    }
    Ok(client.rest_form(
        workspace,
        "drafts.info",
        vec![("draft_id", draft_id.into())],
    ))
}

/// Only a positive envelope with the requested server ID. All fields, including
/// unknown lifecycle/routing/content extensions, remain available for stricter
/// downstream validation. This is not a RemoteDraft or a reconciliation receipt.
#[derive(Clone, Debug, PartialEq)]
pub struct DraftInfo {
    raw: Value,
}
impl DraftInfo {
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    pub fn draft(&self) -> &Value {
        &self.raw["draft"]
    }
    pub fn id(&self) -> &str {
        self.draft()["id"].as_str().expect("validated draft id")
    }
}

/// A missing record, API error, malformed result, or mismatched ID is unresolved.
/// No error code or list absence is converted to deletion or send confirmation.
/// Even is_deleted/is_sent in a positive object are raw evidence only here.
pub fn decode_drafts_info(expected_id: &str, raw: Value) -> Result<DraftInfo, Error> {
    if !valid_id(expected_id)
        || raw.get("ok").and_then(Value::as_bool) != Some(true)
        || raw.get("error").is_some_and(|error| !error.is_null())
        || !raw.get("draft").is_some_and(Value::is_object)
        || raw["draft"].get("id").and_then(Value::as_str) != Some(expected_id)
    {
        return Err(Error::Api("draft_info_unresolved".into()));
    }
    Ok(DraftInfo { raw })
}

#[cfg(test)]
mod tests;
