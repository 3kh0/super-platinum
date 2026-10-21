use super::*;
use serde_json::Map;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnEditSeed {
    workspace: String,
    user: String,
    target: Target,
    ts: String,
    http: Value,
    editable: Vec<Value>,
    preserved: Vec<Value>,
}
impl OwnEditSeed {
    pub fn capture(
        workspace: &str,
        user: &str,
        page: &Page,
        known: &Message,
        policy: &OwnEditPolicy,
        now_ms: u64,
    ) -> Result<Self, ListError> {
        let ts = known
            .ts
            .as_deref()
            .filter(|ts| timestamp(ts))
            .ok_or(ListError("The original comment is unavailable."))?;
        policy.admit(workspace, user, page.target(), ts, now_ms)?;
        // Require both actual current row and immutable original HTTP receipt.
        let current = page
            .messages()
            .iter()
            .find(|m| m.ts.as_deref() == Some(ts))
            .ok_or(ListError("The original comment is no longer loaded."))?;
        if baseline(current, page.target()) != baseline(known, page.target()) {
            return Err(ListError("The original comment changed."));
        }
        root_available(page)?;
        let raw = original(page.raw(), ts)?;
        let (editable, preserved) = partition(raw)?;
        let seed = Self {
            workspace: workspace.into(),
            user: user.into(),
            target: page.target().clone(),
            ts: ts.into(),
            http: page.raw().clone(),
            editable,
            preserved,
        };
        seed.validate()?;
        if !seed.current(known) {
            return Err(ListError(
                "The original HTTP comment body is no longer current.",
            ));
        }
        Ok(seed)
    }
    pub fn validate(&self) -> Result<(), ListError> {
        if !identity(&self.workspace)
            || !identity(&self.user)
            || !timestamp(&self.ts)
            || serde_json::to_vec(&self.http)
                .map_err(|_| ListError("Comment receipt is malformed."))?
                .len()
                > 8 * 1024 * 1024
        {
            return Err(ListError("The original comment identity is malformed."));
        }
        let page = Page::parse(&self.target, self.http.clone())?;
        if page.target() != &self.target {
            return Err(ListError("The original comment root changed."));
        }
        root_available(&page)?;
        let raw = original(&self.http, &self.ts)?;
        let message = parse_message(raw, self.target.channel_id())?;
        ordinary(&message, &self.target, &self.user)?;
        if page.raw()["deleted"]
            .as_array()
            .is_some_and(|a| a.iter().any(|v| v == &self.ts))
            || partition(raw)? != (self.editable.clone(), self.preserved.clone())
        {
            return Err(ListError("The original comment receipt is unavailable."));
        }
        Ok(())
    }
    /// Identity and editable body only. Reaction counts, reply counters, user
    /// metadata and other unrelated ACK fields do not invalidate a rich draft.
    /// Owner, fresh root/policy and actual visible projection remain host guards.
    pub fn current(&self, known: &Message) -> bool {
        ordinary(known, &self.target, &self.user).is_ok()
            && parse_message(self.original_raw(), self.target.channel_id())
                .is_ok_and(|original| Self::same_body(&self.target, &original, known))
    }
    /// Shared pre-seed body comparison. This establishes neither HTTP origin,
    /// visible membership, current owner nor permission to edit.
    pub fn same_body(target: &Target, left: &Message, right: &Message) -> bool {
        target.validate().is_ok()
            && target.root_ts().is_some()
            && [left, right].iter().all(|message| {
                message.ts.as_deref().is_some_and(timestamp)
                    && message.ts.as_deref() != target.root_ts()
                    && message.thread_ts.as_deref() == target.root_ts()
                    && message
                        .channel
                        .as_deref()
                        .is_none_or(|c| c == target.channel_id())
            })
            && baseline(left, target) == baseline(right, target)
    }
    /// Fresh HTTP preflight must still name the same item, root and original
    /// body. It never replaces the draft seed or its original receipt.
    pub fn revalidate(
        &self,
        page: &Page,
        known: &Message,
        policy: &OwnEditPolicy,
        now_ms: u64,
    ) -> Result<(), ListError> {
        self.validate()?;
        if page.target() != &self.target || !self.current(known) {
            return Err(ListError("The original comment changed while editing."));
        }
        root_available(page)?;
        let fresh = parse_message(original(page.raw(), &self.ts)?, self.target.channel_id())?;
        if !self.current(&fresh)
            || !page
                .messages()
                .iter()
                .any(|m| m.ts.as_deref() == Some(self.ts()) && self.current(m))
            || page.raw()["deleted"]
                .as_array()
                .is_some_and(|a| a.iter().any(|v| v == &self.ts))
        {
            return Err(ListError(
                "The fresh comment body does not match this draft.",
            ));
        }
        policy.admit(&self.workspace, &self.user, &self.target, &self.ts, now_ms)
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
        // Restored values must validate before use; malformed inert values
        // yield null rather than panicking or granting authority.
        original(&self.http, &self.ts).unwrap_or(&Value::Null)
    }
    pub fn editable(&self) -> &[Value] {
        &self.editable
    }
    pub fn preserved(&self) -> &[Value] {
        &self.preserved
    }
}

