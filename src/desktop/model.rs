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

#[derive(Debug, Clone, PartialEq)]
pub struct ProfileHoverVm {
    pub user_id: String,
    pub x: f64,
    pub y: f64,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonStyle {
    #[default]
    Default,
    Primary,
    Danger,
}

impl ButtonStyle {
    pub fn from_block(style: Option<&str>) -> Self {
        match style {
            Some("primary") => Self::Primary,
            Some("danger") => Self::Danger,
            _ => Self::Default,
        }
    }

    pub fn class(self) -> &'static str {
        match self {
            Self::Default => "block-button",
            Self::Primary => "block-button primary",
            Self::Danger => "block-button danger",
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
    /// A custom workspace emoji: an image sized to the text, not an attachment.
    EmojiImage {
        id: MediaAssetId,
        name: String,
    },
    Media {
        /// What is painted inline: a thumbnail, or a video's poster frame.
        /// `None` for a file with no preview — a PDF, or a clip Slack has not
        /// finished transcoding — which renders as a chip instead.
        id: Option<MediaAssetId>,
        /// The original, registered but not downloaded until the viewer opens
        /// it. `None` when the preview already is the full thing.
        full: Option<MediaAssetId>,
        name: String,
        mime: String,
        /// Intrinsic size of the preview, so the row holds its height before
        /// the bytes land.
        size: Option<(u32, u32)>,
    },
    /// Block Kit `divider`.
    Divider,
    /// Block Kit `header` — a bold standalone line.
    Header(Vec<RichNode>),
    /// Block Kit `context` — a small muted row of 20px images and text.
    Context(Vec<RichNode>),
    /// Block Kit `section`, with optional two-column `fields` and an accessory.
    Section {
        text: Vec<RichNode>,
        fields: Vec<Vec<RichNode>>,
        accessory: Option<Box<RichNode>>,
    },
    /// Block Kit `actions` — a wrapping row of interactive elements.
    Actions(Vec<RichNode>),
    /// Block Kit `button`. Only `url` buttons can act without a Slack backend
    /// round-trip; the rest render disabled so the layout still matches.
    Button {
        label: String,
        url: Option<String>,
        style: ButtonStyle,
    },
    /// A small inline image (context element or section accessory).
    InlineImage {
        id: MediaAssetId,
        alt: String,
    },
    /// Block Kit `image` block — optional title above a bounded image. Slack
    /// draws these at their declared intrinsic size, not full body width.
    ImageBlock {
        id: MediaAssetId,
        alt: String,
        title: Option<String>,
        size: Option<(u32, u32)>,
    },
    /// `rich_text_list`, ordered or bulleted, with Slack's nesting indent.
    List {
        ordered: bool,
        indent: u8,
        offset: u32,
        items: Vec<Vec<RichNode>>,
    },
}

impl RichNode {
    /// Flatten to plain text: message previews, clipboard copy, and the edit
    /// composer all need the same reading of a node tree.
    pub fn plain_text(&self) -> String {
        let mut output = String::new();
        self.write_plain(&mut output);
        output.trim_end().to_owned()
    }

