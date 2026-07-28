use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposerTarget {
    Channel,
    Thread,
    Edit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachTarget {
    Channel,
    Thread,
}

impl AttachTarget {
    pub fn composer(self) -> ComposerTarget {
        match self {
            Self::Channel => ComposerTarget::Channel,
            Self::Thread => ComposerTarget::Thread,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatMark {
    Bold,
    Italic,
    Strike,
    Code,
    CodeBlock,
    Quote,
}

#[derive(Debug, Clone)]
pub enum Message {
    AccountScoped(u64, Box<Message>),
    Conversation(ConversationMessage),
    Workspace(WorkspaceMessage),
    Discovery(DiscoveryMessage),
    Runtime(RuntimeMessage),
}

#[derive(Debug, Clone)]
pub enum ConversationMessage {
    WorkspaceSelected(TeamId),
    ChannelSelected(ChannelId),
    ComposerAction {
        target: ComposerTarget,
        action: text_editor::Action,
    },
    ComposerFormat {
        target: ComposerTarget,
        mark: FormatMark,
    },
    ComposerDelete {
        target: ComposerTarget,
        motion: text_editor::Motion,
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
pub enum WorkspaceMessage {
    BootLoaded(TeamId, Result<BootData, SlackError>),
    CountsLoaded(TeamId, Result<CountsPage, SlackError>),
    SidebarDmsLoaded(TeamId, Result<SidebarDmsPage, SlackError>),
    ChannelSectionsLoaded(TeamId, Result<ChannelSectionsPage, SlackError>),
    HistoryLoaded(
        TeamId,
        ChannelId,
        HistoryLoadKind,
        Result<super::LoadedHistory, SlackError>,
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
    ImageViewerVideoPrepared {
        generation: u64,
        result: Result<PreparedVideo, SlackError>,
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
    ImageViewerTransformed {
        zoom: f32,
        offset: Vector,
    },
    ImageViewerDownloadPressed,
    OpenUrl(String),
    UrlOpened(Result<(), String>),
    FilePreviewLoaded {
        key: String,
        result: Result<FilePreview, SlackError>,
    },
    AvatarLoaded {
        user: UserId,
        result: Result<Vec<u8>, SlackError>,
    },
    EmojiPreviewLoaded {
        key: String,
        result: Result<FilePreview, SlackError>,
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

#[derive(Debug, Clone)]
pub enum RuntimeMessage {
    Realtime(TeamId, u64, RtEvent),
    RtConnected(TeamId, u64, Connection),
    RtDisconnected(TeamId, u64),
    MainViewSelected(crate::state::MainView),
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
    SelfPresenceSelected(crate::state::Presence),
    SelfPresenceUpdated {
        team: TeamId,
        presence: crate::state::Presence,
        previous: Option<crate::state::Presence>,
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
        command: agent::AgentCommand,
    },
    AgentScreenshotCaptured {
        id: u64,
        path: PathBuf,
        result: Result<iced::window::Screenshot, String>,
    },
}

#[allow(non_snake_case)]
impl Message {
    pub fn PaletteQueryChanged(value: String) -> Self {
        Self::Discovery(DiscoveryMessage::PaletteQueryChanged(value))
    }

    pub fn FileDownloaded(value: Result<PathBuf, SlackError>) -> Self {
        Self::Discovery(DiscoveryMessage::FileDownloaded(value))
    }

    pub fn ImageViewerVideoSeekChanged(value: f32) -> Self {
        Self::Discovery(DiscoveryMessage::ImageViewerVideoSeekChanged(value))
    }

    pub fn ImageViewerVideoVolumeChanged(value: f32) -> Self {
        Self::Discovery(DiscoveryMessage::ImageViewerVideoVolumeChanged(value))
    }

    pub fn ImageViewerZoomChanged(value: f32) -> Self {
        Self::Discovery(DiscoveryMessage::ImageViewerZoomChanged(value))
    }

    pub fn UrlOpened(value: Result<(), String>) -> Self {
        Self::Discovery(DiscoveryMessage::UrlOpened(value))
    }

    pub fn DesktopNotificationShown(value: Result<(), String>) -> Self {
        Self::Discovery(DiscoveryMessage::DesktopNotificationShown(value))
    }

    pub fn DmsUnreadOnlyToggled(value: bool) -> Self {
        Self::Runtime(RuntimeMessage::DmsUnreadOnlyToggled(value))
    }

    pub fn DmsFilterChanged(value: String) -> Self {
        Self::Runtime(RuntimeMessage::DmsFilterChanged(value))
    }

    pub fn AuthenticationFinished(value: bool) -> Self {
        Self::Runtime(RuntimeMessage::AuthenticationFinished(value))
    }

    pub fn SettingsBackgroundPicked(value: Option<PathBuf>) -> Self {
        Self::Runtime(RuntimeMessage::SettingsBackgroundPicked(value))
    }

    pub fn SettingsBackgroundImported(value: Result<config::BackgroundSettings, String>) -> Self {
        Self::Runtime(RuntimeMessage::SettingsBackgroundImported(value))
    }

    pub fn SettingsBackgroundDimChanged(value: f32) -> Self {
        Self::Runtime(RuntimeMessage::SettingsBackgroundDimChanged(value))
    }

    pub fn SettingsSurfaceOpacityChanged(value: f32) -> Self {
        Self::Runtime(RuntimeMessage::SettingsSurfaceOpacityChanged(value))
    }

    pub fn SettingsGapChanged(value: f32) -> Self {
        Self::Runtime(RuntimeMessage::SettingsGapChanged(value))
    }

    pub fn SettingsRadiusChanged(value: f32) -> Self {
        Self::Runtime(RuntimeMessage::SettingsRadiusChanged(value))
    }

    pub fn SettingsBorderChanged(value: f32) -> Self {
        Self::Runtime(RuntimeMessage::SettingsBorderChanged(value))
    }
}
