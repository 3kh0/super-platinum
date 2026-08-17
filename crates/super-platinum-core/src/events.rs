use std::path::PathBuf;
use std::time::Instant;

use crate::FormatMark;
use crate::agent_protocol::AgentCommand;
use crate::config;
use crate::domain::{
    AttachTarget, ComposerTarget, HistoryLoadKind, ImageFetchAuth, ImageViewerSource,
    LoadedHistory, TextSelectionPoint,
};

#[derive(Debug, Clone)]
pub enum ConversationMessage {
    WorkspaceSelected(TeamId),
    ChannelSelected(ChannelId),
    ComposerFormat {
        target: ComposerTarget,
        mark: FormatMark,
    },
    AttachmentPickerOpened(AttachTarget),
    AttachmentsPicked {
        target: AttachTarget,
        paths: Vec<PathBuf>,
    },
    FilesDropped(Vec<PathBuf>),
    VideoPreviewReady {
        source: PathBuf,
        result: Result<PathBuf, String>,
    },
    AttachmentRemoved {
        target: AttachTarget,
        id: u64,
    },
    PasteAttachmentsRequested(AttachTarget),
    ClipboardFilesRead {
        target: AttachTarget,
        result: Result<Vec<PathBuf>, String>,
    },
    ClipboardTextRead {
        target: AttachTarget,
        result: Result<String, String>,
    },
    AttachmentsSent {
        target: AttachTarget,
        team: TeamId,
        channel: ChannelId,
        thread_ts: Option<MessageTs>,
        message_ts: MessageTs,
        client_msg_id: String,
        result: Result<(), SlackError>,
    },
    SendPressed,
    MessageSent {
        team: TeamId,
        channel: ChannelId,
        client_msg_id: String,
        result: Result<SentMessage, SlackError>,
    },
    ThreadOpened {
        channel: ChannelId,
        ts: MessageTs,
        unread_range: Option<(MessageTs, MessageTs)>,
    },
    ThreadClosed,
    ThreadDismissed,
    ThreadLoaded {
        team: TeamId,
        channel: ChannelId,
        root_ts: MessageTs,
        unread_anchor: Option<MessageTs>,
        result: Result<HistoryPage, SlackError>,
    },
    ThreadMarked {
        team: TeamId,
        channel: ChannelId,
        root_ts: MessageTs,
        ts: MessageTs,
        result: Result<(), SlackError>,
    },
    ThreadSendPressed,
    ThreadReplySent {
        team: TeamId,
        channel: ChannelId,
        root_ts: MessageTs,
        client_msg_id: String,
        result: Result<SentMessage, SlackError>,
    },
}