    fn write_plain(&self, output: &mut String) {
        let block = |children: &[RichNode], output: &mut String| {
            for child in children {
                child.write_plain(output);
            }
            output.push('\n');
        };
        match self {
            Self::Text(text) | Self::StyledText { text, .. } | Self::Code(text) => {
                output.push_str(text)
            }
            Self::Link { label, .. }
            | Self::UserMention { label, .. }
            | Self::ChannelMention { label, .. }
            | Self::Button { label, .. } => output.push_str(label),
            Self::Emoji { glyph, .. } => output.push_str(glyph),
            Self::EmojiImage { name, .. } => output.push_str(name),
            Self::Media { name, .. } => output.push_str(name),
            Self::InlineImage { alt, .. } => output.push_str(alt),
            Self::ImageBlock { title, alt, .. } => {
                output.push_str(title.as_deref().unwrap_or(alt.as_str()));
                output.push('\n');
            }
            Self::Paragraph(children)
            | Self::Quote(children)
            | Self::Header(children)
            | Self::Actions(children) => block(children, output),
            Self::Context(children) => {
                for child in children {
                    child.write_plain(output);
                    output.push(' ');
                }
                output.push('\n');
            }
            Self::Section {
                text,
                fields,
                accessory,
            } => {
                block(text, output);
                for field in fields {
                    block(field, output);
                }
                if let Some(accessory) = accessory {
                    accessory.write_plain(output);
                }
            }
            Self::List { items, .. } => {
                for item in items {
                    block(item, output);
                }
            }
            Self::Divider => output.push('\n'),
        }
    }
}

/// One reaction pill: a resolved glyph, or a workspace custom-emoji image.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReactionVm {
    pub name: String,
    pub count: u32,
    pub own: bool,
    /// Unicode glyph when the shortcode is standard, else `None`.
    pub glyph: Option<String>,
    /// Custom workspace emoji image, when one resolved.
    pub media: Option<MediaAssetId>,
}

impl ReactionVm {
    /// Text shown when neither a glyph nor an image resolved.
    pub fn fallback(&self) -> String {
        format!(":{}:", self.name)
    }
}

/// The author line of a message attachment (Slack unfurl or bot embed).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AttachmentAuthorVm {
    pub name: String,
    pub user_id: Option<String>,
    pub icon: Option<MediaAssetId>,
    pub link: Option<String>,
}

/// The footer of a shared-message unfurl: origin channel, stamp, permalink.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AttachmentFooterVm {
    /// e.g. `From a thread in` / `Posted in`, or a bot-supplied footer string.
    pub lead: String,
    pub channel_id: Option<String>,
    pub channel_label: Option<String>,
    pub stamp: Option<String>,
    /// `View reply` / `View message`, paired with the permalink.
    pub permalink: Option<(String, String)>,
    pub icon: Option<MediaAssetId>,
}

/// A message attachment: Slack message unfurls, link unfurls, app unfurls, and
/// legacy bot attachments all project onto this one shape.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AttachmentVm {
    pub key: String,
    /// CSS color for the 4px left bar, when the attachment sets one.
    pub color: Option<String>,
    pub service: Option<String>,
    pub service_icon: Option<MediaAssetId>,
    pub author: Option<AttachmentAuthorVm>,
    pub pretext: Vec<RichNode>,
    pub title: Option<String>,
    pub title_link: Option<String>,
    pub body: Vec<RichNode>,
    pub fields: Vec<(String, Vec<RichNode>, bool)>,
    /// Large preview image plus its intrinsic size, for aspect-correct layout.
    pub image: Option<(MediaAssetId, Option<(u32, u32)>)>,
    pub thumb: Option<MediaAssetId>,
    pub files: Vec<RichNode>,
    pub footer: Option<AttachmentFooterVm>,
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
    pub reactions: Vec<ReactionVm>,
    pub attachments: Vec<AttachmentVm>,
    pub reply_count: u32,
    /// Avatars of the repliers shown on the thread reply bar.
    pub reply_avatars: Vec<(String, Option<MediaAssetId>, String)>,
    /// Relative stamp for the newest reply (`14h ago`).
    pub last_reply: Option<String>,
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
    pub unread_count: u32,
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

/// What the rail says about the link to Slack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectionStatus {
    /// A live realtime socket and a transport that is landing its requests.
    #[default]
    Online,
    /// The machine has a route out; Slack is simply not answering yet.
    Connecting,
    /// No route off the machine at all — a closed lid, a dropped Wi-Fi link.
    NoNetwork,
}

impl ConnectionStatus {
    /// Hover text on the rail indicator.
    pub fn label(self) -> &'static str {
        match self {
            Self::Online => "Connected to Slack",
            Self::Connecting => "Connecting…",
            Self::NoNetwork => "Waiting for network",
        }
    }

    /// The agent protocol's name for this state.
    pub fn key(self) -> &'static str {
        match self {
            Self::Online => "online",
            Self::Connecting => "connecting",
            Self::NoNetwork => "no_network",
        }
    }
}

