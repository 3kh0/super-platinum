//! Bounded ordinary hosted-file Share eligibility, not a permission capability.
//! Caller owns account/current snapshot admission and server outcome custody.
use super::models::{Channel, File, User};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Eligibility {
    Allowed,
    Denied(Denial),
    Unsupported(Unsupported),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Denial {
    DeletedFile,
    UnavailableConversation,
    Archived,
    Frozen,
    PendingExternalIm,
    DeletedPeer,
    SystemIm,
    FileConversation,
    HuddleConversation,
    ReadOnly,
    ThreadOnly,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unsupported {
    FileShape,
    FileKind,
    ConversationShape,
    Membership,
    SlackConnect,
    Peer,
    AppDm,
    PostingFacts,
    RestrictionException,
    FactsIdentity,
}

/// These are independent observed facts, not inferred defaults. A new account's
/// record starts unknown even though the official selector defaults to false.
/// Never initialize this from a cache object that has already lost partial keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostingFacts {
    channel_id: String,
    read_only: Option<bool>,
    thread_only: Option<bool>,
}
impl PostingFacts {
    pub fn new(channel_id: String) -> Self {
        Self {
            channel_id,
            read_only: None,
            thread_only: None,
        }
    }
    pub fn values(&self) -> (Option<bool>, Option<bool>) {
        (self.read_only, self.thread_only)
    }
    /// Ordered current-account ingress only. Foreign IDs are refused without
    /// changing retained facts. Omitted fields never clear a prior restriction.
    /// Explicit effective booleans take precedence over all property fallback.
    pub fn observe(&mut self, raw: &Value, current_member: Option<&User>) -> bool {
        if !raw.is_object() || raw["id"].as_str() != Some(self.channel_id.as_str()) {
            return false;
        }
        let read = raw.get("is_read_only").and_then(Value::as_bool);
        let thread = raw.get("is_thread_only").and_then(Value::as_bool);
        if read.is_some() || thread.is_some() {
            // Deliberately stricter than the official partial reducer: a
            // present malformed companion retires that fact, while omission
            // preserves it. Never retain an old grant through malformed input.
            if raw.get("is_read_only").is_some() {
                self.read_only = read;
            }
            if raw.get("is_thread_only").is_some() {
                self.thread_only = thread;
            }
            return true;
        }
        // Source only computes fallback when at least one restriction is truthy.
        // Restriction objects are the supported schema; malformed explicit input
        // invalidates both facts rather than silently granting permission.
        let props = raw.get("properties");
        let post = props.and_then(|v| v.get("posting_restricted_to"));
        let reply = props.and_then(|v| v.get("threads_restricted_to"));
        if post.is_some_and(|v| !v.is_null()) || reply.is_some_and(|v| !v.is_null()) {
            // Only internal conversations are supported by this fallback. Grid
            // role decisions and externally hosted context need other stores.
            let internal = raw.get("is_ext_shared").and_then(Value::as_bool) == Some(false);
            let pair = if internal {
                restriction(post, current_member).zip(restriction(reply, current_member))
            } else {
                None
            };
            (self.read_only, self.thread_only) = match pair {
                Some((can_post, can_thread)) => (
                    Some(!can_post && !can_thread),
                    Some(!can_post && can_thread),
                ),
                None => (None, None),
            };
        } else if raw.get("is_read_only").is_some() || raw.get("is_thread_only").is_some() {
            // Nonboolean explicit effective data is not a permission grant.
            self.read_only = None;
            self.thread_only = None;
        }
        true
    }
}

fn restriction(value: Option<&Value>, member: Option<&User>) -> Option<bool> {
    // This companion omission is specifically defined by computeChannelPostingPermissions,
    // reached only when another restriction was actually supplied.
    let Some(value) = value.filter(|v| !v.is_null()) else {
        return Some(true);
    };
    let obj = value.as_object()?;
    if obj.is_empty() {
        return Some(false);
    }
    let member = member.filter(|u| {
        !u.id.is_empty()
            && !u.deleted
            && u.extra.get("isUnknown") != Some(&Value::Bool(true))
            && u.extra.get("isNonExistent") != Some(&Value::Bool(true))
    })?;
    if let Some(users) = obj.get("user") {
        let users = users.as_array()?;
        if !users.iter().all(Value::is_string) {
            return None;
        }
        if users.iter().any(|v| v.as_str() == Some(member.id.as_str())) {
            return Some(true);
        }
    }
    // JS selects the first role/type only; do not turn unknown/default role or
    // malformed shape into the captured selector's permissive default branch.
    let roles = obj.get("role").or_else(|| obj.get("type"))?.as_array()?;
    match roles.first()?.as_str()? {
        "all" | "ra" => Some(true),
        // Existing User defaults erase field presence and Grid admin requires
        // channel-workspace membership. No role privilege is guessed here.
        _ => None,
    }
}

/// Uses existing file-kind/tombstone models, never uploader ownership. This is
/// ordinary-route classification only, not proof that Slack grants access.
pub fn source(file: &File) -> Eligibility {
    if crate::file_actions::deleted(file)
        || file.extra.get("is_deleted") == Some(&Value::Bool(true))
    {
        return Eligibility::Denied(Denial::DeletedFile);
    }
    if !file.id.as_deref().is_some_and(|id| id_shape(id, b"F", 2)) {
        return Eligibility::Unsupported(Unsupported::FileShape);
    }
    if ["is_deleted", "is_tombstoned"]
        .iter()
        .any(|key| file.extra.get(*key).is_some_and(|v| !v.is_boolean()))
    {
        return Eligibility::Unsupported(Unsupported::FileShape);
    }
    if !crate::file_actions::renameable(file)
        || crate::slack_lists::is_list(file)
        || file.extra.get("mode").and_then(Value::as_str) != Some("hosted")
        || file.is_external != Some(false)
        || file.filetype.as_deref().is_none_or(|kind| kind.is_empty())
    {
        return Eligibility::Unsupported(Unsupported::FileKind);
    }
    Eligibility::Allowed
}

/// `raw` must be the current canonical destination snapshot, not a serialized
/// default-filled Channel. Deserialization reuses Channel/User routing models;
/// raw presence is retained for permission and membership evidence.
pub fn destination(raw: &Value, facts: &PostingFacts, peer: Option<&Value>) -> Eligibility {
    use Eligibility::{Allowed, Denied, Unsupported as Unknown};
    let Ok(channel) = serde_json::from_value::<Channel>(raw.clone()) else {
        return Unknown(Unsupported::ConversationShape);
    };
    for key in [
        "isUnknown",
        "isNonExistent",
        "is_frozen",
        "is_pending_ext_shared",
        "is_file",
    ] {
        if raw.get(key).is_some_and(|value| !value.is_boolean()) {
            return Unknown(Unsupported::ConversationShape);
        }
    }
    if raw.get("properties").is_some_and(|v| !v.is_object())
        || raw
            .pointer("/properties/huddles")
            .is_some_and(|v| !v.is_object())
        || raw
            .pointer("/properties/huddles/is_huddle_channel")
            .is_some_and(|v| !v.is_boolean())
    {
        return Unknown(Unsupported::ConversationShape);
    }
    if !id_shape(&channel.id, b"CGD", 9) {
        return Unknown(Unsupported::ConversationShape);
    }
    if facts.channel_id != channel.id {
        return Unknown(Unsupported::FactsIdentity);
    }
    if marked(raw, "isUnknown") || marked(raw, "isNonExistent") {
        return Denied(Denial::UnavailableConversation);
    }
    if channel.is_archived {
        return Denied(Denial::Archived);
    }
    if marked(raw, "is_frozen") {
        return Denied(Denial::Frozen);
    }
    if channel.is_im && marked(raw, "is_pending_ext_shared") {
        return Denied(Denial::PendingExternalIm);
    }
    if marked(raw, "is_file") {
        return Denied(Denial::FileConversation);
    }
    if raw.pointer("/properties/huddles/is_huddle_channel") == Some(&Value::Bool(true)) {
        return Denied(Denial::HuddleConversation);
    }
    if raw.get("is_ext_shared").and_then(Value::as_bool) != Some(false) {
        return Unknown(Unsupported::SlackConnect);
    }
    if channel.is_im {
        if channel.user.as_deref() == Some("USLACK") {
            return Denied(Denial::SystemIm);
        }
        let Some(peer_raw) = peer else {
            return Unknown(Unsupported::Peer);
        };
        let Ok(peer) = serde_json::from_value::<User>(peer_raw.clone()) else {
            return Unknown(Unsupported::Peer);
        };
        if Some(peer.id.as_str()) != channel.user.as_deref() {
            return Unknown(Unsupported::Peer);
        }
        if peer.deleted {
            return Denied(Denial::DeletedPeer);
        }
        if peer.is_bot || peer.id == "USLACKBOT" {
            return Unknown(Unsupported::AppDm);
        }
        if peer_raw.get("is_bot").and_then(Value::as_bool) != Some(false)
            || peer_raw.get("deleted").and_then(Value::as_bool) != Some(false)
        {
            return Unknown(Unsupported::Peer);
        }
    }
    if channel.is_im || channel.is_mpim {
        if raw.get("is_open").and_then(Value::as_bool) != Some(true) {
            return Unknown(Unsupported::Membership);
        }
    } else if channel.is_channel || channel.is_group || channel.is_private {
        if raw.get("is_member").and_then(Value::as_bool) != Some(true) {
            return Unknown(Unsupported::Membership);
        }
    } else {
        return Unknown(Unsupported::ConversationShape);
    }
    match facts.values() {
        (Some(false), Some(false)) => Allowed,
        (Some(_), Some(_)) if channel.is_im || channel.is_mpim => {
            // Inconsistent retained restrictions in an IM are ignored in the
            // source, but app/plan exception coverage is not complete here.
            Unknown(Unsupported::RestrictionException)
        }
        (Some(read), Some(thread))
            if raw.get("is_general").and_then(Value::as_bool) == Some(true) =>
        {
            if read {
                Denied(Denial::ReadOnly)
            } else if thread {
                Denied(Denial::ThreadOnly)
            } else {
                Allowed
            }
        }
        (Some(_), Some(_)) => Unknown(Unsupported::RestrictionException),
        _ => Unknown(Unsupported::PostingFacts),
    }
}
fn marked(raw: &Value, key: &str) -> bool {
    raw.get(key) == Some(&Value::Bool(true))
}
fn id_shape(id: &str, prefixes: &[u8], min: usize) -> bool {
    (min..=64).contains(&id.len())
        && prefixes.contains(&id.as_bytes()[0])
        && id
            .bytes()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

#[cfg(test)]
mod tests;
