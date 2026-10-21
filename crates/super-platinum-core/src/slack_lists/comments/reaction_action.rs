//! Existing exact-name List reply reaction actions. Retained FaST/ILc8/jGtm.
//!
//! Unlike the web client's offline-persisted optimistic thunk, this native
//! boundary consumes one intent and distinguishes ACK from desired-state proof.
//! The host owns observed visibility, original account/visit and dispatch guards.
use super::{ListError, Message, Page, Target, identity, item_root, timestamp};
use crate::composer_emoji::catalog::ReactionChoice;
use crate::slack::models::Reaction;
use serde_json::Value;
use std::collections::HashSet;

mod family;
pub use family::{
    ReactionFamilyIntent, ReactionFamilyRecord, own_reaction_family_names, reaction_family_base,
};

pub struct ReactionIntent {
    record: ReactionRecord,
}

/// Inert captured facts, never convertible back into a sendable intent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReactionRecord {
    target: Target,
    root: Value,
    message: Value,
    channel: Value,
    user: String,
    name: String,
    ts: String,
    adding: bool,
    family_selected: Option<String>,
    choice: Option<ReactionChoice>,
}

pub struct ReactionAccepted {
    record: ReactionRecord,
    raw: Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReactionResponse {
    /// A positive response under the original consumed request identity.
    Confirmed,
    /// Current desired state, not proof of which request produced it.
    DesiredState,
    /// Explicit, bounded documented rejection; not every ok:false is safe.
    Rejected,
    Uncertain,
}

impl ReactionIntent {
    /// Capture an existing pill using CURRENT typed self membership.
    ///
    /// The caller must have selected a visibly projected row under this target.
    /// Missing channel on an acquired HTTP row is allowed; explicit foreign
    /// channels are not. No archive/access/can_thread posting policy is inferred.
    pub fn capture(
        target: &Target,
        root: &Message,
        message: &Message,
        channel: Value,
        user: &str,
        name: &str,
    ) -> Result<Self, ListError> {
        Self::capture_inner(target, root, message, channel, user, name, None, None)
    }

    /// Exact final-name picker toggle or captured no-own pill addition, never
    /// a parsed-base family removal. Pill provenance cannot turn into a remove.
    /// The host must revalidate choice.current against its current immutable
    /// catalog and complete preferences before dispatch, under original owner.
    pub fn capture_choice(
        target: &Target,
        root: &Message,
        message: &Message,
        channel: Value,
        user: &str,
        choice: ReactionChoice,
    ) -> Result<Self, ListError> {
        if !choice.matches_reactions(&message.reactions) || !choice.accepts_actor(user) {
            return Err(ListError("The original reaction choices have changed."));
        }
        let name = choice.name().to_owned();
        Self::capture_inner(
            target,
            root,
            message,
            channel,
            user,
            &name,
            None,
            Some(choice),
        )
    }

