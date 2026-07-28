use super::*;

pub(super) type ActiveThreadKey = (ChannelId, MessageTs);
pub(super) type ThreadKey = (TeamId, ChannelId, MessageTs);
pub(super) fn thread_replies_args(
    channel: ChannelId,
    root_ts: MessageTs,
    unread_range: Option<(MessageTs, MessageTs)>,
) -> (api::RepliesArgs, Option<MessageTs>) {
    let unread_anchor = unread_range.as_ref().map(|(oldest, _)| oldest.clone());
    let bounded = unread_anchor.is_some();
    let latest = unread_range.map(|(_, latest)| latest);
    (
        api::RepliesArgs {
            channel,
            ts: root_ts,
            latest,
            limit: bounded.then_some(200),
            inclusive: bounded,
            ..Default::default()
        },
        unread_anchor,
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextSelectionSurface {
    Channel {
        channel: ChannelId,
    },
    Thread {
        channel: ChannelId,
        root_ts: MessageTs,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSelectionPoint {
    pub surface: TextSelectionSurface,
    pub message_ts: MessageTs,
    pub message_index: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSelection {
    pub anchor: TextSelectionPoint,
    pub focus: TextSelectionPoint,
    pub dragging: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PendingScrollTarget {
    Message(MessageTs),
    FirstUnreadAfter(MessageTs),
    Latest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryLoadKind {
    Latest,
    Since,
    Around,
    Older,
}

#[derive(Debug, Clone)]
pub struct LoadedHistory {
    pub page: HistoryPage,
    pub replace_cached: bool,
}

#[derive(Debug, Clone)]
pub enum FilePreview {
    Loading,
    Loaded(ImageHandle),
    Animated {
        frames: Vec<ImageHandle>,
        delays: Vec<Duration>,
        total: Duration,
    },
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFetchAuth {
    Slack,
    Public,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaViewerKind {
    Image,
    Video,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImageViewerSource {
    pub kind: MediaViewerKind,
    pub preview_key: String,
    pub full_url: String,
    pub download_url: String,
    pub fetch_auth: ImageFetchAuth,
    pub filename: String,
    pub author_name: String,
    pub avatar_key: Option<String>,
    pub timestamp: String,
    pub conversation: String,
}

#[derive(Debug, Clone)]
pub enum ImageViewerImage {
    Loading,
    Loaded(ImageHandle),
    Failed,
}

#[derive(Debug, Clone)]
pub struct PreparedVideo {
    pub path: PathBuf,
    pub player: Arc<iced_video_player::Video>,
}

#[derive(Debug, Clone)]
pub struct VideoViewerPlayback {
    pub path: Option<PathBuf>,
    pub player: Option<Arc<iced_video_player::Video>>,
    pub duration: f32,
    pub position: f32,
    pub playing: bool,
    pub seeking: bool,
    pub resume_after_seek: bool,
    pub volume: f32,
    pub muted: bool,
}

impl Default for VideoViewerPlayback {
    fn default() -> Self {
        Self {
            path: None,
            player: None,
            duration: 0.0,
            position: 0.0,
            playing: false,
            seeking: false,
            resume_after_seek: false,
            volume: 1.0,
            muted: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImageViewerState {
    pub source: ImageViewerSource,
    pub image: ImageViewerImage,
    pub generation: u64,
    pub open: bool,
    pub zoom: f32,
    pub offset: Vector,
    pub video: Option<VideoViewerPlayback>,
}

#[derive(Debug, Clone)]
pub(super) struct DesktopNotification {
    pub(super) title: String,
    pub(super) body: String,
}

#[derive(Debug, Clone, Default)]
pub struct ActivityState {
    pub items: Vec<crate::slack::models::ActivityItem>,
    pub hydrated: HashMap<(ChannelId, MessageTs), SlackMessage>,
    pub next_cursor: Option<String>,
    pub load_seq: u64,
    pub loading: bool,
    pub loaded: bool,
    pub selected: Option<String>,
    pub unread_only: bool,
}

impl ActivityState {
    pub fn upsert(&mut self, item: crate::slack::models::ActivityItem) {
        let identity = item.identity();
        if let Some(existing) = self.items.iter_mut().find(|i| i.identity() == identity) {
            if crate::state::cmp_ts(Some(&item.feed_ts), Some(&existing.feed_ts)).is_lt() {
                return;
            }
            if self.selected.as_deref() == Some(existing.key.as_str()) {
                self.selected = Some(item.key.clone());
            }
            *existing = item;
        } else {
            self.items.push(item);
        }
        self.items
            .sort_by(|a, b| crate::state::cmp_ts(Some(&b.feed_ts), Some(&a.feed_ts)));
    }
}

#[derive(Debug, Clone, Default)]
pub struct DmsState {
    pub entries: Vec<crate::slack::models::DmEntry>,
    pub next_cursor: Option<String>,
    pub load_seq: u64,
    pub loading: bool,
    pub loaded: bool,
    pub unread_only: bool,
    pub filter: String,
}

impl DmsState {
    pub fn upsert(&mut self, entry: crate::slack::models::DmEntry) {
        if let Some(existing) = self.entries.iter_mut().find(|e| e.id == entry.id) {
            if crate::state::cmp_ts(entry.latest.as_deref(), existing.latest.as_deref()).is_lt() {
                return;
            }
            let channel = entry.channel.clone().or_else(|| existing.channel.take());
            *existing = entry;
            existing.channel = channel;
        } else {
            self.entries.push(entry);
        }
        self.sort();
    }

    pub fn touch(&mut self, channel: &str, message: SlackMessage) {
        let Some(ts) = message.ts.clone() else {
            return;
        };
        if let Some(existing) = self.entries.iter_mut().find(|e| e.id == channel) {
            if crate::state::cmp_ts(Some(&ts), existing.latest.as_deref()).is_lt() {
                return;
            }
            existing.latest = Some(ts);
            existing.message = Some(message);
        } else {
            self.entries.push(crate::slack::models::DmEntry {
                id: channel.to_owned(),
                latest: Some(ts),
                message: Some(message),
                ..Default::default()
            });
        }
        self.sort();
    }

    fn sort(&mut self) {
        self.entries
            .sort_by(|a, b| crate::state::cmp_ts(b.latest.as_deref(), a.latest.as_deref()));
    }
}

#[derive(Debug, Clone)]
pub struct SearchHit {
    pub channel: ChannelId,
    pub channel_label: String,
    pub message: SlackMessage,
}

#[derive(Debug, Clone)]
pub struct SearchState {
    pub query: String,
    pub team: TeamId,
    pub page: u32,
    pub page_count: u32,
    pub total: u64,
    pub hits: Vec<SearchHit>,
    pub loading: bool,
}

#[derive(Debug, Clone)]
pub struct ComposerAttachment {
    pub id: u64,
    pub path: PathBuf,
    pub name: String,
    pub bytes: u64,
    pub uploading: bool,
    pub upload_started: Option<Instant>,
    pub upload_cancel: Option<Arc<AtomicBool>>,
    pub upload_progress: Option<Arc<AtomicU64>>,
    pub preview_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct PendingFileMessage {
    pub team: TeamId,
    pub channel: ChannelId,
    pub thread_ts: Option<MessageTs>,
    pub message_ts: MessageTs,
    pub client_msg_id: String,
    pub text: String,
    pub attachments: Vec<ComposerAttachment>,
}

#[derive(Debug, Clone)]
pub struct ProfilePaneState {
    pub user: UserId,
    pub loading: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProfileHoverState {
    pub user: UserId,
    pub key: String,
    pub generation: u64,
    pub visible: bool,
    pub source_hovered: bool,
    pub card_hovered: bool,
    pub position: Option<Point>,
}
