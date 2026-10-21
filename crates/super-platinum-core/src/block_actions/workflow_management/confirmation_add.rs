//! AFZ+/vsQp/nGyk shortcut bookmark addition. These are captured facts, not
//! permission, owner-currentness, transport settlement, or navigation authority.
use super::*;
use crate::block_actions::posting_policy::Flags;

#[cfg(test)]
mod tests;

#[derive(Clone, Debug, PartialEq)]
pub struct Shortcut {
    raw: Value,
    id: String,
    title: String,
    url: Option<String>,
}
impl Shortcut {
    pub fn capture(raw: &Value) -> Result<Self, Error> {
        if raw["type"] != "shortcut" {
            return Err(invalid());
        }
        let id = identifier(raw, "id")?;
        let title = match raw.get("name") {
            None | Some(Value::Null) => id.clone(),
            Some(Value::String(s)) => s.clone(),
            _ => return Err(invalid()),
        };
        let url = match raw.get("shortcut_url") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) => Some(s.clone()),
            _ => return Err(invalid()),
        };
        Ok(Self {
            raw: raw.clone(),
            id,
            title,
            url,
        })
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }
}

/// Literal retained caller behavior: its first trigger candidate is tested with
/// zdO/ object predicates, whereas step input values use kdyP's ID expression.
/// A candidate is not a resolved channel or a permission proof.
/// JSON object insertion order is not retained by this core's Value. If the
/// first matching step has different channel candidates, leave selection empty
/// rather than invent Object.values order. Explicit selection remains available.
pub fn initial_candidate(trigger: &Value, workflow: &Value) -> Option<Value> {
    if let Some(candidate) = trigger.pointer("/channel_ids/0") {
        if candidate.is_object()
            && candidate["is_mpim"] != true
            && (candidate["is_private"] == true
                || candidate["is_group"] == true
                || candidate["is_channel"] == true)
        {
            return Some(candidate.clone());
        }
    }
    for step in workflow.get("steps")?.as_array()? {
        let Some(inputs) = step.get("inputs").and_then(Value::as_object) else {
            continue;
        };
        let mut found = None;
        for input in inputs.values() {
            if input["value"].as_str().is_some_and(looks_like_channel_id) {
                if found.as_ref().is_some_and(|v| v != &input["value"]) {
                    return None;
                }
                found = Some(input["value"].clone());
            }
        }
        if found.is_some() {
            return found;
        }
    }
    None
}
fn looks_like_channel_id(value: &str) -> bool {
    value.len() >= 9
        && matches!(value.as_bytes().first(), Some(b'C' | b'G' | b'D' | b'Z'))
        && value
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

/// Complete observed list for this exact channel. A missing list is not empty.
#[derive(Clone, Debug, PartialEq)]
pub struct Bookmarks {
    channel: String,
    raw: Value,
    rows: Vec<Value>,
}
impl Bookmarks {
    pub fn request(channel: &str) -> Result<RequestPlan, Error> {
        if !channel_id(channel) {
            return Err(invalid());
        }
        Ok(RequestPlan::new(
            "bookmarks.list",
            "bookmarks-list-api",
            json!({
                "channel":channel,"include_folders":true,"include_legacy_workflows":true
            }),
        ))
    }
    pub fn decode(channel: &str, raw: Value) -> Result<Self, Error> {
        if !channel_id(channel) {
            return Err(invalid());
        }
        success(&raw)?;
        let rows = array(&raw, "bookmarks")?;
        // No incomplete pagination can manufacture the absence of a duplicate.
        if raw
            .pointer("/response_metadata/next_cursor")
            .is_some_and(|v| !v.is_null() && v != "")
            || raw.get("has_more").is_some_and(|v| v != false)
        {
            return Err(invalid());
        }
        for row in rows {
            if !row.is_object()
                || row.get("channel_id").is_some_and(|v| v != channel)
                || row
                    .get("shortcut_id")
                    .is_some_and(|v| !v.is_null() && !v.is_string())
            {
                return Err(invalid());
            }
        }
        Ok(Self {
            channel: channel.into(),
            rows: rows.clone(),
            raw,
        })
    }
    pub fn channel(&self) -> &str {
        &self.channel
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    pub fn rows(&self) -> &[Value] {
        &self.rows
    }
    pub fn contains_shortcut(&self, id: &str) -> bool {
        self.rows.iter().any(|row| row["shortcut_id"] == id)
    }
}

/// Posting flags must be the actual effective policy for this original channel.
/// `fromAnotherTeam` must be the canonical derived fact, not `is_ext_shared`.
/// The Feature action's host restriction is not an Add permission predicate.
pub fn channel_eligible(raw: &Value, flags: Flags, team: &str, top_level: &str) -> bool {
    if !id(team)
        || !id(top_level)
        || !flags.permits_root_post()
        || !raw["id"].as_str().is_some_and(channel_id)
        || raw["fromAnotherTeam"] != false
    {
        return false;
    }
    for key in [
        "is_channel",
        "is_private",
        "is_group",
        "is_member",
        "is_archived",
        "is_im",
        "is_mpim",
        "isUnknown",
        "isNonExistent",
    ] {
        if raw.get(key).is_some_and(|v| !v.is_boolean()) {
            return false;
        }
    }
    (raw["is_channel"] == true || raw["is_private"] == true || raw["is_group"] == true)
        && raw["is_member"] == true
        && raw["is_archived"] != true
        && raw["is_im"] != true
        && raw["is_mpim"] != true
        && raw["isUnknown"] != true
        && raw["isNonExistent"] != true
}
fn channel_id(value: &str) -> bool {
    id(value) && matches!(value.as_bytes().first(), Some(b'C' | b'G'))
}

#[derive(Clone, Debug, PartialEq)]
pub struct Add {
    shortcut: Shortcut,
    channel: Value,
    bookmarks: Bookmarks,
    plan: RequestPlan,
}
impl Add {
    pub fn new(
        shortcut: &Shortcut,
        channel: &Value,
        flags: Flags,
        bookmarks: &Bookmarks,
        team: &str,
        top_level: &str,
    ) -> Result<Self, Error> {
        if !channel_eligible(channel, flags, team, top_level) || channel["id"] != bookmarks.channel
        {
            return Err(invalid());
        }
        if bookmarks.contains_shortcut(shortcut.id()) {
            return Err(Error::Api(
                "workflow_confirmation_duplicate_shortcut".into(),
            ));
        }
        let mut arguments = json!({"channel":bookmarks.channel,"title":shortcut.title(),
            "type":"shortcut","shortcut_id":shortcut.id()});
        if let Some(url) = shortcut.url() {
            arguments["link"] = json!(url);
        }
        Ok(Self {
            shortcut: shortcut.clone(),
            channel: channel.clone(),
            bookmarks: bookmarks.clone(),
            plan: RequestPlan::new("bookmarks.add", "channel-tab-add-bookmark", arguments),
        })
    }
    pub fn plan(&self) -> &RequestPlan {
        &self.plan
    }
    pub fn request(&self, client: &SlackClient, workspace: &WorkspaceSession) -> PreparedRequest {
        self.plan.request(client, workspace)
    }
    /// Strict native ACK boundary. Rejected/malformed original replies must still
    /// be retained by the operation owner; independent list reads are not ACKs.
    pub fn acknowledge(&self, raw: Value) -> Result<Added, Error> {
        success(&raw)?;
        let bookmark = &raw["bookmark"];
        if !bookmark.is_object()
            || bookmark["channel_id"] != self.bookmarks.channel
            || bookmark["shortcut_id"] != self.shortcut.id()
            || bookmark["type"] != "shortcut"
            || !bookmark["id"].as_str().is_some_and(|v| !v.is_empty())
        {
            return Err(invalid());
        }
        let typed = serde_json::from_value(bookmark.clone()).map_err(|_| invalid())?;
        Ok(Added {
            bookmark: normalize(bookmark),
            typed,
            raw,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Added {
    raw: Value,
    bookmark: Value,
    typed: crate::bookmarks::Bookmark,
}
impl Added {
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    pub fn raw_bookmark(&self) -> &Value {
        &self.raw["bookmark"]
    }
    /// Exact gBju.uN projection; opaque server fields remain in raw_bookmark().
    pub fn bookmark(&self) -> &Value {
        &self.bookmark
    }
    pub fn typed_bookmark(&self) -> &crate::bookmarks::Bookmark {
        &self.typed
    }
    pub fn id(&self) -> &str {
        self.bookmark["id"].as_str().unwrap()
    }
    pub fn channel(&self) -> &str {
        self.bookmark["channelId"].as_str().unwrap()
    }
    /// i81W.Wg, separate from accepting/upserting the returned bookmark.
    pub fn navigable(&self) -> bool {
        self.id()
            .strip_prefix("Bk")
            .is_some_and(|tail| !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_alphanumeric()))
    }
}
fn normalize(raw: &Value) -> Value {
    let mut out = serde_json::Map::new();
    for (source, target) in [
        ("channel_id", "channelId"),
        ("date_create", "dateCreate"),
        ("date_update", "dateUpdate"),
        ("emoji", "emoji"),
        ("id", "id"),
        ("image_url", "iconUrl"),
        ("parent_id", "parentId"),
        ("link", "url"),
        ("rank", "rank"),
        ("title", "title"),
        ("entity_id", "entityId"),
        ("type", "type"),
        ("last_updated_by_team_id", "lastUpdatedByTeamId"),
        ("last_updated_by_user_id", "lastUpdatedByUserId"),
        ("shortcut_id", "shortcutId"),
        ("app_id", "appId"),
        ("quip_section_id", "quipSectionId"),
        ("app_action_id", "appActionId"),
    ] {
        if let Some(value) = raw.get(source) {
            if source != "emoji" || value != "" {
                out.insert(target.into(), value.clone());
            }
        }
    }
    Value::Object(out)
}