    fn capture_inner(
        target: &Target,
        root: &Message,
        message: &Message,
        channel: Value,
        user: &str,
        name: &str,
        family_selected: Option<&str>,
        choice: Option<ReactionChoice>,
    ) -> Result<Self, ListError> {
        target.validate()?;
        let ts = message.ts.as_deref().filter(|ts| timestamp(ts));
        if !identity(user)
            || !reaction_name(name)
            || !item_root(root, target)
            || root.ts.as_deref() != target.root_ts()
            || target.root_ts().is_none()
            || root.channel.as_deref().is_some_and(|c| c != target.channel_id())
            || message.channel.as_deref().is_some_and(|c| c != target.channel_id())
            || ts.is_none()
            || ts == target.root_ts()
            || message.thread_ts.as_deref() != target.root_ts()
            || channel["id"].as_str() != Some(target.channel_id())
            || truthy(channel.get("is_frozen"))
            || (truthy(channel.get("is_pending_ext_shared")) && truthy(channel.get("is_im")))
            || truthy(root.extra.get("is_locked"))
            || !available(root)
            || !available(message)
            || !matches!(message.subtype.as_deref(), None | Some("bot_message" | "me_message" | "thread_broadcast"))
            || target.document_raw()["filetype"] == "thread_share"
            || target.document_raw()["mimetype"] == "application/vnd.slack-thread-share"
            // The FaST Canvas access/experiment branch is outside ordinary List.
            || target.document_raw()["mode"] == "quip"
        {
            return Err(ListError("This List comment reaction is unavailable."));
        }
        let mut names = HashSet::new();
        let mut own_count = 0;
        let mut selected = None;
        for reaction in &message.reactions {
            let users: HashSet<_> = reaction.users.iter().map(String::as_str).collect();
            if !reaction_name(&reaction.name)
                || !names.insert(&reaction.name)
                || users.len() != reaction.users.len()
                || reaction.users.iter().any(|u| !identity(u))
                || reaction.count < users.len() as u32
            {
                return Err(ListError("List comment reaction membership is ambiguous."));
            }
            // Web reactionToggleButton can remove several parsed skin-tone
            // variants at once. This one-request native scope must not turn
            // that into a different add or an incomplete family removal.
            if family_selected.is_none()
                && choice.is_none()
                && (name.contains(":skin-tone-")
                    || reaction.name.starts_with(&format!("{name}::skin-tone-")))
            {
                return Err(ListError(
                    "Reaction skin-tone family actions are unavailable.",
                ));
            }
            let mine = users.contains(user);
            own_count += usize::from(mine);
            if reaction.name == name {
                selected = Some(mine);
            }
        }
        let mine = if choice.is_some() {
            selected.unwrap_or(false)
        } else {
            selected.ok_or(ListError("The original reaction pill is unavailable."))?
        };
        if family_selected.is_some() && !mine {
            return Err(ListError("The original own reaction is unavailable."));
        }
        if mine && choice.as_ref().is_some_and(ReactionChoice::addition_only) {
            return Err(ListError("The original reaction family is already owned."));
        }
        if !mine
            && choice
                .as_ref()
                .is_some_and(|choice| !choice.addition_allowed())
        {
            return Err(ListError("The selected reaction is unavailable."));
        }
        // Existing-name additions do not consume a new distinct emoji slot.
        if !mine && own_count >= 23 {
            return Err(ListError("The reaction limit has been reached."));
        }
        if choice.is_some() && selected.is_none() && message.reactions.len() >= 50 {
            return Err(ListError("The distinct reaction limit has been reached."));
        }
        Ok(Self {
            record: ReactionRecord {
                target: target.clone(),
                root: raw(root)?,
                message: raw(message)?,
                channel,
                user: user.into(),
                name: name.into(),
                ts: ts.unwrap().into(),
                adding: !mine,
                family_selected: family_selected.map(str::to_owned),
                choice,
            },
        })
    }

    pub fn record(&self) -> &ReactionRecord {
        &self.record
    }

