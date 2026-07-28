use super::*;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Channel {
    pub id: ChannelId,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub is_channel: bool,
    #[serde(default)]
    pub is_group: bool,
    #[serde(default)]
    pub is_im: bool,
    #[serde(default)]
    pub is_mpim: bool,
    #[serde(default)]
    pub is_private: bool,
    #[serde(default)]
    pub is_archived: bool,
    #[serde(default)]
    pub is_starred: bool,
    #[serde(default)]
    pub is_ext_shared: bool,
    #[serde(default, deserialize_with = "deserialize_optional_u64")]
    pub updated: Option<u64>,
    #[serde(default)]
    pub user: Option<UserId>,
    #[serde(default)]
    pub unread_count: Option<u32>,
    #[serde(default)]
    pub unread_count_display: Option<u32>,
    #[serde(default)]
    pub mention_count: Option<u32>,
    #[serde(default)]
    pub has_unreads: bool,
    #[serde(default)]
    pub last_read: Option<MessageTs>,
    #[serde(default)]
    pub previous_names: Vec<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// A huddle/call "room", as delivered on `sh_room_*` realtime frames and the
/// `huddle_thread` system message. Only the fields snack uses for awareness +
/// join-handoff are named; the rest are preserved in `extra`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Room {
    pub id: String,
    #[serde(default)]
    pub call_family: Option<String>,
    #[serde(default)]
    pub channels: Vec<ChannelId>,
    #[serde(default)]
    pub created_by: Option<UserId>,
    #[serde(default)]
    pub date_start: Option<i64>,
    #[serde(default)]
    pub date_end: Option<i64>,
    #[serde(default)]
    pub has_ended: bool,
    #[serde(default)]
    pub huddle_link: Option<String>,
    #[serde(default)]
    pub participants: Vec<UserId>,
    #[serde(default)]
    pub participant_history: Vec<UserId>,
    #[serde(default)]
    pub media_backend_type: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl Room {
    /// The channel this huddle lives in (huddles are single-channel in practice).
    pub fn channel(&self) -> Option<&ChannelId> {
        self.channels.first()
    }

    /// A huddle is "active" while it has not ended.
    pub fn is_active(&self) -> bool {
        !self.has_ended
    }
}

fn deserialize_optional_u64<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;
    Ok(match value {
        Some(Value::Number(number)) => number.as_u64(),
        Some(Value::String(string)) => string
            .split_once('.')
            .map(|(seconds, _)| seconds)
            .unwrap_or(&string)
            .parse()
            .ok(),
        _ => None,
    })
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Team {
    pub id: TeamId,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub enterprise_id: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BootSelf {
    pub id: UserId,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub profile: Option<UserProfile>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BootData {
    #[serde(rename = "self", default)]
    pub self_user: BootSelf,
    #[serde(default)]
    pub team: Option<Team>,
    #[serde(default)]
    pub channels: Vec<Channel>,
    #[serde(default)]
    pub ims: Vec<Channel>,
    #[serde(default)]
    pub groups: Vec<Channel>,
    #[serde(default)]
    pub mpims: Vec<Channel>,
    #[serde(default)]
    pub users: Vec<User>,
    #[serde(default)]
    pub starred: Vec<ChannelId>,
    #[serde(default)]
    pub channels_priority: BTreeMap<ChannelId, f64>,
    #[serde(default)]
    pub prefs: BootPrefs,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl BootData {
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
pub struct BootPrefs {
    #[serde(default)]
    pub sidebar_behavior: Option<String>,
    #[serde(default)]
    pub priority_sidebar_section: bool,
    #[serde(default)]
    pub vip_users: Option<String>,
    #[serde(default)]
    pub channel_sections: Option<String>,
    #[serde(default)]
    pub team_channel_sections: Option<String>,
    #[serde(default)]
    pub hidden_user_group_sections: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
