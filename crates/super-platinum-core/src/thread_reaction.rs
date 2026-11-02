//! Isolated existing-Root reaction DATA, never a request or native action grant.
//!
//! The host supplies actual original pre-serde responses and separately owns
//! account, namespace, row, socket, policy versions and physical dispatch. A
//! typed Message reserialization cannot recover omitted raw properties. The
//! only absence folds here are the named fresh first-insertion profile from
//! retained vkwm/ZGtP and Root action01/02; no old model is read or merged.
use serde_json::Value;

pub mod first_add;
pub mod first_add_alias;
pub mod first_add_custom;
pub mod first_add_preferred;
mod membership;
mod selection;
pub use membership::RootMembership;
pub use selection::RootToggle;

const UNAVAILABLE: &str = "Root reaction data is unavailable.";
type Result<T> = std::result::Result<T, &'static str>;

#[derive(Clone, PartialEq, Eq)]
enum Thread {
    NoThread,
    SameRoot,
}

/// Original raw root identity and classification. This conveys no provenance.
#[derive(Clone, PartialEq, Eq)]
pub struct OriginalRoot {
    channel: String,
    timestamp: String,
    user: String,
    thread: Thread,
}
impl OriginalRoot {
    /// `raw_root` must be the caller's original incoming getView root object.
    /// These pure checks cannot establish where a public Value came from.
    pub fn capture(channel: &str, ts: &str, raw_root: &Value) -> Result<Self> {
        coordinate(channel, ts)?;
        let (user, thread) = ordinary_root(raw_root, channel, ts)?;
        Ok(Self {
            channel: channel.into(),
            timestamp: ts.into(),
            user,
            thread,
        })
    }
    pub fn channel(&self) -> &str {
        &self.channel
    }
    pub fn timestamp(&self) -> &str {
        &self.timestamp
    }
    /// CSV lookup identity only; an ID is not proof of a human/profile policy.
    pub fn user(&self) -> &str {
        &self.user
    }
    /// The actual truthy same-root branch is never normalized away.
    pub fn thread_ts(&self) -> Option<&str> {
        (self.thread == Thread::SameRoot).then_some(self.timestamp.as_str())
    }
    /// Pure field equivalence only, not a native currentness observation.
    pub fn matches(&self, raw_root: &Value) -> bool {
        Self::capture(&self.channel, &self.timestamp, raw_root).is_ok_and(|v| v == *self)
    }
}