    pub fn accept(self, raw: Value) -> Result<ReactionAccepted, ListError> {
        if self.record.classify(&raw) != ReactionResponse::Confirmed {
            return Err(ListError("Slack did not confirm the reaction request."));
        }
        Ok(ReactionAccepted {
            record: self.record,
            raw,
        })
    }
}

impl ReactionAccepted {
    pub fn record(&self) -> &ReactionRecord {
        &self.record
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
}

impl ReactionRecord {
    pub fn target(&self) -> &Target {
        &self.target
    }
    pub fn ts(&self) -> &str {
        &self.ts
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn user(&self) -> &str {
        &self.user
    }
    pub fn adding(&self) -> bool {
        self.adding
    }
    pub fn channel_raw(&self) -> &Value {
        &self.channel
    }
    pub fn root_raw(&self) -> &Value {
        &self.root
    }
    pub fn message_raw(&self) -> &Value {
        &self.message
    }

    /// Re-capture under caller-owned fresh Target/root/channel/row observations.
    /// This checks exact facts, not account, socket, visit or renderer authority.
    pub fn current(
        &self,
        target: &Target,
        root: &Message,
        message: &Message,
        channel: &Value,
    ) -> bool {
        if let Some(choice) = &self.choice {
            return ReactionIntent::capture_choice(
                target,
                root,
                message,
                channel.clone(),
                &self.user,
                choice.clone(),
            )
            .is_ok_and(|intent| intent.record == *self);
        }
        if let Some(selected) = &self.family_selected {
            return ReactionFamilyIntent::capture(
                target,
                root,
                message,
                channel.clone(),
                &self.user,
                selected,
            )
            .is_ok_and(|intent| intent.record().children().iter().any(|row| row == self));
        }
        ReactionIntent::capture(
            target,
            root,
            message,
            channel.clone(),
            &self.user,
            &self.name,
        )
        .is_ok_and(|intent| intent.record == *self)
    }

    pub fn classify(&self, raw: &Value) -> ReactionResponse {
        if !raw.is_object() || !self.response_identity(raw) {
            return ReactionResponse::Uncertain;
        }
        if raw["ok"] == true && raw.get("error").is_none_or(Value::is_null) {
            return ReactionResponse::Confirmed;
        }
        if raw["ok"] != false {
            return ReactionResponse::Uncertain;
        }
        match raw["error"].as_str() {
            Some("already_reacted") if self.adding => ReactionResponse::DesiredState,
            Some("no_reaction") if !self.adding => ReactionResponse::DesiredState,
            Some(
                "invalid_name" | "too_many_emoji" | "too_many_reactions" | "thread_locked"
                | "ratelimited",
            ) => ReactionResponse::Rejected,
            // fatal_error/internal_error can partially succeed; unknown errors
            // and operation-inconsistent state errors do not prove rejection.
            _ => ReactionResponse::Uncertain,
        }
    }

    fn response_identity(&self, raw: &Value) -> bool {
        [
            ("channel", self.target.channel_id()),
            ("timestamp", self.ts()),
            ("ts", self.ts()),
            ("name", self.name()),
            ("user", self.user()),
        ]
        .iter()
        .all(|(key, expected)| raw.get(*key).is_none_or(|v| v.as_str() == Some(*expected)))
    }

    /// Conservative native desired-state recovery, NOT an ACK of this request.
    /// Read ORIGINAL page fields: typed defaults or realtime overlays cannot
    /// turn omitted reactions into a complete empty membership observation.
    /// Page does not establish complete reaction-name inventory: an absent
    /// selected reaction (including []) cannot confirm last-pill removal.
    pub fn observed(&self, page: &Page) -> Result<bool, ListError> {
        let target = page.target();
        if self.target.list_id() != target.list_id()
            || self.target.record_id() != target.record_id()
            || self.target.channel_id() != target.channel_id()
            || self.target.root_ts() != target.root_ts()
        {
            return Err(ListError("Reaction observation belongs to another thread."));
        }
        if page
            .raw()
            .get("deleted")
            .and_then(Value::as_array)
            .is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(self.ts())))
        {
            return Ok(false);
        }
        let Some(message) = page.raw()["messages"]
            .as_array()
            .and_then(|rows| rows.iter().find(|m| m["ts"].as_str() == Some(self.ts())))
        else {
            return Ok(false);
        };
        let typed = super::parse_message(message, target.channel_id())?;
        if !available(&typed) || typed.thread_ts.as_deref() != self.target.root_ts() {
            return Ok(false);
        }
        let Some(reactions) = message.get("reactions").and_then(Value::as_array) else {
            return Ok(false);
        };
        self.observed_reactions(reactions, false)
    }

    /// Desired-state proof from the ORIGINAL response to this record's
    /// `reactions.get(channel, timestamp, full:true)` request. The host must bind
    /// that request/response to the original current reader; an arbitrary Page
    /// or typed reserialization cannot supply this completeness authority.
    /// Unlike Page, full:true proves the complete reaction-name inventory.
    /// reactions.get also guarantees authenticated self is included when reacted,
    /// even when other reactors are omitted. The request must use this record's
    /// original user; no such self-membership guarantee is inferred for Page.
    pub fn observed_full(&self, raw: &Value) -> Result<bool, ListError> {
        Ok(self
            .full_observation(raw)?
            .is_some_and(|(_, desired)| desired))
    }

    /// Canonical reactions from the same original full:true read, even when
    /// they do not prove the requested desired state. No message-body adoption.
    pub fn full_reactions(&self, raw: &Value) -> Result<Option<Vec<Reaction>>, ListError> {
        Ok(self
            .full_observation(raw)?
            .map(|(message, _)| message.reactions))
    }

    fn full_observation(&self, raw: &Value) -> Result<Option<(Message, bool)>, ListError> {
        if raw["ok"] != true
            || raw.get("error").is_some_and(|v| !v.is_null())
            || raw["type"] != "message"
            || raw["channel"].as_str() != Some(self.target.channel_id())
            || raw["message"]["ts"].as_str() != Some(self.ts())
            || raw["message"]
                .get("thread_ts")
                .is_some_and(|v| v.as_str() != self.target.root_ts())
        {
            return Err(ListError(
                "Full reaction observation did not match the original message.",
            ));
        }
        let message = &raw["message"];
        let typed = super::parse_message(message, self.target.channel_id())?;
        if !available(&typed) {
            return Ok(None);
        }
        let reactions = message
            .get("reactions")
            .and_then(Value::as_array)
            .ok_or(ListError("Full reaction membership is unavailable."))?;
        // An incomplete/malformed unrelated row cannot establish a trustworthy
        // complete-name list either. Duplicate names are checked below.
        for reaction in reactions {
            let users = reaction["users"]
                .as_array()
                .ok_or(ListError("Full reaction membership is malformed."))?;
            let unique: Option<HashSet<&str>> = users
                .iter()
                .map(|u| u.as_str().filter(|s| identity(s)))
                .collect();
            let unique = unique.ok_or(ListError("Full reaction membership is malformed."))?;
            if unique.len() != users.len()
                || reaction["count"]
                    .as_u64()
                    .is_none_or(|n| n < unique.len() as u64)
            {
                return Err(ListError("Full reaction membership is ambiguous."));
            }
        }
        let desired = self.observed_reactions(reactions, true)?;
        Ok(Some((typed, desired)))
    }

