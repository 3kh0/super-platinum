//! One explicit ordinary own-reply deletion, with separate HTTP and current body
//! custody. The host owns visible occurrence, account/visit and read lifetimes.
//! Retained pLOx/M3rm policy; strict ACK/readback proofs are native refinements.
use super::{
    ListError, Message, OwnEditSeed, Page, Target, identity, item_root, parse_message, timestamp,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

mod policy;
pub use policy::{OwnDeletePermission, OwnDeletePermissionQuery, OwnDeletePolicy};
mod source;

/// Sendable authority is consumed once. An inert record cannot recreate it.
pub struct OwnDeleteIntent {
    record: OwnDeleteRecord,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnDeleteRecord {
    version: u8,
    workspace: String,
    user: String,
    target: Target,
    ts: String,
    http: Value,
    message: Value,
}

pub struct OwnDeleteAccepted {
    record: OwnDeleteRecord,
    raw: Value,
}

impl OwnDeleteIntent {
    /// Candidate presentation only, never current permission or send authority.
    pub fn candidate(page: &Page, known: &Message, user: &str) -> bool {
        source::root_available(page).is_ok()
            && source::ordinary(known, page.target(), user).is_ok()
            && page
                .messages()
                .iter()
                .any(|m| m.ts == known.ts && OwnEditSeed::same_body(page.target(), m, known))
    }
    pub fn capture(
        workspace: &str,
        user: &str,
        page: &Page,
        known: &Message,
        policy: &OwnDeletePolicy,
    ) -> Result<Self, ListError> {
        let ts = known
            .ts
            .as_deref()
            .filter(|v| timestamp(v))
            .ok_or(ListError("The original comment is unavailable."))?;
        policy.admit(workspace, user, page.target())?;
        source::root_available(page)?;
        source::ordinary(known, page.target(), user)?;
        let loaded = page
            .messages()
            .iter()
            .find(|m| m.ts.as_deref() == Some(ts))
            .ok_or(ListError("The original comment is no longer loaded."))?;
        if !OwnEditSeed::same_body(page.target(), loaded, known) {
            return Err(ListError("The selected comment changed."));
        }
        let record = OwnDeleteRecord {
            version: 1,
            workspace: workspace.into(),
            user: user.into(),
            target: page.target().clone(),
            ts: ts.into(),
            http: page.raw().clone(),
            message: serde_json::to_value(known)
                .map_err(|_| ListError("The comment is malformed."))?,
        };
        record.validate()?;
        Ok(Self { record })
    }
    pub fn record(&self) -> &OwnDeleteRecord {
        &self.record
    }
    /// Requires a newly acquired HTTP body matching the selected current body.
    /// Capturing a newer typed body beside older HTTP provenance does not make
    /// that old HTTP receipt sufficient for dispatch.
    pub fn revalidate(
        &self,
        page: &Page,
        known: &Message,
        policy: &OwnDeletePolicy,
    ) -> Result<(), ListError> {
        self.record.revalidate(page, known, policy)
    }
    /// The primary API documents channel/ts on success. Missing identity is not
    /// a confirmed result, even when a caller supplies success-shaped JSON.
    pub fn accept(self, raw: Value) -> Result<OwnDeleteAccepted, ListError> {
        self.record.validate_ack(&raw)?;
        Ok(OwnDeleteAccepted {
            record: self.record,
            raw,
        })
    }
}

impl OwnDeleteRecord {
    pub fn validate(&self) -> Result<(), ListError> {
        if self.version != 1
            || !identity(&self.workspace)
            || !identity(&self.user)
            || !timestamp(&self.ts)
            || serde_json::to_vec(self)
                .map_err(|_| ListError("The delete record is malformed."))?
                .len()
                > 8 * 1024 * 1024
        {
            return Err(ListError("The original delete identity is malformed."));
        }
        let page = Page::parse(&self.target, self.http.clone())?;
        if page.target() != &self.target || source::deleted(&self.http, &self.ts) {
            return Err(ListError("The original comment receipt is unavailable."));
        }
        source::root_available(&page)?;
        let original = parse_message(
            source::original(&self.http, &self.ts)?,
            self.target.channel_id(),
        )?;
        source::ordinary(&original, &self.target, &self.user)?;
        let current = parse_message(&self.message, self.target.channel_id())?;
        source::ordinary(&current, &self.target, &self.user)?;
        if current.ts.as_deref() != Some(self.ts()) {
            return Err(ListError("The captured comment identity changed."));
        }
        Ok(())
    }
    /// Reactions and counters do not change the selected body. This is not a
    /// visibility, owner-lifetime or permission observation.
    pub fn current(&self, known: &Message) -> bool {
        source::ordinary(known, &self.target, &self.user).is_ok()
            && parse_message(&self.message, self.target.channel_id())
                .is_ok_and(|captured| OwnEditSeed::same_body(&self.target, &captured, known))
    }
    /// Fresh raw and typed rows must both match the captured current body. The
    /// older original HTTP receipt stays immutable and cannot satisfy this
    /// preflight merely because its message identity still matches.
    pub fn revalidate(
        &self,
        page: &Page,
        known: &Message,
        policy: &OwnDeletePolicy,
    ) -> Result<(), ListError> {
        self.validate()?;
        if page.target() != &self.target
            || !self.current(known)
            || source::deleted(page.raw(), &self.ts)
        {
            return Err(ListError("The original comment changed before deletion."));
        }
        source::root_available(page)?;
        let raw = parse_message(
            source::original(page.raw(), &self.ts)?,
            self.target.channel_id(),
        )?;
        if !self.current(&raw)
            || !page
                .messages()
                .iter()
                .any(|m| m.ts.as_deref() == Some(self.ts()) && self.current(m))
        {
            return Err(ListError(
                "The fresh comment body no longer matches the selected comment.",
            ));
        }
        policy.admit(&self.workspace, &self.user, &self.target)
    }
    /// Exact scoped deletion evidence, never absence from a partial or complete
    /// page. The original ACK is neither constructed nor changed by readback.
    pub fn observed(&self, page: &Page) -> Result<bool, ListError> {
        self.validate()?;
        if page.target() != &self.target {
            return Err(ListError(
                "Deletion observation belongs to another item or root.",
            ));
        }
        source::root_available(page)?;
        let Some(raw) = page.raw()["messages"]
            .as_array()
            .and_then(|a| a.iter().find(|r| r["ts"].as_str() == Some(self.ts())))
        else {
            return Ok(source::deleted(page.raw(), &self.ts));
        };
        // Page validates the thread/channel. A tombstone may omit its author,
        // but an explicitly different author cannot prove this deletion. A
        // deleted[] entry cannot override a contradictory live or foreign row.
        Ok(raw["thread_ts"].as_str() == self.target.root_ts()
            && matches!(
                raw.get("type").and_then(Value::as_str),
                None | Some("message")
            )
            && raw
                .get("user")
                .is_none_or(|v| v.as_str() == Some(self.user()))
            && (raw["subtype"] == "tombstone"
                || raw["is_deleted"] == true
                || raw["is_tombstoned"] == true))
    }
    pub fn validate_ack(&self, raw: &Value) -> Result<(), ListError> {
        self.validate()?;
        if raw["ok"] != true
            || raw.get("error").is_some_and(|v| !v.is_null())
            || raw["channel"].as_str() != Some(self.target.channel_id())
            || raw["ts"].as_str() != Some(self.ts())
        {
            return Err(ListError(
                "The deletion acknowledgement did not match the original comment.",
            ));
        }
        Ok(())
    }
    /// Bounded known rejection receipts shared by transport and local recovery.
    /// Missing-message is rejection, not proof that this request deleted it.
    pub fn rejected(raw: &Value) -> bool {
        raw["ok"] == false
            && matches!(
                raw["error"].as_str(),
                Some(
                    "cant_delete_message"
                        | "message_not_found"
                        | "channel_not_found"
                        | "not_authed"
                        | "invalid_auth"
                        | "token_revoked"
                        | "account_inactive"
                        | "missing_scope"
                        | "no_permission"
                        | "ratelimited"
                )
            )
    }
    pub fn workspace(&self) -> &str {
        &self.workspace
    }
    pub fn user(&self) -> &str {
        &self.user
    }
    pub fn target(&self) -> &Target {
        &self.target
    }
    pub fn ts(&self) -> &str {
        &self.ts
    }
    pub fn http_raw(&self) -> &Value {
        &self.http
    }
    pub fn original_raw(&self) -> &Value {
        source::original(&self.http, &self.ts).unwrap_or(&Value::Null)
    }
    pub fn message_raw(&self) -> &Value {
        &self.message
    }
}
impl OwnDeleteAccepted {
    pub fn record(&self) -> &OwnDeleteRecord {
        &self.record
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
}

#[cfg(test)]
mod tests;
