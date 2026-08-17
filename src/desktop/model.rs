use super_platinum_core::MediaAssetId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainView {
    Home,
    Unreads,
    Threads,
    Activity,
    Dms,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActivityTab {
    #[default]
    All,
    Dms,
    Mentions,
    Threads,
    Reactions,
}

impl ActivityTab {
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Dms => "DMs",
            Self::Mentions => "Mentions",
            Self::Threads => "Threads",
            Self::Reactions => "Reactions",
        }
    }

    pub fn matches(self, item: &super_platinum_core::slack::models::ActivityItem) -> bool {
        match self {
            Self::All => true,
            Self::Dms => matches!(item.item.kind.as_str(), "dm" | "bot_dm_bundle"),
            Self::Mentions => matches!(
                item.item.kind.as_str(),
                "mention"
                    | "at_user"
                    | "at_channel"
                    | "at_everyone"
                    | "at_user_group"
                    | "keyword"
                    | "unjoined_channel_mention"
            ),
            Self::Threads => item.is_thread(),
            Self::Reactions => item.item.kind == "message_reaction",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RichNode {
    Text(String),
    StyledText {
        text: String,
        bold: bool,
        italic: bool,
        strike: bool,
        code: bool,
    },
    Link {
        label: String,
        url: String,
    },
    UserMention {
        user_id: String,
        label: String,
    },
    ChannelMention {
        channel_id: String,
        label: String,
    },
    Code(String),
    Paragraph(Vec<RichNode>),
    Quote(Vec<RichNode>),
    Emoji {
        name: String,
        glyph: String,
    },
    Media {
        id: MediaAssetId,
        name: String,
        mime: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MessageVm {
    pub id: String,
    pub ts: String,
    pub user_id: Option<String>,
    pub author: String,
    pub timestamp: String,
    pub avatar_initials: String,
    pub avatar: Option<MediaAssetId>,
    pub body: Vec<RichNode>,
    pub edited: bool,
    pub is_own: bool,
    pub is_app: bool,
    pub pending: bool,
    pub compact: bool,
    pub date_label: Option<String>,
    pub show_unread_divider: bool,
    pub reactions: Vec<(String, u32, bool)>,
    pub reply_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PresenceVm {
    Active,
    Away,
    #[default]
    Unknown,
}

impl PresenceVm {
    pub fn from_core(presence: super_platinum_core::state::Presence) -> Self {
        match presence {
            super_platinum_core::state::Presence::Active => Self::Active,
            super_platinum_core::state::Presence::Away => Self::Away,
            super_platinum_core::state::Presence::Unknown => Self::Unknown,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Active => "Active",
            Self::Away => "Away",
            Self::Unknown => "Offline",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChannelVm {
    pub id: String,
    pub name: String,
    pub unread: bool,
    pub mention_count: u32,
    pub is_im: bool,
    pub is_mpim: bool,
    pub is_private: bool,
    pub is_starred: bool,
    pub is_ext_shared: bool,
    pub member_count: Option<usize>,
    pub avatar: Option<MediaAssetId>,
    pub avatar_initials: String,
    pub presence: PresenceVm,
    pub topic: Option<String>,
    pub user_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SidebarSectionVm {
    pub id: String,
    pub kind: String,
    pub title: String,
    /// Indices into `ShellState::channels` for rows in this section.
    pub channel_indices: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceVm {
    pub id: String,
    pub name: String,
    pub initials: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountVm {
    pub id: String,
    pub label: String,
    pub active: bool,
}

pub struct PendingSend {
    pub team: String,
    pub channel: String,
    pub thread_ts: Option<String>,
    pub text: String,
    pub client_msg_id: String,
    pub optimistic_ts: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResultVm {
    pub channel_id: String,
    pub channel_name: String,
    pub ts: String,
    pub author: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewerVm {
    pub id: MediaAssetId,
    pub name: String,
    pub mime: String,
}

#[derive(Debug, Clone, Default)]
pub struct PerformanceVm {
    pub scroll_frame_ms: Vec<f64>,
    pub channel_switch_ms: Option<f64>,
    pub realtime_insert_ms: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    Accounts,
    Palette,
    Search,
    Settings,
    Profile,
    Viewer,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[allow(dead_code)]
pub struct ProfileVm {
    pub user_id: String,
    pub name: String,
    pub title: String,
    pub status_text: String,
    pub status_emoji: String,
    pub email: String,
    pub pronouns: String,
    pub local_time: String,
    pub presence: PresenceVm,
    pub avatar: Option<MediaAssetId>,
    pub avatar_initials: String,
    pub deactivated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub struct DmRowVm {
    pub id: String,
    pub name: String,
    pub snippet: String,
    pub timestamp: String,
    pub unread: u32,
    pub avatar: Option<MediaAssetId>,
    pub avatar_initials: String,
    pub presence: PresenceVm,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub struct ActivityRowVm {
    pub key: String,
    pub title: String,
    pub subtitle: String,
    pub preview: String,
    pub timestamp: String,
    pub unread: bool,
    pub channel_id: Option<String>,
    pub ts: Option<String>,
    pub thread_ts: Option<String>,
    pub avatar: Option<MediaAssetId>,
    pub avatar_initials: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub struct UnreadRowVm {
    pub channel_id: String,
    pub name: String,
    pub unread: u32,
    pub snippets: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub struct ThreadFeedRowVm {
    pub channel_id: String,
    pub root_ts: String,
    pub author: String,
    pub preview: String,
    pub reply_count: usize,
    pub avatar: Option<MediaAssetId>,
    pub avatar_initials: String,
    pub timestamp: String,
}