    fn observed_reactions(
        &self,
        reactions: &[Value],
        original_full: bool,
    ) -> Result<bool, ListError> {
        let mut names = HashSet::new();
        let mut selected = None;
        for reaction in reactions {
            let name = reaction["name"]
                .as_str()
                .filter(|n| reaction_name(n))
                .ok_or(ListError("Reaction observation is malformed."))?;
            if !names.insert(name) {
                return Err(ListError("Reaction observation is ambiguous."));
            }
            if name == self.name {
                selected = Some(reaction);
            }
        }
        let Some(reaction) = selected else {
            return Ok(original_full && !self.adding);
        };
        let Some(users) = reaction["users"].as_array() else {
            return Ok(false);
        };
        let users: Option<HashSet<&str>> = users
            .iter()
            .map(|u| u.as_str().filter(|s| identity(s)))
            .collect();
        let users = users.ok_or(ListError("Reaction membership is malformed."))?;
        if reaction["users"].as_array().unwrap().len() != users.len()
            || reaction["count"]
                .as_u64()
                .is_some_and(|count| count < users.len() as u64)
        {
            return Err(ListError("Reaction membership is ambiguous."));
        }
        let mine = users.contains(self.user());
        Ok(if self.adding {
            mine
        } else {
            !mine && (original_full || reaction["count"].as_u64() == Some(users.len() as u64))
        })
    }
}

impl Page {
    /// Replace only a known live reply's typed reactions from a validated
    /// original full:true response. The host owns request/reader currentness,
    /// actual Projection visibility and accepted/live-copy reconciliation.
    /// No raw receipt, content, target or pagination field is changed.
    pub fn replace_reply_reactions(
        &mut self,
        channel: &str,
        ts: &str,
        reactions: &[Reaction],
    ) -> bool {
        let Some(root) = self.target.root_ts() else {
            return false;
        };
        if channel != self.target.channel_id()
            || !timestamp(ts)
            || ts == root
            || self
                .raw
                .get("deleted")
                .and_then(Value::as_array)
                .is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(ts)))
        {
            return false;
        }
        let Some(message) = self
            .messages
            .iter_mut()
            .find(|m| m.ts.as_deref() == Some(ts))
        else {
            return false;
        };
        if message.thread_ts.as_deref() != Some(root)
            || message.channel.as_deref().is_some_and(|c| c != channel)
            || !available(message)
            || message
                .reactions
                .iter()
                .map(|r| (&r.name, r.count, &r.users, &r.extra))
                .eq(reactions
                    .iter()
                    .map(|r| (&r.name, r.count, &r.users, &r.extra)))
        {
            return false;
        }
        message.reactions = reactions.to_vec();
        true
    }
}

fn raw(message: &Message) -> Result<Value, ListError> {
    serde_json::to_value(message).map_err(|_| ListError("List comment is malformed."))
}
fn reaction_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 256
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'+' | b':'))
}
fn available(message: &Message) -> bool {
    ![
        "isUnknown",
        "isNonExistent",
        "isEKMError",
        "is_ephemeral",
        "no_display",
        "_hidden_stub",
        "is_deleted",
        "is_tombstoned",
    ]
    .iter()
    .any(|key| truthy(message.extra.get(*key)))
        && !matches!(
            message.subtype.as_deref(),
            Some(
                "tombstone"
                    | "message_deleted"
                    | "ekm_access_denied"
                    | "dlp_tombstone"
                    | "dsa_restricted"
                    | "retention_threshold"
                    | "thread_hidden"
                    | "moderated"
                    | "file_comment"
            )
        )
}
fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(v)) => *v,
        Some(Value::Number(v)) => v.as_f64().is_some_and(|n| n != 0.0),
        Some(Value::String(v)) => !v.is_empty(),
        Some(Value::Array(_) | Value::Object(_)) => true,
    }
}

#[cfg(test)]
mod choice_tests;
#[cfg(test)]
mod tests;
