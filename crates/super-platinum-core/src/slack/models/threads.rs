use super::*;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThreadsViewPage {
    #[serde(default)]
    pub total_unread_replies: u32,
    #[serde(default)]
    pub new_threads_count: u32,
    #[serde(default)]
    pub threads: Vec<ThreadViewItem>,
    #[serde(default)]
    pub has_more: bool,
    #[serde(default)]
    pub max_ts: Option<MessageTs>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThreadViewItem {
    pub root_msg: Message,
    #[serde(default)]
    pub unread_replies: Vec<Message>,
    #[serde(default)]
    pub latest_replies: Vec<Message>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl ThreadViewItem {
    pub fn channel(&self) -> Option<&ChannelId> {
        self.root_msg.channel.as_ref().or_else(|| {
            self.unread_replies
                .iter()
                .chain(&self.latest_replies)
                .find_map(|message| message.channel.as_ref())
        })
    }

    pub fn root_ts(&self) -> Option<&MessageTs> {
        self.root_msg
            .thread_ts
            .as_ref()
            .or(self.root_msg.ts.as_ref())
    }

    pub fn latest_ts(&self) -> Option<&MessageTs> {
        self.unread_replies
            .iter()
            .chain(&self.latest_replies)
            .filter_map(|message| message.ts.as_ref())
            .max_by(|a, b| crate::state::cmp_ts(Some(a), Some(b)))
            .or(self.root_msg.latest_reply.as_ref())
            .or_else(|| self.root_ts())
    }

    pub fn replies(&self) -> Vec<&Message> {
        let mut replies: Vec<_> = self
            .latest_replies
            .iter()
            .chain(&self.unread_replies)
            .collect();
        replies.sort_by(|a, b| crate::state::cmp_ts(a.ts.as_deref(), b.ts.as_deref()));
        replies.dedup_by(|a, b| a.ts.is_some() && a.ts == b.ts);
        replies
    }
}