#[derive(Debug, Clone)]
pub enum DiscoveryMessage {
    SearchInputChanged(String),
    SearchSubmitted,
    SearchCleared,
    SearchPageRequested(u32),
    SearchLoaded {
        team: TeamId,
        query: String,
        page: u32,
        result: Result<SearchMessagesPage, SlackError>,
    },
    SearchResultSelected {
        channel: ChannelId,
        ts: MessageTs,
        thread_ts: Option<MessageTs>,
    },
    PaletteToggled,
    PaletteClosed,
    PaletteDismissed,
    PaletteQueryChanged(String),
    PaletteMoved(isize),
    PaletteSubmitted,
    PaletteEntryPressed(usize),
    PaletteRemoteUsersLoaded {
        team: TeamId,
        seq: u64,
        result: Result<Vec<User>, SlackError>,
    },
    PaletteRemoteChannelsLoaded {
        team: TeamId,
        seq: u64,
        result: Result<Vec<Channel>, SlackError>,
    },
    DmOpened {
        team: TeamId,
        user: UserId,
        result: Result<ChannelId, SlackError>,
    },
    FileDownloadPressed {
        url: String,
        filename: String,
        auth: ImageFetchAuth,
    },
    FileDownloaded(Result<PathBuf, SlackError>),
    ImageViewerOpened(ImageViewerSource),
    ImageViewerFullLoaded {
        generation: u64,
        result: Result<Vec<u8>, SlackError>,
    },
    ImageViewerVideoFrame(u64),
    ImageViewerVideoEnded(u64),
    ImageViewerVideoFailed {
        generation: u64,
        error: String,
    },
    ImageViewerVideoPlayPause,
    ImageViewerVideoSeekChanged(f32),
    ImageViewerVideoSeekReleased,
    ImageViewerVideoVolumeChanged(f32),
    ImageViewerVideoVolumeReleased,
    ImageViewerVideoMuteToggled,
    ImageViewerClosed,
    ImageViewerDismissed,
    ImageViewerZoomChanged(f32),
    ImageViewerDownloadPressed,
    OpenUrl(String),
    UrlOpened(Result<(), String>),
    AvatarLoaded {
        user: UserId,
        result: Result<Vec<u8>, SlackError>,
    },
    UsersLoaded {
        team: TeamId,
        result: Result<Vec<User>, SlackError>,
    },
    EmojisLoaded {
        team: TeamId,
        requested: Vec<String>,
        result: Result<Vec<Emoji>, SlackError>,
    },
    ChannelsLoaded {
        team: TeamId,
        requested: Vec<ChannelId>,
        result: Result<Vec<Channel>, SlackError>,
    },
    DesktopNotificationShown(Result<(), String>),
    CacheSaved {
        team: TeamId,
        started_at: Instant,
        result: Result<(), String>,
    },
}
use crate::slack::Error as SlackError;
use crate::slack::events::RtEvent;
use crate::slack::models::{
    ActivityFeedPage, BootData, Channel, ChannelId, ChannelSectionsPage, ClientDmsPage, CountsPage,
    Emoji, HistoryPage, MessageTs, MessagesListPage, ProfileExtrasPage, SearchMessagesPage,
    SentMessage, SidebarDmsPage, TeamId, TeamProfileField, ThreadsViewPage, User, UserId,
    UserProfile,
};
use crate::slack::realtime::Connection;
use crate::state::{MainView, Presence};
use crate::{CapturedScreenshot, Point};

#[derive(Debug, Clone)]
pub enum WorkspaceMessage {
    BootLoaded(TeamId, Result<BootData, SlackError>),
    CountsLoaded(TeamId, Result<CountsPage, SlackError>),
    SidebarDmsLoaded(TeamId, Result<SidebarDmsPage, SlackError>),
    ChannelSectionsLoaded(TeamId, Result<ChannelSectionsPage, SlackError>),
    HistoryLoaded(
        TeamId,
        ChannelId,
        HistoryLoadKind,
        Result<LoadedHistory, SlackError>,
    ),
    ChannelScrolled {
        channel: ChannelId,
        y: f32,
        bottom_gap: f32,
    },
    ChatResumePressed(ChannelId),
    ChannelMarked(TeamId, ChannelId, MessageTs, Result<(), SlackError>),
    EditPressed {
        channel: ChannelId,
        ts: MessageTs,
    },
    EditSubmit,
    CopyMessage(String),
    TextSelectionStarted(TextSelectionPoint),
    TextSelectionDragged(TextSelectionPoint),
    TextSelectionEnded,
    TextSelectionCopyRequested,
    MessageHovered {
        in_thread: bool,
        ts: MessageTs,
    },
    MessageUnhovered,
    ProfilePressed(UserId),
    ProfileDismissed,
    ProfilePaneDismissed,
    ProfileSeeAllConversations(UserId),
    ProfileHoverEntered {
        user: UserId,
        key: String,
    },
    ProfileHoverExited {
        user: UserId,
        key: String,
    },
    CursorMoved(Point),
    ProfileHoverReady {
        user: UserId,
        key: String,
        generation: u64,
    },
    ProfileHoverDismissReady(u64),
    ProfileCardEntered,
    ProfileCardExited,
    ProfileLoaded {
        team: TeamId,
        user: UserId,
        result: Result<UserProfile, SlackError>,
    },
    ProfileExtrasLoaded {
        team: TeamId,
        user: UserId,
        result: Result<ProfileExtrasPage, SlackError>,
    },
    ProfileFieldsLoaded {
        team: TeamId,
        result: Result<Vec<TeamProfileField>, SlackError>,
    },
    ProfileImageLoaded {
        user: UserId,
        result: Result<Vec<u8>, SlackError>,
    },
    ProfileMessagePressed(UserId),
    EditCancelled,
    MessageEdited {
        team: TeamId,
        channel: ChannelId,
        ts: MessageTs,
        result: Result<SentMessage, SlackError>,
    },
    DeletePressed {
        channel: ChannelId,
        ts: MessageTs,
    },
    MessageDeleted {
        team: TeamId,
        channel: ChannelId,
        ts: MessageTs,
        result: Result<(), SlackError>,
    },
    ReactionPressed {
        channel: ChannelId,
        ts: MessageTs,
        name: String,
    },
    ReactionUpdated {
        team: TeamId,
        channel: ChannelId,
        ts: MessageTs,
        user: String,
        name: String,
        added: bool,
        result: Result<(), SlackError>,
    },
}

