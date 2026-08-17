use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use crate::cache::Cache;
use crate::composer::ComposerState;
use crate::config::{self, Session};
use crate::domain::{
    ActiveThreadKey, ActivityState, ComposerAttachment, DmsState, PendingFileMessage,
    PendingScrollTarget, ProfilePaneState, ReadTarget, SearchState, TextSelection, ThreadKey,
    ThreadsState, UnreadsState,
};
use crate::palette::PaletteState;
use crate::slack::models::{ChannelId, MessageTs, TeamId, TeamProfileField, UserId};
use crate::slack::{SlackClient, Transport};
use crate::state::{ChannelMessages, MainView, Screen, Toast, Workspace};

/// Long-lived renderer-neutral state shared by every desktop shell.
///
/// Renderer handles, DOM focus, hover state, modal animation, and window
/// geometry intentionally remain in each shell wrapper.
pub struct CoreAppState {
    pub screen: Screen,
    pub accounts: BTreeMap<config::AccountId, Session>,
    pub active_account: Option<config::AccountId>,
    pub account_epoch: u64,
    pub auth_in_progress: bool,
    pub session: Option<Session>,
    pub cache: Option<Cache>,
    pub client: SlackClient,
    pub transport: Option<Arc<Transport>>,
    pub active_team: Option<TeamId>,
    pub active_channel: Option<ChannelId>,
    pub active_thread: Option<ActiveThreadKey>,
    pub thread_open: bool,
    pub main_view: MainView,
    pub unreads: UnreadsState,
    pub threads_view: ThreadsState,
    pub activity: ActivityState,
    pub dms: DmsState,
    pub workspaces: BTreeMap<TeamId, Workspace>,
    pub threads: HashMap<ThreadKey, ChannelMessages>,
    pub composer: ComposerState,
    pub thread_composer: ComposerState,
    pub edit_composer: ComposerState,
    pub composer_attachments: Vec<ComposerAttachment>,
    pub thread_composer_attachments: Vec<ComposerAttachment>,
    pub pending_file_messages: Vec<PendingFileMessage>,
    pub attachment_seq: u64,
    pub editing: Option<(ChannelId, MessageTs)>,
    pub profile_pane: Option<ProfilePaneState>,
    pub profile_generation: u64,
    pub profile_fields: HashMap<TeamId, Vec<TeamProfileField>>,
    pub text_selection: Option<TextSelection>,
    pub search_input: String,
    pub search: Option<SearchState>,
    pub palette: Option<PaletteState>,
    pub errors: Vec<Toast>,
    pub send_seq: u64,
    pub last_typing: HashMap<(TeamId, ChannelId), Instant>,
    pub last_active_channels: HashMap<TeamId, ChannelId>,
    pub image_viewer_generation: u64,
    pub emoji_hydrated: HashSet<(TeamId, String)>,
    pub channel_hydrated: HashSet<(TeamId, ChannelId)>,
    pub avatar_profile_hydrated: HashSet<UserId>,
    pub pending_scroll_to: Option<(ChannelId, PendingScrollTarget)>,
    pub thread_unread_marker: Option<(ThreadKey, MessageTs)>,
    pub chat_paused: HashMap<ChannelId, u32>,
    pub pending_marks: HashSet<(ReadTarget, MessageTs)>,
    pub mark_blocked: HashSet<ReadTarget>,
    pub cache_dirty: HashMap<TeamId, Instant>,
    pub cache_saving: HashMap<TeamId, Instant>,
    pub settings: config::Settings,
    pub settings_color_drafts: HashMap<config::ColorRole, String>,
    pub settings_color_errors: HashMap<config::ColorRole, String>,
}

impl CoreAppState {
    pub fn new(settings: config::Settings) -> Self {
        let settings_color_drafts = config::ColorRole::ALL
            .into_iter()
            .filter_map(|role| {
                settings
                    .colors
                    .get(role)
                    .map(|color| (role, color.as_hex()))
            })
            .collect();
        Self {
            screen: Screen::Login,
            accounts: BTreeMap::new(),
            active_account: None,
            account_epoch: 0,
            auth_in_progress: false,
            session: None,
            cache: None,
            client: SlackClient::default(),
            transport: None,
            active_team: None,
            active_channel: None,
            active_thread: None,
            thread_open: false,
            main_view: MainView::Home,
            unreads: UnreadsState::default(),
            threads_view: ThreadsState::default(),
            activity: ActivityState::default(),
            dms: DmsState::default(),
            workspaces: BTreeMap::new(),
            threads: HashMap::new(),
            composer: ComposerState::default(),
            thread_composer: ComposerState::default(),
            edit_composer: ComposerState::default(),
            composer_attachments: Vec::new(),
            thread_composer_attachments: Vec::new(),
            pending_file_messages: Vec::new(),
            attachment_seq: 0,
            editing: None,
            profile_pane: None,
            profile_generation: 0,
            profile_fields: HashMap::new(),
            text_selection: None,
            search_input: String::new(),
            search: None,
            palette: None,
            errors: Vec::new(),
            send_seq: 0,
            last_typing: HashMap::new(),
            last_active_channels: HashMap::new(),
            image_viewer_generation: 0,
            emoji_hydrated: HashSet::new(),
            channel_hydrated: HashSet::new(),
            avatar_profile_hydrated: HashSet::new(),
            pending_scroll_to: None,
            thread_unread_marker: None,
            chat_paused: HashMap::new(),
            pending_marks: HashSet::new(),
            mark_blocked: HashSet::new(),
            cache_dirty: HashMap::new(),
            cache_saving: HashMap::new(),
            settings,
            settings_color_drafts,
            settings_color_errors: HashMap::new(),
        }
    }
}