/// Fresh exact-coordinate policy operands, independent of the native owner.
#[derive(Clone, PartialEq)]
pub struct RootPolicy {
    original: OriginalRoot,
    message: Value,
    channel: Value,
    hidden_users: String,
    preferences: crate::composer_emoji::preferences::Preferences,
}
impl RootPolicy {
    pub fn capture(
        original: &OriginalRoot,
        history: &Value,
        info: &Value,
        full_prefs: &Value,
    ) -> Result<Self> {
        envelope(history)?;
        envelope(info)?;
        envelope(full_prefs)?;
        optional_channel(history, original.channel())?;
        if history.get("has_more").is_some_and(|v| !v.is_boolean()) {
            return Err(UNAVAILABLE);
        }
        let messages = history
            .get("messages")
            .and_then(Value::as_array)
            .ok_or(UNAVAILABLE)?;
        if messages.len() != 1 || !original.matches(&messages[0]) {
            return Err(UNAVAILABLE);
        }
        let channel = info
            .get("channel")
            .filter(|v| v.is_object())
            .ok_or(UNAVAILABLE)?;
        let name = channel
            .get("name")
            .and_then(Value::as_str)
            .ok_or(UNAVAILABLE)?;
        if channel.get("id").and_then(Value::as_str) != Some(original.channel())
            || name.is_empty()
            || name.trim() != name
            || name.len() > 255
            || name.chars().any(char::is_control)
            || channel.get("is_channel") != Some(&Value::Bool(true))
            || channel.get("is_member") != Some(&Value::Bool(true))
            || channel.get("is_private") != Some(&Value::Bool(false))
            || channel.get("is_frozen") != Some(&Value::Bool(false))
        {
            return Err(UNAVAILABLE);
        }
        // ZGtP fresh construction normalizes IM/group/MPIM and preserves the
        // absent is_file field. This isolated fold never writes shared flags.
        for field in [
            "is_im",
            "is_group",
            "is_mpim",
            "is_file",
            "is_archived",
            "isUnknown",
            "isNonExistent",
        ] {
            absent_or_false(channel, field)?;
        }
        let prefs = full_prefs
            .get("prefs")
            .and_then(Value::as_object)
            .ok_or(UNAVAILABLE)?;
        let hidden = prefs
            .get("hidden_users")
            .and_then(Value::as_str)
            .ok_or(UNAVAILABLE)?;
        // A native bound, not Slack's schema. Empty CSV is a real observation;
        // missing/null is not. Literal exclusion dominates both SH4S branches
        // without assuming the locked hide_person_opt_out value or global map.
        if hidden.len() > 64 * 1024 || hidden.split(',').any(|id| id == original.user()) {
            return Err(UNAVAILABLE);
        }
        let mut preferences = crate::composer_emoji::preferences::Preferences::default();
        preferences.replace(&prefs.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
        Ok(Self {
            original: original.clone(),
            message: messages[0].clone(),
            channel: channel.clone(),
            hidden_users: hidden.into(),
            preferences,
        })
    }
    pub fn original(&self) -> &OriginalRoot {
        &self.original
    }
    /// Immutable preference DATA from this same complete response, not a
    /// receipt of the host's actual preference producer or current namespace.
    pub fn preferences(&self) -> &crate::composer_emoji::preferences::Preferences {
        &self.preferences
    }
    /// Re-evaluates immutable DATA only; caller-owned versions still matter.
    pub fn current(
        &self,
        original: &OriginalRoot,
        history: &Value,
        info: &Value,
        full_prefs: &Value,
    ) -> bool {
        Self::capture(original, history, info, full_prefs).is_ok_and(|v| v == *self)
    }
}

fn envelope(raw: &Value) -> Result<()> {
    if !raw.is_object()
        || raw.get("ok") != Some(&Value::Bool(true))
        || raw.get("error").is_some_and(|v| !v.is_null())
    {
        return Err(UNAVAILABLE);
    }
    Ok(())
}
fn coordinate(channel: &str, ts: &str) -> Result<()> {
    let Some((seconds, fraction)) = ts.split_once('.') else {
        return Err(UNAVAILABLE);
    };
    if !(2..=64).contains(&channel.len())
        || !channel.starts_with('C')
        || !channel
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        || seconds.len() != 10
        || fraction.len() != 6
        || !seconds
            .bytes()
            .chain(fraction.bytes())
            .all(|b| b.is_ascii_digit())
        || seconds.bytes().all(|b| b == b'0')
    {
        return Err(UNAVAILABLE);
    }
    Ok(())
}
fn user_id(id: &str) -> bool {
    (2..=255).contains(&id.len())
        && matches!(id.as_bytes()[0], b'U' | b'W')
        && id
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}
fn optional_channel(raw: &Value, channel: &str) -> Result<()> {
    if raw
        .get("channel")
        .is_some_and(|v| v.as_str() != Some(channel))
    {
        return Err(UNAVAILABLE);
    }
    Ok(())
}
fn absent_or_false(raw: &Value, field: &str) -> Result<()> {
    if raw.get(field).is_some_and(|v| v != &Value::Bool(false)) {
        return Err(UNAVAILABLE);
    }
    Ok(())
}
fn ordinary_root(raw: &Value, channel: &str, ts: &str) -> Result<(String, Thread)> {
    if !raw.is_object()
        || raw.get("type").and_then(Value::as_str) != Some("message")
        || raw.get("ts").and_then(Value::as_str) != Some(ts)
        || raw.get("subtype").is_some_and(|v| !v.is_null())
    {
        return Err(UNAVAILABLE);
    }
    optional_channel(raw, channel)?;
    let user = raw
        .get("user")
        .and_then(Value::as_str)
        .filter(|id| user_id(id))
        .ok_or(UNAVAILABLE)?;
    let thread = match raw.get("thread_ts") {
        None | Some(Value::Null) => Thread::NoThread,
        Some(Value::String(s)) if s.is_empty() => Thread::NoThread,
        Some(Value::String(s)) if s == ts => Thread::SameRoot,
        _ => return Err(UNAVAILABLE),
    };
    // vkwm preserves these original fields. Missing/null/false is consumed
    // ONLY inside this fresh profile; an original known veto cannot be cleared
    // by later omission because such an OriginalRoot cannot be constructed.
    for field in ["is_locked", "is_beyond_free_limit"] {
        if !matches!(
            raw.get(field),
            None | Some(Value::Null | Value::Bool(false))
        ) {
            return Err(UNAVAILABLE);
        }
    }
    for field in [
        "is_ephemeral",
        "isEKMError",
        "isUnknown",
        "isNonExistent",
        "hidden",
        "is_hidden",
        "deleted",
        "is_deleted",
        "is_tombstone",
        "is_tombstoned",
        "no_display",
        "_hidden_stub",
    ] {
        absent_or_false(raw, field)?;
    }
    // Explicitly narrower ordinary user-authored profile. These are native
    // supported-domain refusals, not a claim that all Source classes use them.
    for field in [
        "bot_id",
        "bot_profile",
        "app_id",
        "document_comment",
        "workflow",
        "workflow_id",
        "workflow_responses",
        "ai_context",
        "file",
        "file_id",
    ] {
        if raw.get(field).is_some_and(|v| !v.is_null()) {
            return Err(UNAVAILABLE);
        }
    }
    if raw
        .get("files")
        .is_some_and(|v| !v.is_null() && !v.as_array().is_some_and(Vec::is_empty))
    {
        return Err(UNAVAILABLE);
    }
    Ok((user.into(), thread))
}

#[cfg(test)]
mod tests;
