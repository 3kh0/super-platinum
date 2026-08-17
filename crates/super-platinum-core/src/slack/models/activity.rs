use super::*;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActivityFeedPage {
    #[serde(default)]
    pub items: Vec<ActivityItem>,
    #[serde(default)]
    pub response_metadata: Option<ResponseMetadata>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActivityItem {
    #[serde(default)]
    pub is_unread: bool,
    #[serde(default)]
    pub feed_ts: String,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub item: ActivityEntry,
    #[serde(default)]
    pub is_bot: bool,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActivityEntry {
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub message: Option<ActivityMessageRef>,
    #[serde(default)]
    pub reaction: Option<ActivityReaction>,
    #[serde(default)]
    pub bundle_info: Option<ActivityBundleInfo>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActivityMessageRef {
    #[serde(default)]
    pub ts: Option<MessageTs>,
    #[serde(default)]
    pub channel: Option<ChannelId>,
    #[serde(default)]
    pub thread_ts: Option<MessageTs>,
    #[serde(default)]
    pub author_user_id: Option<UserId>,
    #[serde(default)]
    pub is_broadcast: bool,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActivityReaction {
    #[serde(default)]
    pub user: Option<UserId>,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActivityBundleInfo {
    #[serde(default)]
    pub payload: Option<ActivityBundlePayload>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActivityBundlePayload {
    #[serde(default)]
    pub thread_entry: Option<ActivityThreadEntry>,
    #[serde(default)]
    pub dm_entry: Option<ActivityDmEntry>,
    #[serde(default)]
    pub channel_entry: Option<ActivityChannelEntry>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActivityChannelEntry {
    #[serde(default)]
    pub latest_message: Option<ActivityMessageRef>,
    #[serde(default)]
    pub unread_msg_count: u32,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActivityDmEntry {
    #[serde(default)]
    pub latest_message: Option<ActivityMessageRef>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActivityThreadEntry {
    #[serde(default)]
    pub channel_id: Option<ChannelId>,
    #[serde(default)]
    pub thread_ts: Option<MessageTs>,
    #[serde(default)]
    pub latest_ts: Option<MessageTs>,
    #[serde(default)]
    pub unread_msg_count: u32,
    #[serde(default)]
    pub min_unread_ts: Option<MessageTs>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessagesListPage {
    #[serde(default)]
    pub messages_data: BTreeMap<ChannelId, MessagesListChannel>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessagesListChannel {
    #[serde(default)]
    pub messages: Vec<Message>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl ActivityItem {
    pub fn is_thread(&self) -> bool {
        self.thread_entry().is_some()
            || matches!(self.item.kind.as_str(), "thread_v2" | "thread_reply")
    }

    pub fn is_conversation(&self) -> bool {
        matches!(self.item.kind.as_str(), "dm" | "bot_dm_bundle" | "channel")
    }

    pub fn mark_read(&mut self) -> bool {
        let changed = self.is_unread;
        self.is_unread = false;
        if let Some(payload) = self
            .item
            .bundle_info
            .as_mut()
            .and_then(|bundle| bundle.payload.as_mut())
        {
            if let Some(entry) = payload.thread_entry.as_mut() {
                entry.unread_msg_count = 0;
            }
            if let Some(entry) = payload.channel_entry.as_mut() {
                entry.unread_msg_count = 0;
            }
        }
        changed
    }

    pub fn channel(&self) -> Option<&str> {
        if let Some(entry) = self.thread_entry() {
            return entry.channel_id.as_deref();
        }
        if let Some(dm) = self.dm_message() {
            return dm.channel.as_deref();
        }
        if let Some(latest) = self.channel_message() {
            return latest.channel.as_deref();
        }
        self.item
            .message
            .as_ref()
            .and_then(|m| m.channel.as_deref())
    }

    pub fn ts(&self) -> Option<&str> {
        if let Some(entry) = self.thread_entry() {
            return entry.thread_ts.as_deref();
        }
        if let Some(dm) = self.dm_message() {
            return dm.ts.as_deref();
        }
        if let Some(latest) = self.channel_message() {
            return latest.ts.as_deref();
        }
        self.item.message.as_ref().and_then(|m| m.ts.as_deref())
    }

    pub fn thread_ts(&self) -> Option<&str> {
        if let Some(entry) = self.thread_entry() {
            return entry.thread_ts.as_deref();
        }
        self.item
            .message
            .as_ref()
            .and_then(|m| m.thread_ts.as_deref())
    }

    pub fn author(&self) -> Option<&str> {
        if let Some(reaction) = &self.item.reaction {
            return reaction.user.as_deref();
        }
        if let Some(latest) = self.channel_message() {
            return latest.author_user_id.as_deref();
        }
        self.item
            .message
            .as_ref()
            .and_then(|m| m.author_user_id.as_deref())
    }

    pub fn latest_ts(&self) -> Option<&str> {
        self.thread_entry().and_then(|e| e.latest_ts.as_deref())
    }

    pub fn min_unread_ts(&self) -> Option<&str> {
        self.thread_entry().and_then(|e| e.min_unread_ts.as_deref())
    }

    pub fn preview_ts(&self) -> Option<&str> {
        self.latest_ts().or_else(|| self.ts())
    }

    pub fn request_ts(&self) -> Vec<String> {
        let mut out = Vec::new();
        for ts in [self.ts(), self.latest_ts()].into_iter().flatten() {
            let ts = ts.to_owned();
            if !out.contains(&ts) {
                out.push(ts);
            }
        }
        out
    }

    fn thread_entry(&self) -> Option<&ActivityThreadEntry> {
        self.item
            .bundle_info
            .as_ref()?
            .payload
            .as_ref()?
            .thread_entry
            .as_ref()
    }

    fn dm_message(&self) -> Option<&ActivityMessageRef> {
        self.item
            .bundle_info
            .as_ref()?
            .payload
            .as_ref()?
            .dm_entry
            .as_ref()?
            .latest_message
            .as_ref()
    }

    fn channel_entry(&self) -> Option<&ActivityChannelEntry> {
        self.item
            .bundle_info
            .as_ref()?
            .payload
            .as_ref()?
            .channel_entry
            .as_ref()
    }

    fn channel_message(&self) -> Option<&ActivityMessageRef> {
        self.channel_entry()?.latest_message.as_ref()
    }

    pub fn unread_msg_count(&self) -> u32 {
        if let Some(entry) = self.thread_entry() {
            return entry.unread_msg_count;
        }
        self.channel_entry()
            .map(|e| e.unread_msg_count)
            .unwrap_or(0)
    }

    pub fn identity(&self) -> String {
        let channel = self.channel().unwrap_or("");
        if self.is_thread() {
            let thread_ts = self.thread_ts().unwrap_or("");
            return format!("thread:{channel}:{thread_ts}");
        }
        let kind = self.item.kind.as_str();
        if matches!(kind, "dm" | "bot_dm_bundle") {
            return format!("dm:{channel}");
        }
        if kind == "channel" {
            return format!("channel:{channel}");
        }
        let ts = self.ts().unwrap_or("");
        format!("{kind}:{channel}:{ts}")
    }
}