pub(super) fn original<'a>(http: &'a Value, ts: &str) -> Result<&'a Value, ListError> {
    let rows = http["messages"]
        .as_array()
        .ok_or(ListError("The HTTP comment receipt is unavailable."))?;
    let mut rows = rows.iter().filter(|v| v["ts"].as_str() == Some(ts));
    let row = rows.next().ok_or(ListError(
        "The original comment is absent from the HTTP receipt.",
    ))?;
    if rows.next().is_some() {
        return Err(ListError("The original comment identity is ambiguous."));
    }
    Ok(row)
}
fn partition(raw: &Value) -> Result<(Vec<Value>, Vec<Value>), ListError> {
    let blocks = raw["blocks"]
        .as_array()
        .ok_or(ListError("An original rich body is required."))?;
    if blocks.is_empty()
        || blocks.len() > 50
        || serde_json::to_vec(blocks)
            .map_err(|_| ListError("Comment body is malformed."))?
            .len()
            > content::MAX_BLOCK_BYTES
        || blocks
            .iter()
            .any(|b| b["type"] != "rich_text" && b["type"] != "context")
    {
        return Err(ListError("This comment needs an unsupported edit mode."));
    }
    let (rich, context): (Vec<_>, Vec<_>) = blocks
        .iter()
        .cloned()
        .partition(|b| b["type"] == "rich_text");
    if rich.is_empty() || content::fallback(&rich).is_err() {
        return Err(ListError(
            "This rich comment cannot be edited without conversion.",
        ));
    }
    // Context is immutable opaque source data, never an editable/rendered seed.
    Ok((rich, context))
}
fn root_available(page: &Page) -> Result<(), ListError> {
    let root = page
        .root()
        .ok_or(ListError("The original comment root is unavailable."))?;
    let raw =
        serde_json::to_value(root).map_err(|_| ListError("The comment root is malformed."))?;
    if !item_root(root, page.target())
        || root.ts.as_deref() != page.target().root_ts()
        || root
            .channel
            .as_deref()
            .is_some_and(|c| c != page.target().channel_id())
        || unavailable(&raw)
        || truthy(raw.get("is_locked"))
        || page.raw()["deleted"].as_array().is_some_and(|deleted| {
            deleted
                .iter()
                .any(|ts| ts.as_str() == page.target().root_ts())
        })
        || page.target().document_raw()["filetype"] == "thread_share"
        || page.target().document_raw()["mimetype"] == "application/vnd.slack-thread-share"
    {
        return Err(ListError(
            "The original comment root does not allow editing.",
        ));
    }
    Ok(())
}
pub(super) fn ordinary(message: &Message, target: &Target, user: &str) -> Result<(), ListError> {
    let raw = serde_json::to_value(message).map_err(|_| ListError("The comment is malformed."))?;
    if message.user.as_deref() != Some(user)
        || !message.ts.as_deref().is_some_and(timestamp)
        || message.ts.as_deref() == target.root_ts()
        || target.root_ts().is_none()
        || message.thread_ts.as_deref() != target.root_ts()
        || message
            .channel
            .as_deref()
            .is_some_and(|c| c != target.channel_id())
        || !matches!(message.kind.as_deref(), None | Some("message"))
        || !matches!(
            message.subtype.as_deref(),
            None | Some("me_message" | "thread_broadcast")
        )
        || unavailable(&raw)
        || !message.files.is_empty()
        || !message.attachments.is_empty()
        || raw
            .get("pendingFiles")
            .is_some_and(|v| v.as_array().is_none_or(|v| !v.is_empty()))
        || truthy(raw.get("markdown_text"))
        || truthy(raw.get("list_edit"))
    {
        return Err(ListError(
            "Only an ordinary own rich reply can be edited here.",
        ));
    }
    Ok(())
}
fn unavailable(raw: &Value) -> bool {
    [
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
    .any(|k| truthy(raw.get(*k)))
}
pub(super) fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null | Value::Bool(false)) => false,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|n| n != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        _ => true,
    }
}
fn baseline(message: &Message, target: &Target) -> Option<Value> {
    let mut raw = serde_json::to_value(message).ok()?;
    // Page messages can omit channel; a socket projection may fill that same ID.
    raw["channel"] = Value::String(target.channel_id().into());
    let mut selected = Map::new();
    for key in [
        "ts",
        "thread_ts",
        "channel",
        "user",
        "type",
        "subtype",
        "text",
        "blocks",
        "files",
        "attachments",
        "edited",
        "markdown_text",
    ] {
        selected.insert(key.into(), raw.get(key).cloned().unwrap_or(Value::Null));
    }
    Some(Value::Object(selected))
}
