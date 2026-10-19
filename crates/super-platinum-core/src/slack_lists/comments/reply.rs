//! Immutable reply capture; inert records never authorize replay after uncertainty.
use super::*;
use crate::canvas::comments::content;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplyPolicy {
    channel: Value,
    archive_view: bool,
    can_thread: bool,
}
impl ReplyPolicy {
    /// The host supplies its positively acquired file conversation and the
    /// actual canThread predicate (including announce-only/admin policy).
    pub fn observed(channel: Value, archive_view: bool, can_thread: bool) -> Self {
        Self {
            channel,
            archive_view,
            can_thread,
        }
    }
    pub fn available(&self, target: &Target) -> bool {
        self.channel["id"].as_str() == Some(target.channel_id())
            && !self.archive_view
            && self.can_thread
            && ["is_non_threadable", "is_archived", "is_frozen", "is_locked"]
                .iter()
                .all(|key| flag(&self.channel, key) == Ok(false))
    }
    pub fn channel_raw(&self) -> &Value {
        &self.channel
    }
}

pub struct ReplyIntent {
    record: ReplyRecord,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplyRecord {
    version: u8,
    workspace: String,
    user: String,
    target: Target,
    root: Value,
    text: String,
    blocks: Vec<Value>,
    client_ts: String,
    client_msg_id: String,
}
pub struct ReplyAccepted {
    record: ReplyRecord,
    message: Message,
    raw: Value,
}

fn writable(target: &Target, root: &Message, policy: &ReplyPolicy) -> Result<(), ListError> {
    target.validate()?;
    if !policy.available(target)
        || flag(target.record_raw(), "archived")?
        || flag(target.document_raw(), "is_locked")?
        || target.document_raw()["filetype"] == "thread_share"
        || target.document_raw()["mimetype"] == "application/vnd.slack-thread-share"
        || root
            .extra
            .get("is_locked")
            .is_some_and(|v| !v.is_null() && v != &Value::Bool(false))
        || !item_root(root, target)
        || root.ts.as_deref() != target.root_ts()
        || root
            .channel
            .as_deref()
            .is_some_and(|c| c != target.channel_id())
    {
        return Err(ListError("This List comment thread cannot accept a reply."));
    }
    Ok(())
}
impl ReplyIntent {
    pub fn available(target: &Target, root: &Message, policy: &ReplyPolicy) -> bool {
        writable(target, root, policy).is_ok()
    }
    pub fn capture(
        workspace: &str,
        user: &str,
        target: &Target,
        root: &Message,
        policy: &ReplyPolicy,
        text: &str,
        client_msg_id: &str,
        client_ts: &str,
    ) -> Result<Self, ListError> {
        Self::capture_rich(
            workspace,
            user,
            target,
            root,
            policy,
            content::plain(text),
            client_msg_id,
            client_ts,
        )
    }
    pub fn capture_rich(
        workspace: &str,
        user: &str,
        target: &Target,
        root: &Message,
        policy: &ReplyPolicy,
        blocks: Vec<Value>,
        client_msg_id: &str,
        client_ts: &str,
    ) -> Result<Self, ListError> {
        writable(target, root, policy)?;
        let text = content::fallback(&blocks)
            .map_err(|_| ListError("List comment content is unsupported."))?;
        let record = ReplyRecord {
            version: 1,
            workspace: workspace.into(),
            user: user.into(),
            target: target.clone(),
            root: serde_json::to_value(root)
                .map_err(|_| ListError("List comment root is malformed."))?,
            text,
            blocks,
            client_msg_id: client_msg_id.into(),
            client_ts: client_ts.into(),
        };
        record.validate()?;
        Ok(Self { record })
    }
    pub fn record(&self) -> &ReplyRecord {
        &self.record
    }
    pub fn revalidate(
        &self,
        document: &Document,
        row: &Record,
        root: &Message,
        policy: &ReplyPolicy,
    ) -> Result<(), ListError> {
        self.record.target.revalidate(document, row)?;
        writable(&self.record.target, root, policy)?;
        if serde_json::to_value(root).map_err(|_| ListError("List comment root is malformed."))?
            != self.record.root
        {
            return Err(ListError("List comment root changed before sending."));
        }
        Ok(())
    }
    pub fn accept(self, raw: Value) -> Result<ReplyAccepted, ListError> {
        if raw["ok"] != true
            || raw.get("error").is_some_and(|v| !v.is_null())
            || raw["channel"].as_str() != Some(self.record.target.channel_id())
        {
            return Err(ListError("List comment reply was not confirmed."));
        }
        let message = parse_message(&raw["message"], self.record.target.channel_id())?;
        if message.ts.as_deref() != raw["ts"].as_str()
            || message.thread_ts.as_deref() != self.record.target.root_ts()
            || message.ts.as_deref() == self.record.target.root_ts()
            || message.client_msg_id.as_deref() != Some(&self.record.client_msg_id)
            || message.user.as_deref() != Some(self.record.user.as_str())
            || !content::confirmed(&self.record.blocks, &message.blocks)
        {
            return Err(ListError(
                "List comment acknowledgement did not match the reply.",
            ));
        }
        Ok(ReplyAccepted {
            record: self.record,
            message,
            raw,
        })
    }
}
impl ReplyRecord {
    /// A positive read can confirm this UUID/content. Absence from a partial
    /// page proves nothing and never reauthorizes a POST. Host still fences the
    /// original account/visit before publishing this observation.
    pub fn observed(&self, page: &Page) -> Result<Option<Message>, ListError> {
        self.validate()?;
        if page.target().list_id() != self.target.list_id()
            || page.target().record_id() != self.target.record_id()
            || page.target().channel_id() != self.target.channel_id()
            || page.target().root_ts() != self.target.root_ts()
        {
            return Err(ListError(
                "List comment observation belongs to another item.",
            ));
        }
        let matches: Vec<_> = page
            .messages()
            .iter()
            .filter(|message| message.client_msg_id.as_deref() == Some(self.client_msg_id.as_str()))
            .collect();
        match matches.as_slice() {
            [] => Ok(None),
            [message]
                if message.thread_ts.as_deref() == self.target.root_ts()
                    && message.ts.as_deref() != self.target.root_ts()
                    && message.user.as_deref() == Some(self.user.as_str())
                    && content::confirmed(&self.blocks, &message.blocks) =>
            {
                Ok(Some((*message).clone()))
            }
            _ => Err(ListError(
                "List comment observation did not match the reply.",
            )),
        }
    }
    pub fn validate(&self) -> Result<(), ListError> {
        self.target.validate()?;
        let root = parse_message(&self.root, self.target.channel_id())?;
        if self.version != 1
            || !identity(&self.workspace)
            || !identity(&self.user)
            || !crate::canvas::comments::reply::valid_text(&self.text)
            || self.text.trim().is_empty()
            || uuid::Uuid::parse_str(&self.client_msg_id).is_err()
            || root.ts.as_deref() != self.target.root_ts()
            || !is_root(&root)
            || !item_root(&root, &self.target)
            || !timestamp(&self.client_ts)
            || !content::fallback(&self.blocks).is_ok_and(|t| t == self.text)
        {
            return Err(ListError("List comment reply record is malformed."));
        }
        Ok(())
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
    pub fn root_raw(&self) -> &Value {
        &self.root
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn blocks(&self) -> &[Value] {
        &self.blocks
    }
    pub fn client_ts(&self) -> &str {
        &self.client_ts
    }
    pub fn client_msg_id(&self) -> &str {
        &self.client_msg_id
    }
}
impl ReplyAccepted {
    pub fn record(&self) -> &ReplyRecord {
        &self.record
    }
    pub fn message(&self) -> &Message {
        &self.message
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplyPhase {
    Ready,
    Dispatched,
    NotSent,
    Rejected,
    Uncertain,
    Accepted,
}
pub struct ReplyAttempt {
    intent: Option<ReplyIntent>,
    record: ReplyRecord,
    phase: ReplyPhase,
}
impl ReplyAttempt {
    pub fn new(intent: ReplyIntent) -> Self {
        Self {
            record: intent.record.clone(),
            intent: Some(intent),
            phase: ReplyPhase::Ready,
        }
    }
    /// Spend before entering the host async worker. There is no reset/retry API.
    pub fn take(&mut self) -> Option<ReplyIntent> {
        if self.phase != ReplyPhase::Ready {
            return None;
        }
        self.phase = ReplyPhase::Dispatched;
        self.intent.take()
    }
    pub fn settle(&mut self, phase: ReplyPhase) -> bool {
        if self.phase != ReplyPhase::Dispatched
            || !matches!(
                phase,
                ReplyPhase::NotSent | ReplyPhase::Rejected | ReplyPhase::Uncertain
            )
        {
            return false;
        }
        self.phase = phase;
        true
    }
    pub fn accepted(&mut self, accepted: &ReplyAccepted) -> bool {
        if self.phase != ReplyPhase::Dispatched || accepted.record != self.record {
            return false;
        }
        self.phase = ReplyPhase::Accepted;
        true
    }
    pub fn phase(&self) -> ReplyPhase {
        self.phase
    }
    pub fn record(&self) -> &ReplyRecord {
        &self.record
    }
}

#[cfg(test)]
mod tests;
