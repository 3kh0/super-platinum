use std::collections::HashMap;

use crate::channel_vm::{ChannelRow, channel_rows, project_channels};
use crate::media::MediaRegistry;
pub use crate::model::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PendingDeliveryStatus {
    Failed,
    Unconfirmed,
}

pub struct ShellState {
    pub(crate) message_buttons: crate::message_buttons::State,
    pub(crate) app_home: crate::app_home::Root,
    pub(crate) directory_member_fetches: crate::directory_member_fetches::Registry,
    pub(crate) app_modal: Option<crate::app_modal::Session>,
    pub(crate) app_modal_closes: Vec<crate::app_modal::CloseTicket>,
    pub core: super_platinum_core::CoreAppState,
    pub(crate) dock_badge_cache: super::dock_badge::Cache,
    pub(crate) managed_drafts: crate::drafts::session::Session,
    pub(crate) draft_writes: crate::drafts::integration::Desktop,
    pub huddle_ui: super::HuddleUi,
    pub media: MediaRegistry,
    pub media_epoch: u64,
    pub background_uri: Option<String>,
    pub signed_in: bool,
    pub loading: bool,
    pub accounts: Vec<AccountVm>,
    pub workspaces: Vec<WorkspaceVm>,
    pub self_account: SelfAccountVm,
    pub active_workspace: usize,
    pub channels: Vec<ChannelVm>,
    pub sidebar_sections: Vec<SidebarSectionVm>,
    /// What the sidebar layout carries between passes (see `SidebarMemory`).
    pub sidebar_memory: super_platinum_core::state::SidebarMemory,
    pub active_channel: usize,
    /// Complete cheap row metadata; rich messages cover only the absolute window.
    pub channel_rows: Vec<ChannelRow>,
    pub(crate) channel_geometry: crate::timeline_geometry::Geometry,
    pub messages: Vec<MessageVm>,
    pub(super) fixture_channel_messages: Option<Vec<MessageVm>>,
    pub timeline_start: usize,
    pub timeline_end: usize,
    pub row_heights: HashMap<String, f64>,
    pub message_arrivals: HashMap<String, std::time::Instant>,
    /// Transient outcomes for globally unique optimistic timestamps. All send
    /// writers guard account epoch and transport; account reloads replace this
    /// map with the fresh ShellState. These labels are not durable recovery.
    pub pending_delivery: HashMap<String, PendingDeliveryStatus>,
    pub selection_pinned: bool,
    /// Bumped while file uploads are in flight so progress UI re-renders.
    pub upload_ui_epoch: u64,
    pub(crate) file_events: crate::file_events::Tracker,
    pub(crate) file_share: crate::file_share::State,
    pub(crate) file_share_fixture: Option<crate::file_share::FixtureCapability>,
    pub stick_to_bottom: bool,
    pub loading_older: bool,
    pub chat_paused: bool,
    pub main_view: MainView,
    pub channel_tab: ChannelTab,
    pub(crate) channel_tabs: crate::channel_tabs::Root,
    pub dm_query: String,
    pub dm_unread_only: bool,
    pub activity_tab: ActivityTab,
    /// What each surface has open in its conversation column, keyed by the
    /// surface. Home is in here too, so a rail switch restores the channel that
    /// tab was last showing rather than inheriting another surface's.
    pub surfaces: HashMap<MainView, SurfaceTarget>,
    pub profile_hover: Option<ProfileHoverVm>,
    pub profile_hover_generation: u64,
    pub profile_hover_card_active: bool,
    pub overlay: Option<Overlay>,
    pub reaction_target: Option<ReactionTarget>,
    pub composer_emoji: crate::composer_emoji::State,
    pub(crate) emoji_catalog: crate::emoji_catalog::State,
    pub(crate) emoji_preferences: crate::emoji_preferences::State,
    pub forward_target: Option<crate::forward::ForwardTarget>,

