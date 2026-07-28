use super::*;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchMessagesPage {
    #[serde(default)]
    pub items: Vec<SearchItem>,
    #[serde(default)]
    pub pagination: Option<SearchPagination>,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub module: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchItem {
    #[serde(default)]
    pub iid: Option<String>,
    #[serde(default)]
    pub team: Option<TeamId>,
    #[serde(default)]
    pub channel: Option<Channel>,
    #[serde(default)]
    pub messages: Vec<Message>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchPagination {
    #[serde(default)]
    pub page: Option<u32>,
    #[serde(default)]
    pub page_count: Option<u32>,
    #[serde(default)]
    pub per_page: Option<u32>,
    #[serde(default)]
    pub total_count: Option<u64>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchInlinePage {
    #[serde(default)]
    pub items: Vec<SearchInlineItem>,
    #[serde(default)]
    pub pagination: Option<SearchPagination>,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchInlineItem {
    #[serde(default)]
    pub channel_id: Option<ChannelId>,
    #[serde(default)]
    pub iid: Option<String>,
    #[serde(default)]
    pub permalink: Option<String>,
    #[serde(default)]
    pub ts: Option<MessageTs>,
    #[serde(default)]
    pub user: Option<UserId>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OpenedConversation {
    pub channel: OpenedChannel,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OpenedChannel {
    pub id: ChannelId,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChannelSectionsPage {
    #[serde(default)]
    pub channel_sections: Vec<ChannelSection>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChannelSection {
    pub channel_section_id: String,
    #[serde(default)]
    pub name: String,
    /// "slack_connect" | "direct_messages" | "stars" | "channels" |
    /// "user_group" | "recent_apps" | "salesforce_records" | "agents" | "standard"
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub next_channel_section_id: Option<String>,
    #[serde(default)]
    pub is_hidden: bool,
    #[serde(default)]
    pub channel_ids_page: ChannelIdsPage,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChannelIdsPage {
    #[serde(default)]
    pub channel_ids: Vec<ChannelId>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EdgeResults<T> {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub results: Vec<T>,
    #[serde(default)]
    pub failed_ids: Vec<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Emoji {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub updated: Option<u64>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