/// The rail's connection indicator, and the bookkeeping that keeps it honest.
///
/// `status` is deliberately slow to leave `Online`: a socket that reconnects
/// inside the grace window must not blink a spinner at the reader, and a boot
/// that lands normally must never show one at all.
#[derive(Debug, Clone)]
pub struct ConnectionVm {
    pub status: ConnectionStatus,
    /// When the shell first noticed it was not live, or `None` while it is.
    pub unstable_since: Option<std::time::Instant>,
    /// Whether the machine has a route off itself at all. Polled on a cadence
    /// rather than inferred from a failed request: switching Wi-Fi off is not a
    /// request failure, and nothing else notices it for tens of seconds.
    pub routable: bool,
    pub routed_at: Option<std::time::Instant>,
    /// A reachability probe is in flight; ticks must not pile more on top of it.
    pub probing: bool,
    pub probed_at: Option<std::time::Instant>,
}

impl Default for ConnectionVm {
    fn default() -> Self {
        Self {
            status: ConnectionStatus::Online,
            unstable_since: None,
            routable: true,
            routed_at: None,
            probing: false,
            probed_at: None,
        }
    }
}

impl ConnectionVm {
    /// What the rail should paint, or `None` while the link is healthy.
    pub fn indicator(&self) -> Option<ConnectionStatus> {
        (self.status != ConnectionStatus::Online).then_some(self.status)
    }
}

/// A transient status line. Toasts carry their own age so a failure the reader
/// has already read cannot sit on the window for the rest of the session.
#[derive(Debug, Clone)]
pub struct ToastVm {
    pub text: String,
    pub shown_at: std::time::Instant,
}

impl ToastVm {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            shown_at: std::time::Instant::now(),
        }
    }
}

/// The signed-in user as the rail paints them: avatar, presence, and whether
/// notifications are snoozed right now.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SelfAccountVm {
    pub user_id: String,
    pub name: String,
    pub avatar: Option<MediaAssetId>,
    pub initials: String,
    pub presence: PresenceVm,
    pub snoozed: bool,
    pub workspace_name: String,
}

impl SelfAccountVm {
    /// Matches the real client's tooltip: "Active", "Away, notifications
    /// snoozed", and so on.
    pub fn status_label(&self) -> String {
        let presence = match self.presence {
            PresenceVm::Active => "Active",
            PresenceVm::Away | PresenceVm::Unknown => "Away",
        };
        if self.snoozed {
            format!("{presence}, notifications snoozed")
        } else {
            presence.to_owned()
        }
    }
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
    SelfMenu,
    Palette,
    Search,
    Settings,
    Viewer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsSection {
    #[default]
    Appearance,
    Storage,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StorageUsageVm {
    pub avatars: u64,
    pub emoji: u64,
    pub icons: u64,
    pub other: u64,
    pub workspace: u64,
}

impl StorageUsageVm {
    pub fn pictures(self) -> u64 {
        self.avatars + self.emoji + self.icons + self.other
    }

    pub fn total(self) -> u64 {
        self.pictures() + self.workspace
    }

    pub fn kind(self, kind: super_platinum_core::MediaCacheKind) -> u64 {
        match kind {
            super_platinum_core::MediaCacheKind::Avatars => self.avatars,
            super_platinum_core::MediaCacheKind::Emoji => self.emoji,
            super_platinum_core::MediaCacheKind::Icons => self.icons,
            super_platinum_core::MediaCacheKind::Other => self.other,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StoragePanel {
    pub usage: StorageUsageVm,
    pub selected: [bool; 4],
    pub scanning: bool,
    /// Offline fixtures inject canned numbers; live scans must not overwrite them.
    pub fixture: bool,
}

impl Default for StoragePanel {
    fn default() -> Self {
        Self {
            usage: StorageUsageVm::default(),
            selected: [true; 4],
            scanning: false,
            fixture: false,
        }
    }
}

impl StoragePanel {
    pub fn selected_picture_bytes(&self) -> u64 {
        super_platinum_core::MediaCacheKind::ALL
            .iter()
            .map(|kind| {
                if self.selected[kind.index()] {
                    self.usage.kind(*kind)
                } else {
                    0
                }
            })
            .sum()
    }
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