    pub settings_section: SettingsSection,
    pub notification_schedule: crate::notification_schedule::State,
    pub presentation_notifications: crate::presentation_notifications::State,
    pub global_notifications: crate::global_notifications::State,
    pub storage: StoragePanel,
    pub palette_query: String,
    pub palette_selected: usize,
    pub palette_generation: u64,
    /// UI acknowledgement for a shortcut opening a fresh dialog visit.
    pub shortcut_generation: u64,
    pub palette_opening: Option<String>,
    pub search_query: String,
    pub search_input_revision: u64,
    pub search_focus_at_end: bool,
    pub search_open_generation: u64,
    pub search_opening: Option<(String, String)>,
    pub search_filters: Option<super_platinum_core::search_query::SearchFilters>,
    pub search_filter_error: Option<&'static str>,
    pub search_results: Vec<SearchResultVm>,
    pub search_pager: super_platinum_core::search::SearchPager,
    /// Cancels metadata resolution when another saved item is opened.
    pub later_open_generation: u64,
    /// Channel and `last_read` captured when the conversation was opened. The
    /// unread divider stays on that message for the whole visit, even once the
    /// read mark has moved `last_read` past it.
    pub unread_anchor: Option<(String, String)>,
    pub manual_unread: crate::unread::UnreadState,
    pub(crate) read_presentation: crate::read_preferences::presentation::Presentation,
    pub(crate) read_settings: crate::read_preferences::settings::State,
    pub(crate) read_shortcuts: crate::read_shortcuts::State,
    pub(crate) bulk_read: crate::read_shortcuts::bulk::State,
    pub thread_unread: crate::thread_unread::ThreadUnreadState,
    pub thread_root: Option<String>,
    pub thread_messages: Vec<MessageVm>,
    pub(crate) thread_window: crate::thread_window::ThreadWindow,
    /// Thread pane scroll anchor: true while the reader is parked at the newest
    /// reply, so arriving replies keep the pane pinned to the bottom.
    pub thread_at_bottom: bool,
    pub thread_generation: u64,
    pub thread_target: Option<String>,
    pub thread_loading: bool,
    pub thread_error: Option<String>,
    pub thread_history: super_platinum_core::thread_history::ThreadHistory,
    pub profile_user: Option<String>,
    pub profile_generation: u64,
    pub profile_dm_generation: u64,
    pub profile_dm_opening: Option<String>,
    /// A user link keeps the profile open; the profile's Message action does not.
    pub(crate) profile_dm_link_generation: Option<u64>,
    pub group_panel: Option<crate::usergroups::GroupPanel>,
    pub profile_pane_width: f64,
    pub profile_menu_open: bool,
    /// Nested pause-notifications list inside the signed-in user's menu.
    pub self_menu_notifications_open: bool,
    pub(crate) slack_lists: crate::slack_lists::State,
    pub(crate) self_status: crate::self_status::State,
    pub(crate) profile_edit: crate::profile_edit::State,
    pub(crate) profile_title: crate::profile_title::State,
    pub(crate) channel_mute: crate::channel_mute::State,
    pub(crate) upload_checks: crate::messaging::delivery_check::State,
    pub(crate) upload_journal: crate::messaging::upload_journal::State,
    pub self_menu_mutations: crate::bootstrap::SelfMenuMutations,
    pub profile_vip_pending: crate::profile_vip::Pending,
    pub viewer: Option<ViewerVm>,
    pub file_refresh: crate::file_preview::refresh::State,
    pub canvas: Option<CanvasTarget>,
    pub(crate) canvas_titles: crate::canvas::quip_titles::State,
    pub(crate) canvas_checklist: crate::canvas::checklist::State,
    pub canvas_create: Option<crate::canvas_create::Draft>,
    pub canvas_creation: crate::canvas_create::Root,
    pub toast: Option<ToastVm>,
    /// What the rail says about the link to Slack.
    pub connection: ConnectionVm,
    pub performance: PerformanceVm,
    pub channel_switch_started: Option<std::time::Instant>,
    /// Bumped on every conversation open. Async work started for one channel
    /// checks it after each await and drops out when the reader has already
    /// moved on, so a slow history fetch cannot scroll or mark the conversation
    /// that replaced it.
    pub channel_generation: u64,
    /// Supersedes unresolved Slack-link intents without cancelling history loads.
    pub(crate) slack_link_generation: u64,
    pub edit_generation: u64,
    pub realtime_insert_started: Option<std::time::Instant>,
}