/// Renderer-neutral runtime and supervisor events.
#[derive(Debug, Clone)]
pub enum RuntimeMessage {
    // Boxed: `RtEvent` is ~1KB, which would otherwise set the size of every
    // `RuntimeMessage` moved through the dispatcher.
    Realtime(TeamId, u64, Box<RtEvent>),
    RtConnected(TeamId, u64, Connection),
    RtDisconnected(TeamId, u64),
    MainViewSelected(MainView),
    UnreadsScrolled {
        remaining: f32,
    },
    UnreadsSortToggled,
    UnreadsChannelToggled(ChannelId),
    UnreadsChannelFocused(ChannelId),
    UnreadsMarkRead(ChannelId),
    UnreadsMarkFocused,
    UnreadsChannelOpened(ChannelId),
    UnreadsChannelLoaded {
        team: TeamId,
        channel: ChannelId,
        seq: u64,
        result: Result<HistoryPage, SlackError>,
    },
    ThreadsScrolled {
        remaining: f32,
    },
    ThreadsVipSelected(bool),
    ThreadsLoaded {
        team: TeamId,
        max_ts: Option<MessageTs>,
        seq: u64,
        result: Result<ThreadsViewPage, SlackError>,
    },
    ThreadFeedSelected {
        channel: ChannelId,
        root_ts: MessageTs,
        unread_range: Option<(MessageTs, MessageTs)>,
    },
    ActivityScrolled {
        remaining: f32,
    },
    ActivityLoaded {
        team: TeamId,
        cursor: Option<String>,
        seq: u64,
        result: Result<ActivityFeedPage, SlackError>,
    },
    ActivityMessagesLoaded(TeamId, Result<MessagesListPage, SlackError>),
    ActivityUnreadOnlyToggled,
    ActivitySelected(String),
    DmsScrolled {
        remaining: f32,
    },
    DmsLoaded {
        team: TeamId,
        cursor: Option<String>,
        seq: u64,
        result: Result<ClientDmsPage, SlackError>,
    },
    DmsUnreadOnlyToggled(bool),
    DmsFilterChanged(String),
    SignInPressed,
    AddAccountPressed,
    SlackProtocolOpened(String),
    AuthenticationFinished(bool),
    RetryAuth,
    AccountMenuToggled,
    AccountSelected(config::AccountId),
    SelfPresenceSelected(Presence),
    SelfPresenceUpdated {
        team: TeamId,
        presence: Presence,
        previous: Option<Presence>,
        result: Result<(), SlackError>,
    },
    SignOutPressed,
    SettingsOpened,
    SettingsClosed,
    SettingsDismissed,
    SettingsPresetSelected(config::ThemePreset),
    SettingsRoleColorChanged(config::ColorRole, String),
    SettingsPresetColorsRestored,
    SettingsBackgroundPickerOpened,
    SettingsBackgroundPicked(Option<PathBuf>),
    SettingsBackgroundImported(Result<config::BackgroundSettings, String>),
    SettingsBackgroundFitChanged(config::BackgroundFit),
    SettingsBackgroundDimChanged(f32),
    SettingsSurfaceOpacityChanged(f32),
    SettingsBackgroundRemoved,
    SettingsGapChanged(f32),
    SettingsRadiusChanged(f32),
    SettingsBorderChanged(f32),
    SettingsReset,
    AccountMenuDismissed,
    SidebarResizeStarted,
    SidebarResizeMoved(f32),
    SidebarResizeEnded,
    ScrollActivity,
    AnimationTick,
    Tick,
    AgentRequest {
        id: u64,
        command: AgentCommand,
    },
    AgentScreenshotCaptured {
        id: u64,
        path: PathBuf,
        result: Result<CapturedScreenshot, String>,
    },
}
