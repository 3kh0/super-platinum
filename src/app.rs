use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use iced::widget::image::Handle as ImageHandle;
use iced::widget::text_editor::{self, Content};
use iced::{Point, Task, Vector};

use crate::cache::Cache;
use crate::config::{self, Session};
use crate::slack::api::{self, HistoryArgs};
use crate::slack::events::RtEvent;
use crate::slack::models::{
    ActivityFeedPage, BootData, Channel, ChannelId, ChannelSectionsPage, ClientDmsPage, CountsPage,
    Emoji, HistoryPage, Message as SlackMessage, MessageTs, MessagesListPage, ProfileExtrasPage,
    SearchMessagesPage, SentMessage, SidebarDmsPage, TeamId, TeamProfileField, ThreadsViewPage,
    User, UserId, UserProfile,
};
use crate::slack::realtime::Connection;
use crate::slack::{Error as SlackError, SlackClient, Transport};
use crate::state::{ChannelMessages, Screen, Toast, Workspace};
use crate::ui;

mod agent;
mod message;
mod model;
mod runtime;

pub use message::{
    AttachTarget, ComposerTarget, ConversationMessage, DiscoveryMessage, FormatMark, Message,
    RuntimeMessage, WorkspaceMessage,
};
use model::{
    ActiveThreadKey, DesktopNotification, PendingScrollTarget, ReadTarget, ThreadKey,
    thread_replies_args,
};
pub use model::{
    ActivityState, ComposerAttachment, DmsState, FilePreview, HistoryLoadKind, ImageFetchAuth,
    ImageViewerImage, ImageViewerSource, ImageViewerState, LoadedHistory, MediaViewerKind,
    PendingFileMessage, PreparedVideo, ProfileHoverState, ProfilePaneState, SearchHit, SearchState,
    TextSelection, TextSelectionPoint, TextSelectionSurface, ThreadsState, UnreadsSort,
    UnreadsState, VideoViewerPlayback,
};
use runtime::merge_history_pages;
pub use runtime::run;

mod palette;
mod subscription;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod ui_visual;
mod update;
mod view;

pub use palette::{PaletteEntry, PaletteState, PaletteTarget};
use subscription::subscription;
use update::{preferred_channel, update};
use view::view;

pub struct App {
    screen: Screen,
    accounts: BTreeMap<config::AccountId, Session>,
    active_account: Option<config::AccountId>,
    account_epoch: u64,
    session: Option<Session>,
    cache: Option<Cache>,
    client: SlackClient,
    transport: Option<Arc<Transport>>,
    active_team: Option<TeamId>,
    active_channel: Option<ChannelId>,
    active_thread: Option<ActiveThreadKey>,
    thread_open: bool,
    main_view: crate::state::MainView,
    unreads: UnreadsState,
    threads_view: ThreadsState,
    activity: ActivityState,
    dms: DmsState,
    workspaces: BTreeMap<TeamId, Workspace>,
    threads: HashMap<ThreadKey, ChannelMessages>,
    composer: Content,
    thread_composer: Content,
    composer_attachments: Vec<ComposerAttachment>,
    thread_composer_attachments: Vec<ComposerAttachment>,
    pending_file_messages: Vec<PendingFileMessage>,
    attachment_seq: u64,
    editing: Option<(ChannelId, MessageTs)>,
    edit_content: Content,
    hovered_message: Option<(bool, MessageTs)>,
    profile_pane: Option<ProfilePaneState>,
    profile_open: bool,
    profile_hover: Option<ProfileHoverState>,
    profile_generation: u64,
    profile_fields: HashMap<TeamId, Vec<TeamProfileField>>,
    cursor_position: Option<Point>,
    text_selection: Option<TextSelection>,
    search_input: String,
    search: Option<SearchState>,
    palette: Option<PaletteState>,
    palette_open: bool,
    errors: Vec<Toast>,
    send_seq: u64,
    last_typing: HashMap<(TeamId, ChannelId), Instant>,
    last_active_channels: HashMap<TeamId, ChannelId>,
    file_previews: HashMap<String, FilePreview>,
    image_viewer: Option<ImageViewerState>,
    image_viewer_generation: u64,
    avatar_previews: HashMap<UserId, FilePreview>,
    profile_previews: HashMap<UserId, FilePreview>,
    emoji_previews: HashMap<String, FilePreview>,
    emoji_animation_started: Instant,
    emoji_hydrated: HashSet<(TeamId, String)>,
    channel_hydrated: HashSet<(TeamId, ChannelId)>,
    avatar_profile_hydrated: HashSet<UserId>,
    pending_scroll_to: Option<(ChannelId, PendingScrollTarget)>,
    thread_unread_marker: Option<(ThreadKey, MessageTs)>,
    chat_paused: HashMap<ChannelId, u32>,
    pending_marks: HashSet<(ReadTarget, MessageTs)>,
    mark_blocked: HashSet<ReadTarget>,
    cache_dirty: HashMap<TeamId, Instant>,
    cache_saving: HashMap<TeamId, Instant>,
    settings: config::Settings,
    settings_color_drafts: HashMap<config::ColorRole, String>,
    settings_color_errors: HashMap<config::ColorRole, String>,
    show_settings: bool,
    settings_open: bool,
    show_account_menu: bool,
    account_menu_open: bool,
    sidebar_resizing: bool,
    sidebar_resize_prev_x: Option<f32>,
    scrollbar_visible_until: Option<Instant>,
}