/// How long a status line stays on screen. Long enough to read, short enough
/// that a failure the reader has moved on from cannot outlive their interest.
pub const TOAST_LIFE: std::time::Duration = std::time::Duration::from_secs(5);

impl ShellState {
    pub(crate) fn pending_delivery_label(&self, ts: &str) -> &'static str {
        match self.pending_delivery.get(ts) {
            Some(PendingDeliveryStatus::Failed) => "Not sent",
            Some(PendingDeliveryStatus::Unconfirmed) => "Delivery unconfirmed",
            None if self.pending_attachments_for_message(ts).is_some() => "Uploading…",
            None => "Sending…",
        }
    }

    pub(crate) fn prune_pending_delivery(&mut self) {
        if self.pending_delivery.is_empty() {
            return;
        }
        let core = &self.core;
        self.pending_delivery.retain(|ts, _| {
            core.workspaces.values().any(|workspace| {
                workspace
                    .messages
                    .values()
                    .any(|messages| messages.pending.contains(ts))
            }) || core
                .threads
                .values()
                .any(|messages| messages.pending.contains(ts))
        });
    }

    /// Shows a transient status line, replacing whatever is on screen and
    /// restarting its five-second life.
    pub fn show_toast(&mut self, text: impl Into<String>) {
        self.toast = Some(ToastVm::new(text));
    }

    /// Routes a failed Slack call.
    ///
    /// A dropped link is the connection indicator's story, not a toast's: the
    /// raw transport error is a signed Slack URL the reader can do nothing
    /// with, and every call in flight produces one. `offline` is the short line
    /// to show instead when the reader asked for this themselves; an empty one
    /// keeps a background refresh silent, since the rail is already saying it.
    pub fn report_failure(
        &mut self,
        error: &impl super_platinum_core::error::NetworkFailure,
        offline: &str,
        detail: impl FnOnce() -> String,
    ) {
        if error.is_offline() {
            if !offline.is_empty() {
                self.show_toast(offline);
            }
            return;
        }
        self.show_toast(detail());
    }

    /// Retires a status line that has had its five seconds.
    pub fn expire_toast(&mut self, now: std::time::Instant) {
        if self
            .toast
            .as_ref()
            .is_some_and(|toast| now.duration_since(toast.shown_at) >= TOAST_LIFE)
        {
            self.toast = None;
        }
    }

    /// The divider position to project this channel with.
    pub(crate) fn divider_at(&self, channel_id: &str) -> Option<&str> {
        self.unread_anchor
            .as_ref()
            .filter(|(channel, _)| channel == channel_id)
            .map(|(_, ts)| ts.as_str())
    }

    /// Closes the thread pane. Activity shows one surface at a time, so the
    /// pane must not linger with the previous item's replies when a channel
    /// item is opened next.
    pub fn close_thread(&mut self) {
        self.cancel_profile_dm();
        self.checkpoint_composers();
        self.thread_generation = self.thread_generation.wrapping_add(1);
        self.thread_root = None;
        self.sync_composer_drafts();
        self.thread_messages.clear();
        self.thread_window = Default::default();
        self.thread_at_bottom = true;
        self.thread_target = None;
        self.thread_loading = false;
        self.thread_error = None;
        self.thread_history = Default::default();
    }

    pub(crate) fn thread_scope(&self) -> String {
        format!(
            "{}-{}-{}",
            self.timeline_scope(),
            self.thread_root.as_deref().unwrap_or_default(),
            self.thread_generation
        )
    }

    pub(crate) fn begin_thread(&mut self, root: String, target: Option<String>) {
        self.close_thread();
        self.close_profile();
        self.thread_root = Some(root);
        self.thread_at_bottom = target.is_none();
        self.thread_target = target;
        self.thread_loading = true;
        self.thread_history
            .watch_live(super_platinum_core::state::now_secs());
        self.refresh_from_core();
    }

    /// A visible resource panel must not consume unread messages behind it.
    pub fn transcript_visible(&self) -> bool {
        self.channel_tab == ChannelTab::Messages
            && matches!(
                self.main_view,
                MainView::Home | MainView::Dms | MainView::Activity | MainView::Later
            )
            && self.detail_open()
            && !(matches!(self.main_view, MainView::Activity | MainView::Later)
                && self.thread_root.is_some())
    }

    /// Whether the current surface has a conversation to show.
    ///
    /// Home always does — it is the channel. DMs and Activity keep a list and
    /// stay on their empty state until a row is opened, so a channel the quick
    /// switcher opened elsewhere never leaks into their column.
    pub fn detail_open(&self) -> bool {
        !matches!(self.main_view, MainView::Files | MainView::Scheduled)
            && (!self.main_view.has_own_list() || self.surfaces.contains_key(&self.main_view))
    }

    /// Records what the current surface is showing, so returning to this rail
    /// tab restores it rather than inheriting whatever another surface opened.
    pub fn remember_surface(&mut self) {
        if !self.detail_open() {
            return;
        }
        let Some(channel) = self.core.active_channel.clone() else {
            return;
        };
        self.surfaces.insert(
            self.main_view,
            SurfaceTarget {
                channel,
                thread_root: self.thread_root.clone(),
            },
        );
    }

    /// Clears the conversation the current surface was showing, returning it to
    /// its empty state. Home has no empty state, so it keeps its channel.
    pub fn forget_surface(&mut self) {
        if self.main_view.has_own_list() {
            self.surfaces.remove(&self.main_view);
        }
        self.close_thread();
        if self.main_view == MainView::Activity {
            self.core.activity.selected = None;
        }
    }

    pub fn show_profile_hover(&mut self, user_id: String, x: f64, y: f64) {
        self.profile_hover_generation = self.profile_hover_generation.wrapping_add(1);
        self.profile_hover_card_active = false;
        self.profile_hover = Some(ProfileHoverVm { user_id, x, y });
    }

    pub fn hold_profile_hover(&mut self) {
        self.profile_hover_generation = self.profile_hover_generation.wrapping_add(1);
        self.profile_hover_card_active = true;
    }

    pub fn cancel_profile_dm(&mut self) {
        self.profile_dm_generation = self.profile_dm_generation.wrapping_add(1);
        self.profile_dm_opening = None;
        self.profile_dm_link_generation = None;
    }

    pub fn close_profile(&mut self) {
        self.profile_generation = self.profile_generation.wrapping_add(1);
        self.cancel_profile_dm();
        self.profile_user = None;
        self.group_panel = None;
        self.profile_menu_open = false;
        self.core.profile_pane = None;
    }

    /// Collapse or expand a sidebar section, as clicking its heading does in
    /// Slack. Held locally: Slack's own `channel_sections` pref is not written.
    pub fn toggle_sidebar_section(&mut self, section: &str) {
        let Some(team) = self.core.active_team.clone() else {
            return;
        };
        if let Some(workspace) = self.core.workspaces.get_mut(&team) {
            workspace.sidebar.toggle_collapsed(section);
            self.refresh_from_core();
        }
    }

    pub fn refresh_from_core(&mut self) {
        self.refresh_dock_badge();
        self.prune_pending_delivery();
        let held_window = (self.core.pending_scroll_to.is_none()
            && self.channels.get(self.active_channel).map(|c| &c.id)
                == self.core.active_channel.as_ref())
        .then(|| {
            self.channel_rows[self.timeline_start.min(self.channel_rows.len())
                ..self
                    .timeline_end
                    .min(self.channel_rows.len())
                    .max(self.timeline_start.min(self.channel_rows.len()))]
                .iter()
                .map(|m| m.id.clone())
                .collect::<Vec<_>>()
        });
        let Some(team) = self.core.active_team.clone() else {
            return;
        };
        let Some(workspace) = self.core.workspaces.get(&team) else {
            return;
        };
        if self
            .viewer
            .as_ref()
            .and_then(|v| v.file.as_ref().map(|f| &f.id))
            .is_some_and(|id| workspace.deleted_files.contains(id))
        {
            self.file_refresh.close();
            self.viewer = None;
            if self.overlay == Some(Overlay::Viewer) {
                self.overlay = None;
            }
        }
        if self
            .canvas
            .as_ref()
            .is_some_and(|v| workspace.deleted_files.contains(&v.id))
        {
            self.canvas = None;
            if self.overlay == Some(Overlay::Canvas) {
                self.overlay = None;
            }
        }
        self.self_account = project_self_account(workspace, &self.media);
        let preferred_active = self.core.active_channel.clone();
        let (mut channels, sidebar_sections, default_active, _) = project_channels(
            workspace,
            &self.media,
            preferred_active.as_deref(),
            &mut self.sidebar_memory,
        );
        let active_channel = preferred_active
            .as_ref()
            .and_then(|id| channels.iter().position(|channel| &channel.id == id))
            .unwrap_or(default_active);
        // Ensure active conversation remains addressable even if filtered out of sections.
        if let Some(active_id) = preferred_active.as_ref()
            && !channels.iter().any(|channel| &channel.id == active_id)
            && let Some(channel) = workspace.channels.get(active_id)
        {
            channels.push(crate::channel_vm::channel_vm(
                workspace,
                channel,
                &self.media,
            ));
        }
        let active_channel = preferred_active
            .as_ref()
            .and_then(|id| channels.iter().position(|channel| &channel.id == id))
            .unwrap_or(active_channel.min(channels.len().saturating_sub(1)));
        let rows = channels
            .get(active_channel)
            .map(|channel| channel_rows(workspace, &channel.id, self.divider_at(&channel.id)))
            .unwrap_or_default();
        self.active_channel = active_channel;
        self.sidebar_sections = sidebar_sections;
        self.channels = channels;
        self.fixture_channel_messages = None;
        self.channel_rows = rows;
        self.rebuild_channel_geometry();
        self.refresh_thread_from_core();
        if let Some(ids) = held_window {
            self.restore_timeline_window(&ids);
            if self.stick_to_bottom && !self.selection_pinned {
                // Keep the measured bottom neighborhood instead of mounting
                // 100 rows again on every presence/reaction/metadata refresh.
                // Include arrivals, with a bound for large history merges.
                self.timeline_end = self.channel_rows.len();
                self.timeline_start = self
                    .timeline_start
                    .max(self.timeline_end.saturating_sub(100));
            }
        } else {
            self.reset_timeline_bounds();
        }
        if let Some(channel) = self.channels.get(active_channel) {
            self.core.active_channel = Some(channel.id.clone());
        }
        self.reproject_channel_window();
        self.sync_composer_drafts();
    }
}

