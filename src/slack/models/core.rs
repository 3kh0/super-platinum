use super::*;

pub type TeamId = String;
pub type ChannelId = String;
pub type UserId = String;
pub type MessageTs = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub ok: bool,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(flatten)]
    pub body: T,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResponseMetadata {
    #[serde(default)]
    pub next_cursor: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HistoryPage {
    #[serde(default)]
    pub messages: Vec<Message>,
    #[serde(default)]
    pub has_more: bool,
    #[serde(default)]
    pub unchanged_messages: Vec<MessageTs>,
    #[serde(default)]
    pub latest_updates: BTreeMap<MessageTs, String>,
    #[serde(default)]
    pub pin_count: Option<u32>,
    #[serde(default)]
    pub response_metadata: Option<ResponseMetadata>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CountsPage {
    #[serde(default)]
    pub channels: Vec<Channel>,
    #[serde(default)]
    pub ims: Vec<Channel>,
    #[serde(default)]
    pub groups: Vec<Channel>,
    #[serde(default)]
    pub mpims: Vec<Channel>,
    #[serde(default)]
    pub activity_v2: Option<BTreeMap<String, u32>>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SidebarDmsPage {
    #[serde(default)]
    pub ims: Vec<Channel>,
    #[serde(default)]
    pub mpdms: Vec<Channel>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl SidebarDmsPage {
    pub fn all_channels(&self) -> Vec<Channel> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for channel in self.ims.iter().chain(&self.mpdms) {
            if seen.insert(channel.id.clone()) {
                out.push(channel.clone());
            }
        }
        out
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClientDmsPage {
    #[serde(default)]
    pub dms: Vec<DmEntry>,
    #[serde(default)]
    pub response_metadata: Option<ResponseMetadata>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DmEntry {
    pub id: ChannelId,
    #[serde(default)]
    pub latest: Option<MessageTs>,
    #[serde(default)]
    pub message: Option<Message>,
    #[serde(default)]
    pub channel: Option<Channel>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl CountsPage {
    pub fn activity_unread_count(&self) -> Option<u32> {
        self.activity_v2
            .as_ref()
            .map(|counts| counts.values().copied().fold(0, u32::saturating_add))
    }

    pub fn all_channels(&self) -> Vec<Channel> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for channel in self
            .channels
            .iter()
            .chain(&self.groups)
            .chain(&self.ims)
            .chain(&self.mpims)
        {
            if seen.insert(channel.id.clone()) {
                out.push(channel.clone());
            }
        }
        out
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Message {
    #[serde(default)]
    pub user: Option<UserId>,
    #[serde(default)]
    pub bot_id: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub bot_profile: Option<BotProfile>,
    #[serde(default)]
    pub icons: Option<MessageIcons>,
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub subtype: Option<String>,
    #[serde(default)]
    pub ts: Option<MessageTs>,
    #[serde(default)]
    pub client_msg_id: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub team: Option<TeamId>,
    #[serde(default)]
    pub channel: Option<ChannelId>,
    #[serde(default)]
    pub thread_ts: Option<MessageTs>,
    #[serde(default)]
    pub parent_user_id: Option<UserId>,
    #[serde(default)]
    pub reply_count: Option<u32>,
    #[serde(default)]
    pub reply_users_count: Option<u32>,
    #[serde(default)]
    pub latest_reply: Option<MessageTs>,
    #[serde(default)]
    pub reply_users: Vec<UserId>,
    #[serde(default)]
    pub reactions: Vec<Reaction>,
    #[serde(default)]
    pub blocks: Vec<Value>,
    #[serde(default)]
    pub files: Vec<File>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    #[serde(default)]
    pub edited: Option<Value>,
    #[serde(default)]
    pub message: Option<Box<Message>>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BotProfile {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub user_id: Option<UserId>,
    #[serde(default)]
    pub icons: Option<MessageIcons>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessageIcons {
    #[serde(default)]
    pub image_36: Option<String>,
    #[serde(default)]
    pub image_48: Option<String>,
    #[serde(default)]
    pub image_64: Option<String>,
    #[serde(default)]
    pub image_72: Option<String>,
    #[serde(default)]
    pub image_192: Option<String>,
    #[serde(default)]
    pub image_512: Option<String>,
    #[serde(default)]
    pub image_original: Option<String>,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Attachment {
    #[serde(default)]
    pub id: Option<u64>,
    #[serde(default)]
    pub service_name: Option<String>,
    #[serde(default)]
    pub service_icon: Option<String>,
    #[serde(default)]
    pub author_name: Option<String>,
    #[serde(default)]
    pub author_link: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub title_link: Option<String>,
    #[serde(default)]
    pub pretext: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub footer: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub image_url: Option<String>,
    #[serde(default)]
    pub thumb_url: Option<String>,
    #[serde(default)]
    pub from_url: Option<String>,
    #[serde(default)]
    pub original_url: Option<String>,
    #[serde(default)]
    pub fields: Vec<AttachmentField>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AttachmentField {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub short: bool,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SentMessage {
    pub channel: ChannelId,
    pub ts: MessageTs,
    pub message: Message,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Reaction {
    pub name: String,
    #[serde(default)]
    pub users: Vec<UserId>,
    #[serde(default)]
    pub count: u32,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct File {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub mimetype: Option<String>,
    #[serde(default)]
    pub filetype: Option<String>,
    #[serde(default)]
    pub pretty_type: Option<String>,
    #[serde(default)]
    pub url_private: Option<String>,
    #[serde(default)]
    pub thumb_64: Option<String>,
    #[serde(default)]
    pub thumb_80: Option<String>,
    #[serde(default)]
    pub thumb_160: Option<String>,
    #[serde(default)]
    pub thumb_360: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub is_external: Option<bool>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