/// Projects the signed-in user for the rail button. Falls back to the user id
/// when the boot payload has not named them yet, so the button never renders
/// blank.
pub(crate) fn project_self_account(
    workspace: &super_platinum_core::state::Workspace,
    media: &MediaRegistry,
) -> SelfAccountVm {
    let user_id = workspace.self_user_id.clone();
    let name = workspace.display_name(&user_id);
    let avatar = workspace
        .avatar_url(&user_id)
        .map(|url| media.register_avatar(&user_id, &url));
    let initials = name
        .chars()
        .find(|ch| ch.is_alphanumeric())
        .map(|ch| ch.to_uppercase().to_string())
        .unwrap_or_else(|| "?".into());
    SelfAccountVm {
        user_id,
        name,
        avatar,
        initials,
        presence: PresenceVm::from_core(workspace.self_presence()),
        snoozed: workspace.self_snoozed(),
        workspace_name: workspace.name.clone(),
    }
}

pub(crate) fn keep_recent_messages(state: &mut ShellState, count: usize) {
    let mut messages = state
        .fixture_channel_messages
        .take()
        .unwrap_or_else(|| state.messages.clone());
    let len = messages.len();
    if len > count {
        messages = messages.split_off(len - count);
    }
    state.install_fixture_messages(messages);
}

pub(crate) fn account_vms(accounts: &super_platinum_core::config::Accounts) -> Vec<AccountVm> {
    accounts
        .sessions
        .iter()
        .map(|(id, session)| AccountVm {
            id: id.clone(),
            label: session
                .workspaces
                .values()
                .map(|workspace| workspace.name.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            active: id == &accounts.active_account,
        })
        .collect()
}

impl ShellState {
    pub fn replace_search_query(&mut self, query: String) {
        self.search_focus_at_end = false;
        self.cancel_search_open();
        self.search_input_revision = self.search_input_revision.wrapping_add(1);
        self.set_search_query(query);
    }

    pub fn set_search_query(&mut self, query: String) {
        if self.search_query.trim() != query.trim() {
            self.cancel_search_open();
            self.search_pager.reset();
            self.search_results.clear();
            self.search_filters = None;
            self.search_filter_error = None;
        }
        self.search_query = query;
    }
}

impl ShellState {
    pub fn apply_search_filters(&mut self) -> bool {
        let Some(filters) = self.search_filters.as_ref() else {
            return false;
        };
        match filters.apply(&self.search_query) {
            Ok(query) => {
                self.replace_search_query(query);
                self.search_filters = None;
                self.search_filter_error = None;
                true
            }
            Err(error) => {
                self.search_filter_error = Some(error);
                false
            }
        }
    }
}

impl ShellState {
    pub fn cancel_search_open(&mut self) {
        self.search_open_generation = self.search_open_generation.wrapping_add(1);
        self.search_opening = None;
    }

    pub fn dismiss_overlay(&mut self) {
        if crate::canvas_create::departure(self) {
            return;
        }
        if self.overlay == Some(Overlay::Viewer) {
            crate::file_preview::refresh::close(self);
        }
        if self.overlay == Some(Overlay::FileShare) {
            crate::file_share::dismiss(self);
            return;
        }
        if self.overlay == Some(Overlay::ComposerEmoji) {
            crate::composer_emoji::close_shell(self);
            return;
        }
        if self.overlay == Some(Overlay::MessageExternalSelect) {
            crate::message_buttons::inputs::external::cancel_local(self);
            return;
        }
        if self.overlay == Some(Overlay::MessageUserSelect) {
            crate::message_buttons::inputs::user::cancel_local(self);
        }
        if self.overlay == Some(Overlay::MessageMultiSelect) {
            crate::message_buttons::inputs::multi::cancel_local(self);
            return;
        }
        if self.overlay == Some(Overlay::ButtonConfirmation) {
            crate::message_buttons::cancel_local(self);
            return;
        }
        if self.overlay == Some(Overlay::AppShortcuts) {
            crate::message_buttons::shortcuts::cancel(self);
            return;
        }
        if self.overlay == Some(Overlay::AppModal) {
            crate::app_modal::dismiss_local(self);
            return;
        }
        if self.overlay == Some(Overlay::SlackList) {
            crate::slack_lists::close_shell(self);
            return;
        }
        if self.overlay == Some(Overlay::ProfileTitle) {
            crate::profile_title::close_shell(self);
            return;
        }
        if self.overlay == Some(Overlay::ProfileEdit) {
            crate::profile_edit::close_shell(self);
            return;
        }
        if self.overlay == Some(Overlay::SelfStatus) {
            crate::self_status::close_shell(self);
            return;
        }
        if self.overlay == Some(Overlay::Canvas) {
            crate::canvas::checklist::close_visit(self);
        }
        self.cancel_profile_dm();
        self.cancel_palette_open();
        self.cancel_search_open();
        self.overlay = None;
    }

    pub fn open_search(&mut self, conversation: bool) {
        if crate::canvas_create::departure::request(
            self,
            crate::canvas_create::departure::Destination::Search { conversation },
        ) {
            return;
        }
        if !self.signed_in || self.loading {
            return;
        }
        self.shortcut_generation = self.shortcut_generation.wrapping_add(1);
        self.cancel_profile_dm();
        self.cancel_search_open();
        if conversation && self.detail_open() {
            if let Some(channel) = self.core.active_channel.clone() {
                self.replace_search_query(format!("in:<#{channel}> "));
                self.search_focus_at_end = true;
            }
        } else {
            // Reopening a workspace search retains the previous query/results.
            self.search_focus_at_end = false;
            self.search_input_revision = self.search_input_revision.wrapping_add(1);
        }
        self.profile_hover = None;
        self.overlay = Some(Overlay::Search);
    }
}
