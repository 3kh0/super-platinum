mod pinned_files;

use crate::media::MediaUri;
use std::collections::HashMap;
use std::sync::Arc;

use super_platinum_core::MediaAssetKind;

use super::{ShellState, account_vms, keep_recent_messages};
use crate::channel_vm::{channel_rows, project_channel_window, project_channels};
use crate::fixture::fixture_core;
use crate::media::MediaRegistry;
use crate::model::*;

/// Fixture reaction pill — resolves the glyph the same way live messages do.
fn reaction(name: &str, count: u32, own: bool) -> ReactionVm {
    let others = [("U1", "Maya Chen"), ("U2", "Jules")];
    let reactors = own
        .then_some(("U0", "You"))
        .into_iter()
        .chain(others)
        .take(count as usize)
        .map(|(user_id, name)| ReactorVm {
            user_id: user_id.to_owned(),
            name: name.to_owned(),
            initials: name[..1].to_owned(),
            avatar: None,
        })
        .collect();
    ReactionVm {
        glyph: Some(super_platinum_core::state::emoji_glyph(name)),
        media: None,
        name: name.to_owned(),
        count,
        own,
        reactors,
    }
}

impl ShellState {
    pub fn from_environment(media: MediaRegistry) -> Self {
        let Ok(fixture) = std::env::var("SUPER_PLATINUM_FIXTURE") else {
            let mut state = Self::from_cached_account(media.clone()).unwrap_or_else(|error| {
                eprintln!("super-platinum: could not open cached account: {error}");
                Self::login(media)
            });
            state.load_managed_background();
            return state;
        };
        if crate::performance::emoji_retirement::enabled() {
            let mut state = Self::fixture(media);
            crate::performance::emoji_retirement::install(&mut state);
            return state;
        }
        if crate::performance::emoji_lifecycle::fixture(&fixture).is_some() {
            let mut state = Self::fixture(media);
            crate::performance::emoji_lifecycle::install(&mut state);
            return state;
        }
        if let Some(total) = crate::performance::completion::size(&fixture) {
            let mut state = Self::fixture(media);
            crate::performance::completion::install(&mut state, total);
            return state;
        }
        match fixture.as_str() {
            "login" => Self::login(media),
            "opening" => Self::opening(media),
            "loading" => {
                let mut state = Self::fixture(media);
                state.loading = true;
                state
            }
            "settings" => {
                let mut state = Self::fixture(media);
                state.overlay = Some(Overlay::Settings);
                state
            }
            "search-has-filters" | "search-has-filters-narrow" => {
                let mut state = Self::fixture(media);
                crate::view::search::check::install(&mut state);
                state
            }
            "search"
            | "search-loading"
            | "search-empty"
            | "search-error"
            | "search-more-error"
            | "search-narrow"
            | "search-opening"
            | "search-filters"
            | "search-filters-narrow" => {
                let mut state = Self::fixture(media);
                crate::bootstrap::search_fixture(&mut state, &fixture);
                state
            }
            "thread-window-200"
            | "thread-window-2000"
            | "thread-window-10000"
            | "thread-window-target"
            | "thread-window-gap" => {
                let mut state = Self::fixture(media);
                let count = match fixture.as_str() {
                    "thread-window-200" => 200,
                    "thread-window-2000" => 2000,
                    _ => 10000,
                };
                let root = "1719800400.000000";
                let mut bag = crate::thread_vm::fixture_bag(count);
                bag.pending.push(format!("{}.000000", 1719800400 + count));
                state.thread_root = Some(root.into());
                if fixture == "thread-window-target" || fixture == "thread-window-gap" {
                    state.thread_target = Some("1719800650.000000".into());
                    state.thread_at_bottom = false;
                }
                if fixture == "thread-window-gap" {
                    use super_platinum_core::thread_history::ThreadHistory;
                    state.thread_history.watch_live(1719810300);
                    let page = super_platinum_core::slack::models::HistoryPage {
                        messages: bag.messages[250..450].to_vec(),
                        has_more: true,
                        ..Default::default()
                    };
                    state.thread_history.start(
                        &ThreadHistory::initial(Some("1719800650.000000"), 1719810400),
                        root,
                        &page,
                    );
                }
                state.core.threads.insert(
                    (
                        "T1".into(),
                        state.core.active_channel.clone().unwrap(),
                        root.into(),
                    ),
                    bag,
                );
                state.refresh_thread_from_core();
                state
            }
            "threads"
            | "threads-keyboard-narrow"
            | "threads-file-paste"
            | "threads-file-paste-narrow"
            | "thread-unread-divider"
            | "thread-target"
            | "thread-error"
            | "thread-paging"
            | "thread-paging-error"
            | "thread-grouped"
            | "thread-grouped-pending" => {
                let mut state = Self::fixture(media);
                state.thread_root = Some("m2".into());
                state.thread_messages = vec![
                    MessageVm {
                        id: "m2".into(),
                        ts: "3.0".into(),
                        author: "Jules".into(),
                        timestamp: "9:47 AM".into(),
                        avatar_initials: "JU".into(),
                        body: vec![RichNode::Text(
                            "I measured the warm path: cache, render, then refresh.".into(),
                        )],
                        edited: true,
                        reactions: vec![reaction("white_check_mark", 6, false)],
                        reply_count: 1,
                        ..Default::default()
                    },
                    MessageVm {
                        id: "m2-r1".into(),
                        ts: "3.1".into(),
                        author: "Maya Chen".into(),
                        timestamp: "9:52 AM".into(),
                        avatar_initials: "MC".into(),
                        body: vec![RichNode::Text(
                            "Confirmed offline too; cached content remains visible.".into(),
                        )],
                        ..Default::default()
                    },
                ];
                if fixture.starts_with("thread-grouped") {
                    let root = "1719800400.000000";
                    let mut bag = super_platinum_core::state::ChannelMessages::default();
                    for index in 0..13 {
                        let mut message:super_platinum_core::slack::models::Message=serde_json::from_value(serde_json::json!({"ts":format!("{}.000000",1719800400+index),"user":"U1","thread_ts":root,"text":if index==0 {"A compact thread with complete message controls.".into()}else{format!("Synthetic reply {index}: readable, with less repeated chrome.")}})).unwrap();
                        if index == 4 {
                            message.edited =
                                Some(serde_json::json!({"user":"U1","ts":"1719800450.000000"}));
                        }
                        bag.upsert(message);
                    }
                    if fixture == "thread-grouped-pending" {
                        bag.pending.push("1719800412.000000".into());
                    }
                    state.thread_root = Some(root.into());
                    state.thread_at_bottom = false;
                    state.thread_messages = crate::thread_vm::project(
                        &state.core.workspaces["T1"],
                        &bag,
                        &state.thread_history,
                        root,
                        &state.media,
                    );
                    state.core.threads.insert(
                        (
                            "T1".into(),
                            state.core.active_channel.clone().unwrap(),
                            root.into(),
                        ),
                        bag,
                    );
                }
                if fixture.starts_with("thread-paging") {
                    use super_platinum_core::thread_history::{Direction, ThreadHistory};
                    state.thread_root = Some("3.0".into());
                    state.thread_messages[0].id = "3.0".into();
                    let page=serde_json::from_value(serde_json::json!({"messages":[{"ts":"3.0"},{"ts":"3.1"}],"has_more":true,"response_metadata":{"next_cursor":"fixture-next"}})).unwrap();
                    state.thread_history.start(
                        &ThreadHistory::initial(Some("3.1"), 4),
                        "3.0",
                        &page,
                    );
                    state.thread_at_bottom = false;
                    if fixture == "thread-paging-error" {
                        state.thread_history.failed = Some(Direction::Earlier);
                    }
                }
                if fixture == "thread-error" {
                    state.thread_messages.clear();
                    state.thread_error = Some("Replies could not load. Please try again.".into());
                } else if fixture == "thread-target" {
                    let template = state.thread_messages[1].clone();
                    state.thread_root = Some("3.000000".into());
                    state.thread_messages = (0..40).map(|index| {
                        let mut message = template.clone();
                        message.id = format!("3.{index:06}");
                        message.ts = message.id.clone();
                        message.body = vec![RichNode::Text(format!("Synthetic thread reply {index}. A saved reply should open here even when newer replies exist."))];
                        message
                    }).collect();
                    state.thread_target = Some("3.000007".into());
                    state.thread_at_bottom = false;
                }
                state
            }
            name => Self::fixture_variant(media, name),
        }
    }

    fn from_cached_account(
        media: MediaRegistry,
    ) -> Result<Self, super_platinum_core::error::AppError> {
        let Some(accounts) = super_platinum_core::config::load_accounts()? else {
            return Ok(Self::login(media));
        };
        let Some(session) = accounts.sessions.get(&accounts.active_account) else {
            return Ok(Self::login(media));
        };
        let workspaces = session
            .workspaces
            .values()
            .map(|workspace| WorkspaceVm {
                id: workspace.team_id.clone(),
                name: workspace.name.clone(),
                initials: workspace
                    .name
                    .chars()
                    .next()
                    .unwrap_or('S')
                    .to_uppercase()
                    .to_string(),
            })
            .collect::<Vec<_>>();
        let Some(workspace_session) = session.workspaces.values().next() else {
            return Ok(Self::login(media));
        };
        let cache =
            super_platinum_core::cache::Cache::open_default(&accounts.active_account, true)?;
        let cached_workspaces = session
            .workspaces
            .values()
            .map(|workspace_session| {
                let mut workspace =
                    cache.load_workspace_for_startup(workspace_session)?.unwrap_or_else(|| {
                        super_platinum_core::state::Workspace::from_session(workspace_session)
                    });
                if workspace.recent_channels.is_empty()
                    && let Some(id) = workspace.last_active_channel.clone()
                {
                    workspace.touch_recent(&id);
                }
                Ok((workspace_session.team_id.clone(), workspace))
            })
            .collect::<Result<std::collections::BTreeMap<_, _>, super_platinum_core::error::AppError>>()?;
        let mut core =
            super_platinum_core::CoreAppState::new(super_platinum_core::config::load_settings());
        core.screen = super_platinum_core::state::Screen::Main;
        core.accounts = accounts.sessions.clone();
        core.active_account = Some(accounts.active_account.clone());
        // Async startup restore acquires the configured owner before any draft
        // I/O. This unpublished shell must not start the ordinary worker first.
        let drafts_load_failed = false;
        core.drafts = Default::default();
        core.attachment_seq = core
            .drafts
            .items
            .values()
            .flat_map(|d| &d.attachments)
            .map(|a| a.id)
            .max()
            .unwrap_or(0);
        core.session = Some(session.clone());
        core.transport = super_platinum_core::slack::Transport::new(session.d_cookie.clone())
            .ok()
            .map(Arc::new);
        core.cache = Some(cache);
        core.active_team = Some(workspace_session.team_id.clone());
        core.active_channel = cached_workspaces
            .get(&workspace_session.team_id)
            .and_then(|workspace| workspace.last_active_channel.clone());
        core.workspaces = cached_workspaces;
        let workspace = core.workspaces.get(&workspace_session.team_id).unwrap();

        let preferred = core.active_channel.clone();
        let mut sidebar_memory = super_platinum_core::state::SidebarMemory::default();
        let (channels, sidebar_sections, default_active, _) =
            project_channels(workspace, &media, None, &mut sidebar_memory);
        let active_channel = preferred
            .as_ref()
            .and_then(|id| channels.iter().position(|channel| &channel.id == id))
            .unwrap_or(default_active);
        let channel_rows = channels
            .get(active_channel)
            .map(|channel| channel_rows(workspace, &channel.id, None))
            .unwrap_or_default();
        let timeline_end = channel_rows.len();
        let messages = channels
            .get(active_channel)
            .map(|channel| {
                project_channel_window(
                    workspace,
                    &channel.id,
                    &media,
                    &channel_rows,
                    timeline_end.saturating_sub(100)..timeline_end,
                )
            })
            .unwrap_or_default();
        let self_account = crate::state::project_self_account(workspace, &media);
        core.active_channel = channels
            .get(active_channel)
            .map(|channel| channel.id.clone());

        let state = Self {
            core,
            managed_drafts: Default::default(),
            draft_writes: Default::default(),
            media,
            media_epoch: 0,
            huddle_ui: Default::default(),
            background_uri: None,
            signed_in: true,
            loading: channels.is_empty(),
            accounts: account_vms(&accounts),
            self_account,
            workspaces,
            active_workspace: 0,
            channels,
            sidebar_sections,
            sidebar_memory,
            active_channel,
            channel_rows,
            channel_geometry: crate::timeline_geometry::Geometry::uniform(timeline_end),
            fixture_channel_messages: None,
            messages,
            timeline_start: timeline_end.saturating_sub(100),
            timeline_end,
            row_heights: HashMap::new(),
            message_arrivals: HashMap::new(),
            pending_delivery: HashMap::new(),
            selection_pinned: false,
            upload_ui_epoch: 0,
            file_events: Default::default(),
            file_share: Default::default(),
            file_share_fixture: None,
            stick_to_bottom: true,
            loading_older: false,
            chat_paused: false,
            main_view: MainView::Home,
            channel_tab: ChannelTab::Messages,
            channel_tabs: Default::default(),
            dm_query: String::new(),
            dm_unread_only: false,
            activity_tab: ActivityTab::default(),
            surfaces: HashMap::new(),
            profile_hover: None,
            profile_hover_generation: 0,
            profile_hover_card_active: false,
            overlay: None,
            reaction_target: None,
            composer_emoji: Default::default(),
            forward_target: None,
            emoji_catalog: Default::default(),
            emoji_preferences: Default::default(),
            settings_section: SettingsSection::Appearance,
            notification_schedule: Default::default(),
            presentation_notifications: Default::default(),
            global_notifications: Default::default(),
            storage: StoragePanel::default(),
            palette_query: String::new(),
            palette_selected: 0,
            palette_generation: 0,
            shortcut_generation: 0,
            palette_opening: None,
            search_query: String::new(),
            search_input_revision: 0,
            search_focus_at_end: false,
            search_open_generation: 0,
            search_opening: None,
            search_filters: None,
            search_filter_error: None,
            search_results: Vec::new(),
            search_pager: Default::default(),
            later_open_generation: 0,
            unread_anchor: None,
            manual_unread: Default::default(),
            read_presentation: Default::default(),
            read_settings: Default::default(),
            read_shortcuts: Default::default(),
            bulk_read: Default::default(),
            thread_unread: Default::default(),
            thread_root: None,
            thread_messages: Vec::new(),
            thread_window: Default::default(),
            thread_at_bottom: true,
            thread_generation: 0,
            thread_target: None,
            thread_loading: false,
            thread_error: None,
            thread_history: Default::default(),
            profile_user: None,
            profile_generation: 0,
            profile_dm_generation: 0,
            profile_dm_opening: None,
            profile_dm_link_generation: None,
            group_panel: None,
            profile_pane_width: 495.0,
            profile_menu_open: false,
            self_menu_notifications_open: false,
            slack_lists: crate::slack_lists::State::default(),
            self_status: crate::self_status::State::default(),
            profile_edit: crate::profile_edit::State::default(),
            profile_title: crate::profile_title::State::default(),
            channel_mute: crate::channel_mute::State::default(),
            upload_checks: crate::messaging::delivery_check::State::default(),
            upload_journal: crate::messaging::upload_journal::State::default(),
            dock_badge_cache: super::dock_badge::Cache::default(),
            self_menu_mutations: Default::default(),
            profile_vip_pending: Default::default(),
            viewer: None,
            file_refresh: Default::default(),
            canvas: None,
            canvas_checklist: Default::default(),
            canvas_create: None,
            canvas_creation: Default::default(),
            app_home: Default::default(),
            app_modal: None,
            message_buttons: crate::message_buttons::State::default(),
            app_modal_closes: Vec::new(),
            toast: None,
            connection: ConnectionVm::default(),
            performance: PerformanceVm::default(),
            channel_switch_started: None,
            channel_generation: 0,
            slack_link_generation: 0,
            edit_generation: 0,
            realtime_insert_started: None,
        };
        state
            .media
            .set_cache_limit(state.core.settings.cache_limit.bytes());
        let mut state = state;
        state.sync_composer_drafts();
        if drafts_load_failed {
            state.show_toast(
                "Local drafts could not be restored. The saved data has been left in place.",
            );
        }
        Ok(state)
    }

    /// No disk or Keychain access on the first render.
    pub fn opening(media: MediaRegistry) -> Self {
        let mut shell = Self::login_with_settings(media, Default::default());
        shell.loading = true;
        shell
    }

    pub fn login(media: MediaRegistry) -> Self {
        Self::login_with_settings(media, super_platinum_core::config::load_settings())
    }

    fn login_with_settings(
        media: MediaRegistry,
        settings: super_platinum_core::config::Settings,
    ) -> Self {
        let core = super_platinum_core::CoreAppState::new(settings);
        let state = Self {
            core,
            managed_drafts: Default::default(),
            draft_writes: Default::default(),
            media,
            media_epoch: 0,
            huddle_ui: Default::default(),
            background_uri: None,
            signed_in: false,
            loading: false,
            accounts: Vec::new(),
            self_account: SelfAccountVm::default(),
            workspaces: Vec::new(),
            active_workspace: 0,
            channels: Vec::new(),
            sidebar_sections: Vec::new(),
            sidebar_memory: Default::default(),
            active_channel: 0,
            channel_rows: Vec::new(),
            channel_geometry: Default::default(),
            fixture_channel_messages: None,
            messages: Vec::new(),
            timeline_start: 0,
            timeline_end: 0,
            row_heights: HashMap::new(),
            message_arrivals: HashMap::new(),
            pending_delivery: HashMap::new(),
            selection_pinned: false,
            upload_ui_epoch: 0,
            file_events: Default::default(),
            file_share: Default::default(),
            file_share_fixture: None,
            stick_to_bottom: true,
            loading_older: false,
            chat_paused: false,
            main_view: MainView::Home,
            channel_tab: ChannelTab::Messages,
            channel_tabs: Default::default(),
            dm_query: String::new(),
            dm_unread_only: false,
            activity_tab: ActivityTab::default(),
            surfaces: HashMap::new(),
            profile_hover: None,
            profile_hover_generation: 0,
            profile_hover_card_active: false,
            overlay: None,
            reaction_target: None,
            composer_emoji: Default::default(),
            forward_target: None,
            emoji_catalog: Default::default(),
            emoji_preferences: Default::default(),
            settings_section: SettingsSection::Appearance,
            notification_schedule: Default::default(),
            presentation_notifications: Default::default(),
            global_notifications: Default::default(),
            storage: StoragePanel::default(),
            palette_query: String::new(),
            palette_selected: 0,
            palette_generation: 0,
            shortcut_generation: 0,
            palette_opening: None,
            search_query: String::new(),
            search_input_revision: 0,
            search_focus_at_end: false,
            search_open_generation: 0,
            search_opening: None,
            search_filters: None,
            search_filter_error: None,
            search_results: Vec::new(),
            search_pager: Default::default(),
            later_open_generation: 0,
            unread_anchor: None,
            manual_unread: Default::default(),
            read_presentation: Default::default(),
            read_settings: Default::default(),
            read_shortcuts: Default::default(),
            bulk_read: Default::default(),
            thread_unread: Default::default(),
            thread_root: None,
            thread_messages: Vec::new(),
            thread_window: Default::default(),
            thread_at_bottom: true,
            thread_generation: 0,
            thread_target: None,
            thread_loading: false,
            thread_error: None,
            thread_history: Default::default(),
            profile_user: None,
            profile_generation: 0,
            profile_dm_generation: 0,
            profile_dm_opening: None,
            profile_dm_link_generation: None,
            group_panel: None,
            profile_pane_width: 495.0,
            profile_menu_open: false,
            self_menu_notifications_open: false,
            slack_lists: crate::slack_lists::State::default(),
            self_status: crate::self_status::State::default(),
            profile_edit: crate::profile_edit::State::default(),
            profile_title: crate::profile_title::State::default(),
            channel_mute: crate::channel_mute::State::default(),
            upload_checks: crate::messaging::delivery_check::State::default(),
            upload_journal: crate::messaging::upload_journal::State::default(),
            dock_badge_cache: super::dock_badge::Cache::default(),
            self_menu_mutations: Default::default(),
            profile_vip_pending: Default::default(),
            viewer: None,
            file_refresh: Default::default(),
            canvas: None,
            canvas_checklist: Default::default(),
            canvas_create: None,
            canvas_creation: Default::default(),
            app_home: Default::default(),
            app_modal: None,
            message_buttons: crate::message_buttons::State::default(),
            app_modal_closes: Vec::new(),
            toast: None,
            connection: ConnectionVm::default(),
            performance: PerformanceVm::default(),
            channel_switch_started: None,
            channel_generation: 0,
            slack_link_generation: 0,
            edit_generation: 0,
            realtime_insert_started: None,
        };
        state
            .media
            .set_cache_limit(state.core.settings.cache_limit.bytes());
        state
    }

    pub fn fixture(media: MediaRegistry) -> Self {
        let mut core = fixture_core();
        // Existing offline scenes intentionally exercise Slack's left-off
        // opening. Production missing preferences remain unavailable/falsy.
        for workspace in core.workspaces.values_mut() {
            workspace.read_preferences = super_platinum_core::state::ReadPreferences::from_prefs(
                &super_platinum_core::state::ReadMode::LeftOffAndRead.patch(),
            );
        }
        if matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("channel-join" | "channel-leave" | "channel-leave-last" | "channel-join-composer")
        ) {
            let name = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap();
            let channel = core
                .workspaces
                .get_mut("T1")
                .unwrap()
                .channels
                .get_mut("C2")
                .unwrap();
            channel
                .extra
                .insert("is_member".into(), name.starts_with("channel-leave").into());
            channel.is_private = name.starts_with("channel-leave");
            channel.extra.insert(
                "num_members".into(),
                if name == "channel-leave-last" {
                    1.into()
                } else {
                    2.into()
                },
            );
        }
        // Fixture avatars use opaque media IDs backed by local placeholder bytes so
        // profile/DM chrome is not limited to letter placeholders offline.
        let placeholder = include_bytes!("../../../assets/icons/icon-512.png");
        for url in [
            "https://example.test/you.png",
            "https://example.test/maya.png",
            "https://example.test/maya-lg.png",
            "https://example.test/jules.png",
            "https://example.test/bot.png",
        ] {
            let id = media.register(MediaAssetKind::Avatar, url, "image/png", false);
            media.insert(id, "image/png", placeholder.as_slice());
        }
        let maya_avatar = media.register(
            MediaAssetKind::Avatar,
            "https://example.test/maya.png",
            "image/png",
            false,
        );
        let jules_avatar = media.register(
            MediaAssetKind::Avatar,
            "https://example.test/jules.png",
            "image/png",
            false,
        );
        let bot_avatar = media.register(
            MediaAssetKind::Avatar,
            "https://example.test/bot.png",
            "image/png",
            false,
        );
        let profile_avatar = media.register_avatar("U1", "https://example.test/maya-lg.png");
        media.insert(profile_avatar, "image/png", placeholder.as_slice());
        // Filler first so virtualization stays exercised; rich parity messages at
        // the end remain visible when scrolled to bottom.
        let mut messages = Vec::new();
        for index in 1..=200 {
            let same_author = index % 9;
            messages.push(MessageVm {
                id: format!("m{index}"),
                ts: format!("171979{:04}.000{index}", index),
                author: format!("Teammate {same_author}"),
                timestamp: format!("09:{:02} AM", index % 60),
                avatar_initials: format!("T{same_author}"),
                body: vec![RichNode::Text(format!(
                    "Fixture message {index} keeps the timeline long enough to exercise bounded rendering and native text selection."
                ))],
                compact: index > 1 && (index % 9) == ((index - 1) % 9),
                date_label: if index == 1 {
                    Some(super_platinum_core::state::format_ts_date_label("1719790001.0001"))
                } else {
                    None
                },
                ..Default::default()
            });
        }
        messages.push(MessageVm {
            id: "m-rich-1".into(),
            ts: "1719800000.000100".into(),
            user_id: Some("U1".into()),
            author: "Maya Chen".into(),
            timestamp: super_platinum_core::state::format_ts_hm("1719800000.000100"),
            avatar_initials: "MC".into(),
            avatar: Some(maya_avatar.clone()),
            body: vec![
                RichNode::Text(
                    "Morning! The desktop migration branch is ready for review. ".into(),
                ),
                RichNode::Link {
                    label: "Open the checklist".into(),
                    url: "https://example.com/checklist".into(),
                },
                RichNode::Text(" when you have a minute.".into()),
            ],
            reactions: vec![reaction("rocket", 4, false), reaction("eyes", 2, false)],
            reply_count: 3,
            date_label: Some(super_platinum_core::state::format_ts_date_label(
                "1719800000.000100",
            )),
            ..Default::default()
        });
        messages.push(MessageVm {
            id: "m-rich-2".into(),
            ts: "1719800400.000200".into(),
            user_id: Some("U2".into()),
            author: "Jules".into(),
            timestamp: super_platinum_core::state::format_ts_hm("1719800400.000200"),
            avatar_initials: "JU".into(),
            avatar: Some(jules_avatar),
            body: vec![
                RichNode::Text("I measured the warm path:".into()),
                RichNode::Code("cache → render → background refresh".into()),
                RichNode::Quote(vec![RichNode::Text(
                    "Cached content stays visible if refresh fails.".into(),
                )]),
            ],
            edited: true,
            reactions: vec![reaction("white_check_mark", 6, false)],
            reply_count: 1,
            show_unread_divider: true,
            ..Default::default()
        });
        messages.push(MessageVm {
            id: "m-rich-3".into(),
            ts: "1719800800.000300".into(),
            author: "Deploy Bot".into(),
            timestamp: super_platinum_core::state::format_ts_hm("1719800800.000300"),
            avatar_initials: "DB".into(),
            avatar: Some(bot_avatar),
            body: vec![RichNode::Text("Build #482 passed on main.".into())],
            is_app: true,
            ..Default::default()
        });
        messages.push(MessageVm {
            id: "m-rich-4".into(),
            ts: "1719801000.000400".into(),
            author: "Ari".into(),
            timestamp: super_platinum_core::state::format_ts_hm("1719801000.000400"),
            avatar_initials: "AR".into(),
            body: vec![
                RichNode::Text("The multiline composer and custom emoji fixture both pass ".into()),
                RichNode::Emoji {
                    name: "ship".into(),
                    glyph: "🚢".into(),
                },
            ],
            ..Default::default()
        });
        let workspace = core.workspaces.get("T1").cloned().unwrap_or_else(|| {
            super_platinum_core::state::Workspace::from_session(
                &super_platinum_core::config::WorkspaceSession {
                    team_id: "T1".into(),
                    enterprise_id: None,
                    user_id: "U0".into(),
                    name: "Echonet".into(),
                    url: "https://echonet.slack.com".into(),
                    token: String::new(),
                },
            )
        });
        let mut sidebar_memory = super_platinum_core::state::SidebarMemory::default();
        let (channels, sidebar_sections, default_active, _) =
            project_channels(&workspace, &media, None, &mut sidebar_memory);
        let active_channel = channels
            .iter()
            .position(|channel| channel.id == "C2")
            .unwrap_or(default_active);
        let timeline_end = messages.len();
        let channel_rows = messages
            .iter()
            .enumerate()
            .map(|(source_index, message)| crate::channel_vm::ChannelRow {
                id: message.id.clone(),
                ts: message.ts.clone(),
                pending: message.pending,
                source_index,
                compact: message.compact,
                date_label: message.date_label.clone(),
                show_unread_divider: message.show_unread_divider,
            })
            .collect();
        let fixture_channel_messages = Some(messages.clone());
        let messages = messages[timeline_end.saturating_sub(100)..].to_vec();
        let self_fixture_account = core
            .workspaces
            .get("T1")
            .map(|workspace| crate::state::project_self_account(workspace, &media))
            .unwrap_or_default();
        Self {
            core,
            managed_drafts: Default::default(),
            draft_writes: Default::default(),
            media,
            media_epoch: 0,
            huddle_ui: Default::default(),
            background_uri: None,
            signed_in: true,
            loading: false,
            accounts: vec![AccountVm {
                id: "fixture".into(),
                label: "Fixture account".into(),
                active: true,
            }],
            self_account: self_fixture_account,
            workspaces: vec![
                WorkspaceVm {
                    id: "T1".into(),
                    name: "Echonet".into(),
                    initials: "E".into(),
                },
                WorkspaceVm {
                    id: "T2".into(),
                    name: "Super Platinum Lab".into(),
                    initials: "S".into(),
                },
            ],
            active_workspace: 0,
            channels,
            sidebar_sections,
            sidebar_memory,
            active_channel,
            channel_rows,
            channel_geometry: crate::timeline_geometry::Geometry::uniform(timeline_end),
            fixture_channel_messages,
            messages,
            timeline_start: timeline_end.saturating_sub(100),
            timeline_end,
            row_heights: HashMap::new(),
            message_arrivals: HashMap::new(),
            pending_delivery: HashMap::new(),
            selection_pinned: false,
            upload_ui_epoch: 0,
            file_events: Default::default(),
            file_share: Default::default(),
            file_share_fixture: None,
            stick_to_bottom: true,
            loading_older: false,
            chat_paused: false,
            main_view: MainView::Home,
            channel_tab: ChannelTab::Messages,
            channel_tabs: Default::default(),
            dm_query: String::new(),
            dm_unread_only: false,
            activity_tab: ActivityTab::default(),
            surfaces: HashMap::new(),
            profile_hover: None,
            profile_hover_generation: 0,
            profile_hover_card_active: false,
            overlay: None,
            reaction_target: None,
            composer_emoji: Default::default(),
            forward_target: None,
            emoji_catalog: Default::default(),
            emoji_preferences: Default::default(),
            settings_section: SettingsSection::Appearance,
            notification_schedule: Default::default(),
            presentation_notifications: Default::default(),
            global_notifications: Default::default(),
            storage: StoragePanel::default(),
            palette_query: String::new(),
            palette_selected: 0,
            palette_generation: 0,
            shortcut_generation: 0,
            palette_opening: None,
            search_query: String::new(),
            search_input_revision: 0,
            search_focus_at_end: false,
            search_open_generation: 0,
            search_opening: None,
            search_filters: None,
            search_filter_error: None,
            search_results: Vec::new(),
            search_pager: Default::default(),
            later_open_generation: 0,
            unread_anchor: None,
            manual_unread: Default::default(),
            read_presentation: Default::default(),
            read_settings: Default::default(),
            read_shortcuts: Default::default(),
            bulk_read: Default::default(),
            thread_unread: Default::default(),
            thread_root: None,
            thread_messages: Vec::new(),
            thread_window: Default::default(),
            thread_at_bottom: true,
            thread_generation: 0,
            thread_target: None,
            thread_loading: false,
            thread_error: None,
            thread_history: Default::default(),
            profile_user: None,
            profile_generation: 0,
            profile_dm_generation: 0,
            profile_dm_opening: None,
            profile_dm_link_generation: None,
            group_panel: None,
            profile_pane_width: 495.0,
            profile_menu_open: false,
            self_menu_notifications_open: false,
            slack_lists: crate::slack_lists::State::default(),
            self_status: crate::self_status::State::default(),
            profile_edit: crate::profile_edit::State::default(),
            profile_title: crate::profile_title::State::default(),
            channel_mute: crate::channel_mute::State::default(),
            upload_checks: crate::messaging::delivery_check::State::default(),
            upload_journal: crate::messaging::upload_journal::State::default(),
            dock_badge_cache: super::dock_badge::Cache::default(),
            self_menu_mutations: Default::default(),
            profile_vip_pending: Default::default(),
            viewer: None,
            file_refresh: Default::default(),
            canvas: None,
            canvas_checklist: Default::default(),
            canvas_create: None,
            canvas_creation: Default::default(),
            app_home: Default::default(),
            app_modal: None,
            message_buttons: crate::message_buttons::State::default(),
            app_modal_closes: Vec::new(),
            toast: None,
            connection: ConnectionVm::default(),
            performance: PerformanceVm::default(),
            channel_switch_started: None,
            channel_generation: 0,
            slack_link_generation: 0,
            edit_generation: 0,
            realtime_insert_started: None,
        }
    }

    pub(crate) fn fixture_variant(media: MediaRegistry, name: &str) -> Self {
        let mut state = Self::fixture(media);
        match name {
            "channel-canvas-surface" | "channel-canvas-surface-narrow" => {
                crate::canvas::surface::install_fixture(&mut state)
            }
            "channel-tabs-canvas" | "channel-tabs-canvas-narrow" => {
                crate::channel_tabs::fixture::install(&mut state)
            }
            "channel-window-10000" => crate::channel_vm::channel_window_fixture(&mut state),
            "member-add"
            | "member-live-refresh"
            | "member-add-error"
            | "member-remove"
            | "channel-members"
            | "member-add-narrow"
            | "member-remove-narrow" => {
                let channel = state
                    .core
                    .workspaces
                    .get_mut("T1")
                    .unwrap()
                    .channels
                    .get_mut("C2")
                    .unwrap();
                channel.is_private = true;
            }
            "thread-marked-unread" | "thread-marked-unread-narrow" => {
                crate::thread_unread::fixture(&mut state)
            }
            "thread-counts-mentions"
            | "thread-counts-unread"
            | "thread-counts-clear"
            | "thread-counts-narrow" => {
                let counts = super_platinum_core::slack::models::ThreadCounts {
                    has_unreads: name != "thread-counts-clear",
                    mention_count: if matches!(
                        name,
                        "thread-counts-mentions" | "thread-counts-narrow"
                    ) {
                        127
                    } else {
                        0
                    },
                    ..Default::default()
                };
                state
                    .core
                    .workspaces
                    .get_mut("T1")
                    .unwrap()
                    .thread_counts
                    .apply_live(counts);
            }
            "thread-feed"
            | "thread-feed-empty"
            | "thread-feed-error"
            | "thread-feed-loading"
            | "thread-feed-updated"
            | "thread-feed-narrow" => crate::thread_feed::fixture(&mut state, name),
            "thread-follow-on"
            | "thread-follow-off"
            | "thread-follow-pending"
            | "thread-follow-narrow" => crate::thread_subscription::fixture(&mut state, name),
            "thread-list-unread" => crate::thread_unread::list_fixture(&mut state),
            "slack-list-subitem-create"
            | "slack-list-subitem-create-narrow"
            | "slack-list-subitem-uncertain"
            | "slack-list-subitem-uncertain-narrow" => {
                crate::slack_lists::subitem::fixture::install(&mut state, name)
            }
            "slack-list-view-create"
            | "slack-list-view-create-narrow"
            | "slack-list-view-rename"
            | "slack-list-view-rename-narrow"
            | "slack-list-column-rename"
            | "slack-list-column-rename-narrow" => {
                crate::slack_lists::rename::fixture::install(&mut state, name)
            }
            "slack-list-board-move"
            | "slack-list-board-move-narrow"
            | "slack-list-board-move-checkbox"
            | "slack-list-board-move-checkbox-narrow" => {
                crate::slack_lists::fixture::board_move::install(&mut state, name)
            }
            "slack-list-view-order" | "slack-list-view-order-narrow" => {
                crate::slack_lists::fixture::view_order::install(&mut state, name);
            }
            "slack-list-field-delete" | "slack-list-field-delete-narrow" => {
                crate::slack_lists::fixture::field_delete::install(&mut state, name);
            }
            "slack-list-view-grouping"
            | "slack-list-view-grouping-narrow"
            | "slack-list-board-grouping"
            | "slack-list-board-grouping-narrow" => {
                crate::slack_lists::fixture::view_grouping::install(&mut state, name);
            }
            "slack-list-view-sort"
            | "slack-list-view-sort-narrow"
            | "slack-list-ordered-sort"
            | "slack-list-ordered-sort-narrow" => {
                crate::slack_lists::fixture::view_sort::install(&mut state, name);
            }
            "slack-list-vote-cells"
            | "slack-list-vote-cells-narrow"
            | "slack-list-vote-board"
            | "slack-list-vote-board-narrow"
            | "slack-list-vote-detail"
            | "slack-list-vote-detail-narrow" => {
                crate::slack_lists::fixture::vote::install(&mut state, name);
            }
            "slack-list-rating-cells" | "slack-list-rating-cells-narrow" => {
                crate::slack_lists::fixture::rating::install(&mut state, name);
            }
            "slack-list-people-avatars"
            | "slack-list-people-avatars-narrow"
            | "slack-list-people-avatars-board"
            | "slack-list-people-avatars-board-narrow"
            | "slack-list-people-avatars-detail"
            | "slack-list-people-avatars-detail-narrow" => {
                crate::slack_lists::fixture::people_avatars::install(&mut state, name);
            }
            "slack-list-people-settings"
            | "slack-list-people-settings-narrow"
            | "slack-list-channel-settings"
            | "slack-list-channel-settings-narrow" => {
                crate::slack_lists::fixture::entity_settings::install(&mut state, name);
            }
            "slack-list-people-defaults-single-create"
            | "slack-list-people-defaults-single-create-narrow"
            | "slack-list-people-defaults-multiple-create"
            | "slack-list-people-defaults-multiple-create-narrow"
            | "slack-list-people-defaults-null-create"
            | "slack-list-people-defaults-null-create-narrow"
            | "slack-list-people-defaults-omitted-create"
            | "slack-list-people-defaults-omitted-create-narrow"
            | "slack-list-channel-defaults-single-create"
            | "slack-list-channel-defaults-single-create-narrow"
            | "slack-list-channel-defaults-multiple-create"
            | "slack-list-channel-defaults-multiple-create-narrow"
            | "slack-list-channel-defaults-null-create"
            | "slack-list-channel-defaults-null-create-narrow"
            | "slack-list-channel-defaults-omitted-create"
            | "slack-list-channel-defaults-omitted-create-narrow"
            | "slack-list-vote-create-default"
            | "slack-list-vote-create-default-narrow"
            | "slack-list-vote-create-custom"
            | "slack-list-vote-create-custom-narrow"
            | "slack-list-text-field-create"
            | "slack-list-text-field-create-narrow"
            | "slack-list-number-field-create"
            | "slack-list-number-field-create-narrow"
            | "slack-list-number-precision-field-create"
            | "slack-list-number-precision-field-create-narrow"
            | "slack-list-checkbox-field-create"
            | "slack-list-checkbox-field-create-narrow"
            | "slack-list-date-field-create"
            | "slack-list-date-field-create-narrow"
            | "slack-list-select-field-create"
            | "slack-list-select-field-create-narrow"
            | "slack-list-multi-select-field-create"
            | "slack-list-multi-select-field-create-narrow"
            | "slack-list-people-field-create"
            | "slack-list-people-field-create-narrow"
            | "slack-list-link-field-create"
            | "slack-list-link-field-create-narrow"
            | "slack-list-channel-field-create"
            | "slack-list-channel-field-create-narrow"
            | "slack-list-rating-field-create"
            | "slack-list-rating-field-create-narrow"
            | "slack-list-rating-default-field-create"
            | "slack-list-rating-default-field-create-narrow"
            | "slack-list-rating-standard-field-create"
            | "slack-list-rating-standard-field-create-narrow"
            | "slack-list-rating-custom-field-create"
            | "slack-list-rating-custom-field-create-narrow"
            | "slack-list-vote-field-create"
            | "slack-list-vote-field-create-narrow" => {
                crate::slack_lists::fixture::text_field_create::install(&mut state, name);
            }
            "slack-list-compound-filter" | "slack-list-compound-filter-narrow" => {
                crate::slack_lists::fixture::compound_filter::install(&mut state, name);
            }
            "slack-list-number-filter" | "slack-list-number-filter-narrow" => {
                crate::slack_lists::fixture::number_filter::install(&mut state, name);
            }
            "slack-list-select-filter" | "slack-list-select-filter-narrow" => {
                crate::slack_lists::fixture::select_filter::install(&mut state, name);
            }
            "slack-list-view-filter" | "slack-list-view-filter-narrow" => {
                crate::slack_lists::fixture::view_filter::install(&mut state, name);
            }
            "slack-list-field-visibility" | "slack-list-field-visibility-narrow" => {
                crate::slack_lists::fixture::field_visibility::install(&mut state, name);
            }
            "slack-list-number-rename" | "slack-list-number-rename-narrow" => {
                crate::slack_lists::fixture::number_rename::install(&mut state, name);
            }
            "slack-list-number-precision" | "slack-list-number-precision-narrow" => {
                crate::slack_lists::fixture::column_precision::install(&mut state, name);
            }
            "slack-list-hydration" | "slack-list-hydration-narrow" => {
                crate::slack_lists::fixture::hydration::install(&mut state, name)
            }
            "slack-list-find"
            | "slack-list-find-narrow"
            | "slack-list-find-board"
            | "slack-list-find-board-narrow"
            | "slack-list-find-empty"
            | "slack-list-find-empty-narrow" => {
                crate::slack_lists::fixture::find::install(&mut state, name);
            }
            "slack-list-board-heading"
            | "slack-list-board-heading-narrow"
            | "slack-list-board-heading-checked-narrow" => {
                crate::slack_lists::fixture::board_heading::install(&mut state, name)
            }
            "slack-list-board-add" | "slack-list-board-add-narrow" => {
                crate::slack_lists::fixture::board_creation::install(&mut state, name)
            }
            "slack-list"
            | "slack-list-narrow"
            | "slack-list-readonly"
            | "slack-list-incomplete"
            | "slack-list-error"
            | "slack-list-loading" => crate::slack_lists::fixture::install(&mut state, name),
            "profile-title" | "profile-title-narrow" => {
                crate::profile_title::fixture::install(&mut state)
            }
            "profile-photo" | "profile-photo-narrow" => {
                crate::profile_edit::photo::fixture::install(&mut state)
            }
            "profile-pronunciation" | "profile-pronunciation-narrow" => {
                crate::profile_edit::sections::fixture::install_pronunciation(&mut state);
            }
            "profile-sections" | "profile-sections-narrow" => {
                crate::profile_edit::sections::fixture::install(&mut state)
            }
            "profile-edit" | "profile-edit-narrow" => {
                crate::profile_edit::fixture::install(&mut state)
            }
            "self-status-editor" | "self-status-editor-narrow" => {
                crate::self_status::fixture::install(&mut state)
            }
            "thread-broadcast" | "thread-broadcast-narrow" => {
                crate::messaging::thread_broadcast_check::install(&mut state)
            }
            "message-broadcast" | "message-broadcast-narrow" => {
                crate::message_vm::broadcast_check::install(&mut state)
            }
            "upload-restart" | "upload-restart-narrow" => {
                crate::messaging::upload_journal::fixture::install(&mut state)
            }
            "upload-delivery-check" | "upload-delivery-check-narrow" => {
                crate::messaging::delivery_check::fixture::install(&mut state)
            }
            "emoji-native" | "emoji-native-narrow" => crate::emoji_native::install(&mut state),
            "composer-emoji" | "composer-emoji-narrow" => {
                crate::composer_emoji::check::install(&mut state)
            }
            "global-notifications" | "global-notifications-narrow" => {
                crate::global_notifications::fixture::install(&mut state)
            }
            "notification-presentation" | "notification-presentation-narrow" => {
                crate::presentation_notifications::fixture::install(&mut state)
            }
            "notification-schedule" | "notification-schedule-narrow" => {
                crate::notification_schedule::fixture::install(&mut state)
            }
            "channel-notifications" | "channel-notifications-narrow" => {
                crate::channel_mute::settings::fixture::install(&mut state)
            }
            "channel-mute" | "channel-mute-narrow" => {
                crate::channel_mute::fixture::install(&mut state)
            }
            "system-messages" => crate::fixture::system_messages(&mut state),
            "slack-links" | "slack-links-narrow" => crate::slack_links_check::install(&mut state),
            "app-home" | "app-home-narrow" => crate::app_home::fixture::install(&mut state),
            "app-calendar"
            | "app-calendar-narrow"
            | "app-calendar-modern"
            | "app-calendar-modern-narrow"
            | "app-calendar-connect"
            | "app-calendar-connect-narrow"
            | "app-calendar-uncertain"
            | "app-calendar-uncertain-narrow" => {
                crate::message_buttons::shortcuts::fixture::calendar(&mut state, name)
            }
            "app-shortcuts" | "app-shortcuts-narrow" => {
                crate::message_buttons::shortcuts::fixture::install(&mut state)
            }
            "app-modal-users" | "app-modal-users-narrow" => {
                crate::app_modal::users_fixture::install(&mut state)
            }
            "app-modal-multi-conversation" | "app-modal-multi-conversation-narrow" => {
                crate::app_modal::multi_conversation_fixture::install(&mut state);
            }
            "message-external-select"
            | "message-external-select-narrow"
            | "app-modal-external-select"
            | "app-modal-external-select-narrow" => {
                crate::message_buttons::external_fixture::install(&mut state);
            }
            "message-multi-conversations-select" | "message-multi-conversations-select-narrow" => {
                crate::message_buttons::multi_conversation_fixture::install(&mut state);
            }
            "message-multi-external-select" | "message-multi-external-select-narrow" => {
                crate::message_buttons::multi_external_fixture::install(&mut state);
            }
            "app-modal-multi-external-select" | "app-modal-multi-external-select-narrow" => {
                crate::app_modal::multi_external_fixture::install(&mut state);
            }
            "app-modal-multi-channel" | "app-modal-multi-channel-narrow" => {
                crate::app_modal::multi_channel_fixture::install(&mut state);
            }
            "message-multi-channels-select" | "message-multi-channels-select-narrow" => {
                crate::message_buttons::multi_channel_fixture::install(&mut state);
            }
            "app-modal-channel" | "app-modal-channel-narrow" => {
                crate::app_modal::channel_fixture::install(&mut state);
            }
            "app-modal-conversation" | "app-modal-conversation-narrow" => {
                crate::app_modal::conversation_fixture::install(&mut state);
            }
            "app-modal-user" | "app-modal-user-narrow" => {
                crate::app_modal::users_fixture::install_single(&mut state)
            }
            "app-modal-form"
            | "app-modal-form-narrow"
            | "app-modal-lifecycle"
            | "app-modal-lifecycle-narrow" => crate::app_modal::fixture::install(&mut state),
            "message-multi-users-select" | "message-multi-users-select-narrow" => {
                crate::message_buttons::multi_user_fixture::install(&mut state)
            }
            "message-users-select" | "message-users-select-narrow" => {
                crate::message_buttons::user_fixture::install(&mut state)
            }
            "message-overflow" | "message-overflow-narrow" => {
                crate::message_buttons::overflow_fixture::install(&mut state)
            }
            "message-channels-select" | "message-channels-select-narrow" => {
                crate::message_buttons::channel_fixture::install(&mut state);
            }
            "message-conversations-select" | "message-conversations-select-narrow" => {
                crate::message_buttons::conversation_fixture::install(&mut state)
            }
            "message-timepicker" | "message-timepicker-narrow" => {
                crate::message_buttons::time_fixture::install(&mut state)
            }
            "message-datepicker" | "message-datepicker-narrow" => {
                crate::message_buttons::date_fixture::install(&mut state)
            }
            "message-multi-controls" | "message-multi-controls-narrow" => {
                crate::message_buttons::multi_fixture::install(&mut state)
            }
            "message-radio-buttons" | "message-radio-buttons-narrow" => {
                crate::message_buttons::radio_fixture::install(&mut state)
            }
            "message-static-select" | "message-static-select-narrow" => {
                crate::message_buttons::select_fixture::install(&mut state)
            }
            "message-button-actions" | "message-button-actions-narrow" => {
                crate::message_buttons::fixture::install(&mut state)
            }
            "message-button-metadata" | "message-button-metadata-narrow" => {
                crate::message_buttons::fixture::install_metadata(&mut state)
            }
            "button-metadata" | "button-metadata-narrow" => {
                crate::blocks::button_metadata_fixture(&mut state)
            }
            "block-control-values" | "block-control-values-narrow" => {
                crate::blocks::controls_fixture(&mut state)
            }
            "managed-draft-recovery" | "managed-draft-recovery-narrow" => {
                crate::drafts::session::fixture(&mut state)
            }
            "rejected-send" | "rejected-send-narrow" => {
                crate::drafts::rejected_fixture::apply(&mut state)
            }
            "rejected-thread-return" | "rejected-thread-return-narrow" => {
                crate::drafts::rejected_thread_fixture::apply(&mut state)
            }
            "draft-native-projection" | "draft-native-projection-narrow" => {
                crate::drafts::projection_fixture::apply(&mut state)
            }
            "draft-remote-review" | "draft-remote-review-narrow" => {
                crate::drafts::remote_review_fixture::apply(&mut state)
            }
            "draft-schedule-recovery" | "draft-schedule-recovery-narrow" => {
                crate::drafts::recovery::schedule_fixture::apply(&mut state)
            }
            "draft-update-recovery" | "draft-update-recovery-narrow" => {
                crate::drafts::recovery::update_fixture::apply(&mut state)
            }
            "managed-draft-schedule" | "managed-draft-schedule-narrow" => {
                crate::drafts::schedule::fixture::apply(&mut state)
            }
            #[cfg(feature = "protected-native-fixture")]
            "protected-owned-baseline" | "protected-owned-baseline-narrow" => {
                crate::drafts::protected::production::baseline_native::apply(&mut state)
            }
            #[cfg(feature = "protected-native-fixture")]
            "protected-owned-slash" | "protected-owned-slash-narrow" => {
                crate::drafts::protected::slash_native::apply(&mut state)
            }
            #[cfg(feature = "protected-native-fixture")]
            "protected-owned-send" | "protected-owned-send-narrow" => {
                crate::drafts::protected::owned_native::apply(&mut state)
            }
            #[cfg(feature = "protected-native-fixture")]
            "protected-drafts" | "protected-drafts-narrow" => {
                crate::drafts::protected::native_fixture::apply(&mut state)
            }
            #[cfg(feature = "protected-native-fixture")]
            "protected-enrollment" | "protected-enrollment-narrow" => {
                crate::drafts::protected::enrollment_fixture::apply(&mut state)
            }
            #[cfg(feature = "protected-native-fixture")]
            "protected-adoption" | "protected-adoption-narrow" => {
                crate::drafts::protected::adoption_fixture::apply(&mut state)
            }
            "managed-draft-send" | "managed-draft-send-narrow" => {
                crate::drafts::send::fixture::apply(&mut state)
            }
            "scheduled-attachments" | "scheduled-attachments-narrow" => {
                crate::scheduled::attachments::fixture::apply(&mut state)
            }
            "user-links" | "user-links-narrow" => {
                crate::slack_links::user_fixture::install(&mut state)
            }
            "read-preferences" | "read-preferences-narrow" => {
                crate::read_preferences::settings::fixture::install(&mut state)
            }
            "read-navigation" | "read-navigation-narrow" => {
                crate::read_preferences::navigation_fixture::install(&mut state)
            }
            "reading-shortcuts" | "reading-shortcuts-narrow" => {
                crate::read_shortcuts::fixture::install(&mut state)
            }
            "bulk-read" | "bulk-read-narrow" => {
                crate::read_shortcuts::bulk::fixture::install(&mut state)
            }
            "send-outcomes" | "send-outcomes-narrow" | "send-outcomes-thread" => {
                crate::messaging::send_outcomes_fixture(&mut state, name)
            }
            "schedule-clear" | "schedule-clear-narrow" | "schedule-clear-thread" => {
                crate::view::scheduled::clear_check::install(&mut state, name)
            }
            "message-marked-unread" | "message-marked-unread-narrow" => {
                crate::unread::fixture(&mut state)
            }
            "usergroup-members" | "usergroup-channels" => {
                let workspace = state.core.workspaces.values_mut().next().unwrap();
                let group = super_platinum_core::slack::models::UserGroup {
                    id: "S1".into(),
                    members_loaded: true,
                    name: "Desktop crew".into(),
                    handle: "desktop-crew".into(),
                    description: "Making a focused, fast desktop client together.".into(),
                    users: workspace.users.keys().cloned().collect(),
                    prefs: super_platinum_core::slack::models::UserGroupPrefs {
                        channels: vec!["C2".into()],
                        groups: vec![],
                    },
                    ..Default::default()
                };
                workspace.usergroups.insert(group.id.clone(), group);
                for messages in workspace.messages.values_mut() {
                    if let Some(message) = messages.messages.last_mut() {
                        message.text = Some("<!subteam^S1> let’s ship this!".into());
                        message.blocks.clear();
                    }
                }
                state.refresh_from_core();
                state.group_panel = Some(crate::usergroups::GroupPanel {
                    id: "S1".into(),
                    loading: false,
                    error: false,
                    channels: name == "usergroup-channels",
                });
            }
            // Rail account button with notifications snoozed: the avatar notch
            // and the "Z" presence glyph only appear in this state.
            "rail-account-snoozed" => {
                state.self_account.snoozed = true;
                state.self_account.presence = PresenceVm::Away;
            }
            "local-drafts"
            | "local-drafts-loading"
            | "local-drafts-error"
            | "local-drafts-error-narrow" => {
                state.main_view = MainView::Scheduled;
                let team = state.core.active_team.clone().unwrap();
                state
                    .core
                    .scheduled
                    .ensure_scope(state.core.account_epoch, &team);
                let (generation, limit) = state.core.scheduled.begin(false).unwrap();
                state.core.scheduled.apply(
                    generation,
                    super_platinum_core::scheduled::ScheduledPage {
                        drafts: vec![],
                        has_more: false,
                    },
                    limit,
                );
                state.core.scheduled.filter = super_platinum_core::scheduled::DraftFilter::Drafts;
                for (channel, thread, text) in [
                    (
                        "C_COLD",
                        None,
                        "A saved draft survives a missing conversation cache. 🚀",
                    ),
                    (
                        "C2",
                        Some("1719800000.000001"),
                        "An unfinished reply stays in its original thread.",
                    ),
                ] {
                    state.core.drafts.capture(
                        &super_platinum_core::drafts::DraftKey {
                            team: team.clone(),
                            channel: channel.into(),
                            thread: thread.map(str::to_owned),
                        },
                        &super_platinum_core::ComposerState::with_text(text),
                        &[],
                    );
                }
            }
            "scheduled"
            | "scheduled-empty"
            | "scheduled-error"
            | "scheduled-edit"
            | "scheduled-send-confirm"
            | "drafts"
            | "drafts-empty"
            | "drafts-edit"
            | "drafts-conflict"
            | "drafts-no-destination" => {
                state.main_view = MainView::Scheduled;
                let team = state.core.active_team.clone().unwrap();
                state
                    .core
                    .scheduled
                    .ensure_scope(state.core.account_epoch, &team);
                let (generation, limit) = state.core.scheduled.begin(false).unwrap();
                let drafts = if matches!(name, "scheduled-empty" | "drafts-empty") {
                    vec![]
                } else {
                    vec![serde_json::from_value(serde_json::json!({
                    "id":"DrFixture","date_scheduled":super_platinum_core::state::now_secs()+86400,"destinations":[{"channel_id":"C2","thread_ts":"1790000000.000001"}],"file_ids":["Ffixture"],
                    "blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Release review 🚀","style":{"bold":true}},{"type":"text","text":"\nCheck the final changes before we ship. The report is attached."}]}]}]
                })).unwrap()]
                };
                state.core.scheduled.apply(
                    generation,
                    super_platinum_core::scheduled::ScheduledPage {
                        drafts,
                        has_more: false,
                    },
                    limit,
                );
                if name.starts_with("drafts") {
                    state.core.scheduled.filter =
                        super_platinum_core::scheduled::DraftFilter::Drafts;
                    if let Some(item) = state.core.scheduled.items.get_mut("DrFixture") {
                        item.post_at = 0;
                        if name == "drafts-no-destination" {
                            item.destinations.clear();
                        }
                    }
                }
                if name == "scheduled-error" {
                    state.core.scheduled.error = Some(
                        "Could not refresh scheduled messages. Your saved queue is still here."
                            .into(),
                    );
                }
            }
            "bookmarks"
            | "bookmarks-narrow"
            | "bookmarks-edit"
            | "bookmarks-remove"
            | "bookmarks-error"
            | "bookmarks-folder"
            | "bookmarks-folder-remove"
            | "bookmarks-move"
            | "bookmarks-emoji"
            | "bookmarks-emoji-narrow" => {
                state.channel_tab = ChannelTab::Bookmarks;
                let team = state.core.active_team.clone().unwrap();
                let channel = state.core.active_channel.clone().unwrap();
                let store = &mut state.core.bookmarks;
                store.ensure_scope(state.core.account_epoch, &team, &channel);
                let g = store.begin().unwrap();
                let items=serde_json::from_value(serde_json::json!([
                    {"id":"Bk1","title":"Design system","link":"https://vercel.com/geist/introduction","type":"link","rank":"a"},
                    {"id":"Bk2","title":"Launch references","type":"local_folder","rank":"b"},
                    {"id":"Bk3","title":"Release checklist · 日本語","link":"https://example.com/release","type":"link","parent_id":"Bk2","rank":"c"},
                    {"id":"Bk4","title":"Product handbook and the decisions behind our next release","link":"https://example.com/docs/handbook","type":"link","rank":"d"}
                ])).unwrap();
                store.apply(g, items);
                if name == "bookmarks-edit" || name == "bookmarks-remove" {
                    store.draft = Some(super_platinum_core::bookmarks::Draft::new(
                        Some(store.items[0].clone()),
                        name == "bookmarks-remove",
                    ));
                }
                if name == "bookmarks-folder" {
                    let mut draft = super_platinum_core::bookmarks::Draft::new(None, false);
                    draft.folder = true;
                    store.draft = Some(draft);
                }
                if name == "bookmarks-folder-remove" {
                    let mut draft = super_platinum_core::bookmarks::Draft::new(
                        Some(store.items[1].clone()),
                        true,
                    );
                    draft.contents =
                        super_platinum_core::bookmarks::descendants(&store.items, "Bk2")
                            .into_iter()
                            .cloned()
                            .collect();
                    store.draft = Some(draft);
                }
                if name == "bookmarks-move" {
                    let mut draft = super_platinum_core::bookmarks::Draft::new(
                        Some(store.items[0].clone()),
                        false,
                    );
                    draft.movement = Some(super_platinum_core::bookmarks::Movement {
                        parent: "Bk2".into(),
                        before: "Bk3".into(),
                    });
                    store.draft = Some(draft);
                }
                if name == "bookmarks-error" {
                    store.error = Some(
                        "Bookmarks could not refresh. Your previous links are still here.".into(),
                    );
                }
                if matches!(name, "bookmarks-emoji" | "bookmarks-emoji-narrow") {
                    crate::bookmarks::emoji::fixture(&mut state);
                }
            }
            "canvas-creation-durable" | "canvas-creation-durable-narrow" => {
                crate::canvas_create::fixture::apply(&mut state);
            }
            "canvas-create" | "canvas-create-narrow" | "canvas-create-error" => {
                state.main_view = MainView::Files;
                let mut draft = crate::canvas_create::Draft::fixture(&state);
                if name == "canvas-create-error" {
                    draft.uncertain = true;
                    draft.error=Some("Slack may have created this canvas. Check Files before creating another. Your draft is retained.".into());
                }
                state.canvas_create = Some(draft);
                state.overlay = Some(Overlay::CanvasCreate);
            }
            "canvas"
            | "canvas-narrow"
            | "canvas-comments"
            | "canvas-comments-narrow"
            | "canvas-comment-resolve"
            | "canvas-comment-resolve-narrow"
            | "canvas-comment-reply"
            | "canvas-comment-reply-narrow"
            | "canvas-checklist"
            | "canvas-checklist-narrow"
            | "canvas-text"
            | "canvas-text-narrow"
            | "canvas-structure"
            | "canvas-structure-narrow"
            | "canvas-structure-mixed"
            | "canvas-structure-mixed-narrow"
            | "canvas-replacement"
            | "canvas-replacement-narrow"
            | "canvas-structure-text"
            | "canvas-structure-text-narrow"
            | "canvas-local-review"
            | "canvas-local-review-narrow"
            | "canvas-checklist-saving"
            | "canvas-checklist-uncertain"
            | "canvas-loading"
            | "canvas-error"
            | "canvas-rename"
            | "canvas-rename-error"
            | "canvas-readonly" => {
                state.canvas = Some(crate::model::CanvasTarget {
                    id: "Ffixture".into(),
                    title: "Project handbook".into(),
                    permalink: None,
                });
                state.overlay = Some(Overlay::Canvas);
                crate::canvas::comments::install_fixture(&mut state);
            }
            "unreads" => state.main_view = MainView::Unreads,
            "dms" => state.main_view = MainView::Dms,
            "files"
            | "files-narrow"
            | "files-density"
            | "files-density-narrow"
            | "files-empty"
            | "files-error"
            | "channel-files"
            | "channel-files-narrow" => {
                state.main_view = MainView::Files;
                let team = state.core.active_team.clone().unwrap();
                state
                    .core
                    .files
                    .ensure_scope(state.core.account_epoch, &team);
                let generation = state.core.files.begin(Default::default(), 1);
                let items = if name == "files-empty" {
                    vec![]
                } else {
                    vec![
                        serde_json::json!({"id":"F1","title":"Super Platinum interface review.pdf","filetype":"pdf","pretty_type":"PDF","size":184200,"user":"U1","created":1791763200}),
                        serde_json::json!({"id":"F2","title":"Product decisions and launch notes","filetype":"quip","pretty_type":"Canvas","user":"U2","created":1791676800}),
                        serde_json::json!({"id":"F3","title":"Screenshot — compact thread layout.png","filetype":"png","pretty_type":"PNG","size":294100,"user":"U1","created":1791590400}),
                        serde_json::json!({"id":"F4","title":"A long file name that still stays on one line and leaves room for useful metadata when the window is narrow.csv","filetype":"csv","pretty_type":"CSV","size":12200,"user":"U2","created":1791504000}),
                    ]
                };
                let page=serde_json::from_value(serde_json::json!({"items":items,"pagination":{"page":1,"page_count":if items.is_empty(){1}else{3},"total_count":if items.is_empty(){0}else{124}}})).unwrap();
                state.core.files.apply(generation, page);
                if matches!(name, "files-density" | "files-density-narrow") {
                    crate::view::files::check::fixture(&mut state);
                }
                if matches!(name, "channel-files" | "channel-files-narrow") {
                    state.main_view = MainView::Home;
                    state.channel_tab = ChannelTab::Files;
                    state.core.channel_files = std::mem::take(&mut state.core.files);
                    state.core.channel_files.query.channel = state.core.active_channel.clone();
                    state.core.channel_files.query.sort =
                        super_platinum_core::files::FileSort::Newest;
                }
                if name == "files-error" {
                    state.core.files.error = Some(
                        "Files could not refresh. Your previous results are still available."
                            .into(),
                    );
                }
            }
            "pins"
            | "pins-narrow"
            | "pins-empty"
            | "pins-error"
            | "channel-pinned-message"
            | "pins-files"
            | "pins-files-narrow" => {
                use super_platinum_core::pins::{PinnedItem, PinsPage};
                let file_fixture = matches!(name, "pins-files" | "pins-files-narrow");
                let team = state.core.active_team.clone().unwrap();
                let channel = state.core.active_channel.clone().unwrap();
                state.channel_tab = if name == "channel-pinned-message" {
                    ChannelTab::Messages
                } else {
                    ChannelTab::Pins
                };
                state
                    .core
                    .pins
                    .ensure_scope(state.core.account_epoch, &team);
                let generation = state.core.pins.begin(&channel);
                let items = if matches!(name, "pins-empty" | "pins-error") {
                    vec![]
                } else {
                    state.core.workspaces[&team].messages[&channel]
                        .messages
                        .iter()
                        .take(if file_fixture { 1 } else { 3 })
                        .enumerate()
                        .map(|(i, message)| PinnedItem {
                            kind: "message".into(),
                            channel: Some(channel.clone()),
                            message: Some(message.clone()),
                            created: Some(1791763200 - i as i64 * 86400),
                            created_by: Some("U1".into()),
                            ..Default::default()
                        })
                        .collect()
                };
                state.core.pins.apply(generation, PinsPage { items });
                state.core.reconcile_pin_copies(&team, &channel);
                if file_fixture {
                    pinned_files::apply(&mut state.core, &team, &channel);
                }
                if name == "pins-error" {
                    state.core.pins.loaded = false;
                    state.core.pins.error = Some("Pins could not load. Please try again.".into());
                }
                state.refresh_from_core();
            }
            "channel-links"
            | "channel-links-narrow"
            | "channel-links-empty"
            | "channel-links-error" => {
                state.channel_tab = ChannelTab::Links;
                let team = state.core.active_team.clone().unwrap();
                let channel = state.core.active_channel.clone().unwrap();
                state
                    .core
                    .channel_links
                    .ensure_scope(state.core.account_epoch, &team, &channel);
                let generation = state.core.channel_links.begin(String::new(), 1);
                let items = if name == "channel-links-empty" {
                    vec![]
                } else {
                    vec![
                        serde_json::json!({"url":"https://vercel.com/geist/introduction","title":"Geist — a precise design system","timestamp":"1791763200.000001"}),
                        serde_json::json!({"url":"https://example.com/notes/a-long-url-that-should-truncate-without-pushing-message-actions-outside-the-window","title":"A long shared link title that remains compact and keeps its original message within reach","timestamp":"1791676800.000001"}),
                        serde_json::json!({"url":"https://example.com/launch","title":"Launch notes 🚀","timestamp":"1791590400.000001"}),
                    ]
                };
                state.core.channel_links.apply(generation,serde_json::from_value(serde_json::json!({"items":items,"pagination":{"page":1,"page_count":if items.is_empty(){1}else{6},"total_count":if items.is_empty(){0}else{104}}})).unwrap());
                if name == "channel-links-error" {
                    state.core.channel_links.error = Some(
                        "Links could not refresh. Your previous results are still available."
                            .into(),
                    );
                }
            }
            "later-files" | "later-files-narrow" => crate::later::file_fixture::apply(&mut state),
            "later" | "later-empty" | "later-error" | "later-reminder" | "later-new-reminder" => {
                use super_platinum_core::later::{SavedItem, SavedPage};
                state.main_view = MainView::Later;
                let team = state.core.active_team.clone().unwrap();
                let channel = state.core.active_channel.clone().unwrap();
                state
                    .core
                    .later
                    .ensure_scope(state.core.account_epoch, &team);
                let mut page = SavedPage::default();
                if name != "later-empty" {
                    let messages = state.core.workspaces[&team].messages[&channel]
                        .messages
                        .iter()
                        .rev()
                        .take(3)
                        .cloned()
                        .collect::<Vec<_>>();
                    for (index, message) in messages.into_iter().enumerate() {
                        let item = SavedItem {
                            item_type: "message".into(),
                            item_id: channel.clone(),
                            ts: message.ts.clone().unwrap(),
                            date_created: super_platinum_core::state::now_secs(),
                            date_due: if index == 0 {
                                super_platinum_core::state::now_secs() + 3600
                            } else {
                                0
                            },
                            state: "in_progress".into(),
                            todo_state: "saved".into(),
                            ..Default::default()
                        };
                        state.core.later.hydrated.insert(item.key(), message);
                        page.saved_items.push(item);
                    }
                }
                if name == "later-new-reminder" {
                    page.saved_items.push(SavedItem {
                        item_type: "reminder".into(), item_id: "Sa_fixture".into(),
                        date_due: super_platinum_core::state::now_secs()+3600,
                        description: vec![serde_json::json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Check the release notes\nVerify keyboard navigation.","style":{"bold":true}}]}]})],
                        ..Default::default()
                    });
                }
                page.counts.uncompleted_count = page.saved_items.len();
                let generation = state.core.later.begin(Default::default(), false);
                state.core.later.apply_page(generation, page, false, None);
                if name == "later-error" {
                    state.core.later.error = Some(
                        "Later could not refresh. Your saved items are still available.".into(),
                    );
                }
            }
            "dm-header-compact" => {
                state.main_view = MainView::Dms;
                if let Some(index) = state.channels.iter().position(|channel| channel.id == "D1") {
                    state.select_channel(index, ChannelOpen::InSurface);
                }
            }
            "activity-paging"
            | "activity-navigation"
            | "activity-navigation-narrow"
            | "activity-loading"
            | "activity-error"
            | "activity-more-error"
            | "activity-empty"
            | "activity-narrow" => {
                crate::activity_feed::fixture(&mut state, name);
            }
            "activity-unread" => {
                state.main_view = MainView::Activity;
                state.core.activity.unread_only = true;
            }
            // A channel item opened from Activity: the conversation gets the
            // whole right side, composer and all, the way Slack's does.
            "activity-channel-post" => {
                state.main_view = MainView::Activity;
                state.select_activity_item(
                    "mention-1".into(),
                    Some("C2"),
                    Some("1719800400.000200"),
                    None,
                );
            }
            // A thread item opened from Activity. The reply is the signed-in
            // user's, so the row carries the same Edit/Delete toolbar the
            // channel timeline gives it.
            "activity-thread-tools" => {
                state.main_view = MainView::Activity;
                state.select_activity_item(
                    "thread-1".into(),
                    Some("C2"),
                    Some("1719801000.000400"),
                    Some("1719800000.000100"),
                );
                state.thread_root = Some("m2".into());
                state.thread_messages = vec![
                    MessageVm {
                        id: "m2".into(),
                        ts: "3.0".into(),
                        author: "Jules".into(),
                        timestamp: "9:47 AM".into(),
                        avatar_initials: "JU".into(),
                        body: vec![RichNode::Text(
                            "I measured the warm path: cache, render, then refresh.".into(),
                        )],
                        ..Default::default()
                    },
                    MessageVm {
                        id: "m2-r1".into(),
                        ts: "3.1".into(),
                        author: "You".into(),
                        timestamp: "9:52 AM".into(),
                        avatar_initials: "YO".into(),
                        is_own: true,
                        body: vec![RichNode::Text(
                            "Confirmed offline too; cached content remains visible.".into(),
                        )],
                        ..Default::default()
                    },
                ];
            }
            "accounts" => state.overlay = Some(Overlay::Accounts),
            "self-menu" => {
                state.overlay = Some(Overlay::SelfMenu);
                state.self_account.presence = PresenceVm::Away;
                if let Some(workspace) = state.core.workspaces.get_mut("T1") {
                    workspace
                        .presence
                        .insert("U0".into(), super_platinum_core::state::Presence::Away);
                    if let Some(profile) = workspace
                        .users
                        .get_mut("U0")
                        .and_then(|user| user.profile.as_mut())
                    {
                        profile.status_text = Some("You Could Be - MII...".into());
                        profile.status_emoji = Some(":musical_note:".into());
                    }
                }
            }
            "self-menu-notifications" => {
                state.overlay = Some(Overlay::SelfMenu);
                state.self_menu_notifications_open = true;
            }
            "settings-storage" => {
                state.overlay = Some(Overlay::Settings);
                state.settings_section = SettingsSection::Storage;
                state.storage = StoragePanel {
                    usage: StorageUsageVm {
                        avatars: 12_484_608,
                        emoji: 3_250_176,
                        icons: 1_258_496,
                        other: 0,
                        workspace: 19_505_152,
                    },
                    selected: [true; 4],
                    scanning: false,
                    fixture: true,
                };
            }
            "palette-people" => {
                let workspace = state.core.workspaces.get_mut("T1").unwrap();
                workspace.users.insert(
                    "U_PALETTE_NEW".into(),
                    super_platinum_core::slack::models::User {
                        id: "U_PALETTE_NEW".into(),
                        name: Some("avery.park".into()),
                        profile: Some(super_platinum_core::slack::models::UserProfile {
                            display_name: Some("Avery Park".into()),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                );
                state.open_palette();
                state.set_palette_query("avery".into());
            }
            "palette" => state.overlay = Some(Overlay::Palette),
            "reaction-picker" => {
                let channel = state.core.active_channel.clone().unwrap_or_default();
                let team = state.core.active_team.clone().unwrap_or_default();
                let ts = state
                    .messages
                    .last()
                    .map(|message| message.id.clone())
                    .unwrap_or_default();
                state.reaction_target = Some(ReactionTarget { team, channel, ts });
                state.overlay = Some(Overlay::ReactionPicker);
            }
            "forward-recovery"
            | "forward-recovery-narrow"
            | "forward"
            | "forward-narrow"
            | "forward-links"
            | "forward-links-narrow"
            | "forward-error"
            | "forward-partial"
            | "forward-partial-narrow"
            | "forward-people"
            | "forward-people-narrow" => {
                let team = state.core.active_team.clone().unwrap();
                let channel = if name.ends_with("narrow") && !name.starts_with("forward-recovery") {
                    "C5".into()
                } else {
                    state.core.active_channel.clone().unwrap()
                };
                let mut message = state.messages.first().cloned().unwrap();
                if name.starts_with("forward-recovery") {
                    message.ts = "1719800000.000100".into();
                    state.core.active_account = Some("00000000-0000-4000-8000-000000000071".into());
                    state.main_view = MainView::Scheduled;
                    state.core.scheduled.filter =
                        super_platinum_core::scheduled::DraftFilter::Drafts;
                }
                state.forward_target = Some(crate::forward::ForwardTarget {
                    recovery_only: name.starts_with("forward-recovery"),
                    epoch: state.core.account_epoch,
                    team,
                    channel,
                    message,
                    id: "fixture-forward".into(),
                });
                state.overlay = Some(Overlay::Forward);
            }
            "new-dm-directory"
            | "new-dm-directory-loading"
            | "new-dm-directory-error"
            | "new-dm-directory-empty"
            | "new-dm-directory-error-narrow" => {
                state.overlay = Some(Overlay::NewDm);
                if let Some(ws) = state
                    .core
                    .active_team
                    .clone()
                    .and_then(|t| state.core.workspaces.get_mut(&t))
                {
                    ws.users.insert(
                        "ULOCAL".into(),
                        super_platinum_core::slack::models::User {
                            id: "ULOCAL".into(),
                            name: Some("alex_local".into()),
                            real_name: Some("Alex Local".into()),
                            ..Default::default()
                        },
                    );
                }
            }
            "new-dm" | "new-dm-selected" | "new-dm-pending" | "new-dm-error" | "new-dm-narrow" => {
                state.overlay = Some(Overlay::NewDm);
            }
            "animated-reaction" => {
                keep_recent_messages(&mut state, 8);
                if let Some(message) = state.messages.last_mut() {
                    message.reactions = vec![
                        reaction("rocket", 12, true),
                        reaction("eyes", 4, false),
                        // A custom emoji whose art never resolved: the card must
                        // not set its `:shortcode:` at emoji size.
                        reaction("blunder", 1, false),
                    ];
                }
                state.sync_fixture_messages();
            }
            "main-general-new-message-motion" => {
                keep_recent_messages(&mut state, 8);
                if let Some(message) = state.messages.last() {
                    state
                        .message_arrivals
                        .insert(message.id.clone(), std::time::Instant::now());
                }
                state.sync_fixture_messages();
            }
            "profile-hover-card" => {
                state.profile_hover = Some(ProfileHoverVm {
                    user_id: "U1".into(),
                    x: 610.0,
                    y: 120.0,
                });
            }
            "profile-pane"
            | "profile-dm-opening"
            | "profile-vip-pending"
            | "profile-pane-rich-fields"
            | "profile-pane-min-width"
            | "channel-thread-profile-headers" => {
                state.profile_user = Some("U1".into());
                state.core.profile_pane = Some(super_platinum_core::domain::ProfilePaneState {
                    user: "U1".into(),
                    loading: false,
                    error: None,
                });
                if name == "profile-vip-pending" {
                    crate::profile_vip::fixture_pending(&mut state, "U1");
                }
                if name == "profile-dm-opening" {
                    state.profile_dm_opening = Some("U1".into());
                }
                if name == "profile-pane-rich-fields" {
                    state.channels.retain(|channel| {
                        !(channel.is_im && channel.user_id.as_deref() == Some("U1"))
                    });
                    if let Some(workspace) = state.core.workspaces.get_mut("T1") {
                        if let Some(user) = workspace.users.get_mut("U1") {
                            user.im_mpim_ids.clear();
                        }
                        workspace.apply_emojis(vec![
                            serde_json::from_value(serde_json::json!({
                                "name": "sob-pray",
                                "value": "https://example.test/sob-pray.png"
                            }))
                            .expect("fixture emoji"),
                        ]);
                    }
                    let emoji = state
                        .media
                        .register_emoji("sob-pray", "https://example.test/sob-pray.png");
                    state.media.insert(
                        emoji,
                        "image/png",
                        include_bytes!("../../../assets/icons/icon-512.png").as_slice(),
                    );
                }
                if name == "profile-pane-min-width" {
                    state.profile_pane_width = 200.0;
                }
            }
            "channel-huddle" => {
                if let Some(workspace) = state.core.workspaces.get_mut("T1") {
                    workspace.apply_room(serde_json::from_value(serde_json::json!({"id":"R1","channels":["C2"],"huddle_link":"https://app.slack.com/huddle/T1/C2","participants":["U1","U2"]})).expect("fixture room"));
                }
            }
            // In the call: the dock with one person talking and one muted,
            // the reader's own mic off, and the header showing the live call.
            "huddle-connected"
            | "huddle-video"
            | "huddle-video-expanded"
            | "huddle-settings"
            | "huddle-people"
            | "huddle-captions" => {
                use super_platinum_core::huddle::MediaEvent;
                if let Some(workspace) = state.core.workspaces.get_mut("T1") {
                    workspace.apply_room(serde_json::from_value(serde_json::json!({"id":"R1","channels":["C2"],"participants":["U0","U1","U2"]})).expect("fixture room"));
                }
                let huddle = &mut state.core.huddle;
                if let Some((generation, _)) = huddle.begin_join("T1".into(), "C2".into()) {
                    huddle.joined(generation, "R1".into(), "a-0".into());
                    huddle.apply_media(generation, MediaEvent::Started);
                    huddle.apply_media(generation, MediaEvent::Muted(true));
                    state.huddle_ui.begin(generation);
                    state.huddle_ui.visible = name != "huddle-connected";
                    state.huddle_ui.expanded = name == "huddle-video-expanded";
                    state.huddle_ui.settings = name == "huddle-settings";
                    state.huddle_ui.invite = name == "huddle-people";
                    state.huddle_ui.rect = Some(super::HuddleRect {
                        left: 500.0,
                        top: 250.0,
                        width: 640.0,
                        height: 420.0,
                    });
                    if name != "huddle-connected" {
                        use super_platinum_core::huddle::HuddleVideoTile;
                        huddle.apply_media(
                            generation,
                            MediaEvent::Camera {
                                enabled: true,
                                pending: false,
                            },
                        );
                        huddle.apply_media(
                            generation,
                            MediaEvent::Tiles(vec![
                                HuddleVideoTile {
                                    id: 1,
                                    user: Some("U0".into()),
                                    local: true,
                                    content: false,
                                },
                                HuddleVideoTile {
                                    id: 2,
                                    user: Some("U1".into()),
                                    local: false,
                                    content: true,
                                },
                            ]),
                        );
                        state.huddle_ui.devices = super::HuddleDevices {
                            microphone: vec![super::HuddleDevice {
                                id: "mic-1".into(),
                                label: "MacBook microphone".into(),
                            }],
                            camera: vec![super::HuddleDevice {
                                id: "cam-1".into(),
                                label: "FaceTime HD camera".into(),
                            }],
                            speaker: vec![super::HuddleDevice {
                                id: "out-1".into(),
                                label: "Built-in speakers".into(),
                            }],
                            output_supported: false,
                            screen_supported: true,
                        };
                    }
                    if name == "huddle-captions" {
                        state.huddle_ui.captions_on = true;
                        state.huddle_ui.caption(super::HuddleCaption {
                            id: "caption-1".into(),
                            user: Some("U1".into()),
                            text: "The release checklist is ready. Let's review the desktop build."
                                .into(),
                            partial: false,
                        });
                        state.huddle_ui.reactions.push(super::HuddleReaction {
                            shown_at: std::time::Instant::now(),
                            user: Some("U2".into()),
                            emoji: "thumbsup".into(),
                        });
                    }
                    huddle.apply_media(
                        generation,
                        MediaEvent::Roster(vec![
                            ("T1-R1-U0".into(), false, true),
                            ("T1-R1-U1".into(), true, false),
                            ("T1-R1-U2".into(), false, true),
                        ]),
                    );
                }
            }
            // Maya is ringing from her DM while a huddle runs in #general.
            "huddle-invite" => {
                if let Some(workspace) = state.core.workspaces.get_mut("T1") {
                    workspace.apply_room(serde_json::from_value(serde_json::json!({"id":"R1","channels":["C2"],"participants":["U1","U2"]})).expect("fixture room"));
                }
                state.core.huddle.add_invite(
                    "T1".into(),
                    super_platinum_core::slack::models::HuddleInvite {
                        channel_id: "D1".into(),
                        call_id: "R2".into(),
                        sender_user_id: Some("U1".into()),
                    },
                    std::time::Instant::now(),
                );
            }
            "chat-paused-pill" => state.chat_paused = true,
            "appearance-countertop" => {
                state.core.settings.preset = super_platinum_core::config::ThemePreset::Countertop
            }
            "appearance-blue-steel" => {
                state.core.settings.preset = super_platinum_core::config::ThemePreset::BlueSteel
            }
            "appearance-paper-bag" => {
                state.core.settings.preset = super_platinum_core::config::ThemePreset::PaperBag
            }
            "appearance-custom-background" => {
                let id = super_platinum_core::MediaAssetId::new(MediaAssetKind::Background);
                state.media.insert(
                    id.clone(),
                    "image/png",
                    include_bytes!("../../../assets/icons/icon-512.png").as_slice(),
                );
                state.background_uri = Some(id.webview_uri());
                state.core.settings.background =
                    Some(super_platinum_core::config::BackgroundSettings {
                        file_name: "fixture.png".into(),
                        fit: super_platinum_core::config::BackgroundFit::Contain,
                        dim: 0.25,
                        surface_opacity: 0.78,
                    });
            }
            "image-viewer-upload" | "image-viewer-embed-compact" => {
                state.open_fixture_viewer("image/png")
            }
            "pdf-preview"
            | "pdf-preview-narrow"
            | "pdf-preview-error"
            | "pdf-preview-search-long" => crate::pdf_preview::fixture::apply(&mut state, &name),
            "video-viewer" => state.open_fixture_viewer("video/mp4"),
            "file-share-draft-reopen" => crate::file_share::check::restart::install(&mut state),
            "file-share"
            | "file-share-narrow"
            | "file-share-pressure"
            | "file-share-pressure-narrow"
            | "file-share-dm"
            | "file-share-dm-narrow"
            | "file-share-group-dm"
            | "file-share-group-dm-narrow" => crate::file_share::check::seed::install(&mut state),
            "message-file-actions" | "message-file-actions-narrow" => {
                crate::file_actions::attachment::check::fixture(&mut state)
            }
            "viewer-actions-files"
            | "viewer-actions-message"
            | "viewer-actions-narrow"
            | "viewer-actions-pdf" => crate::viewer_actions_check::fixture(&mut state, &name),
            "file-actions"
            | "file-actions-narrow"
            | "file-actions-channel-narrow"
            | "file-rename"
            | "file-rename-narrow"
            | "file-recovery"
            | "file-recovery-narrow"
            | "file-rename-channel-narrow" => crate::file_actions_check::fixture(&mut state),
            "original-download"
            | "original-download-narrow"
            | "original-download-save-dialog"
            | "original-download-save-workflow"
            | "cached-viewer-save-check" => {}
            #[cfg(all(
                feature = "original-download-native-fixture",
                feature = "protected-native-fixture"
            ))]
            "file-preview-refresh-check" | "file-preview-refresh-check-narrow" => {
                crate::file_preview::refresh::check::install(&mut state)
                    .expect("Owned refresh fixture configuration required");
            }
            "file-preview-refresh"
            | "file-preview-refresh-narrow"
            | "file-preview-refresh-error"
            | "file-preview-refresh-error-narrow" => {
                crate::file_preview::refresh::fixture::install(&mut state, &name)
            }
            "file-preview-text"
            | "file-preview-narrow"
            | "file-preview-large"
            | "file-preview-utf16"
            | "file-preview-empty"
            | "file-preview-binary"
            | "file-preview-renamed-files"
            | "file-preview-renamed-message"
            | "file-preview-audio" => crate::file_preview::fixture(&mut state, &name),
            "gif-picker-attachment" => {
                let id = super_platinum_core::MediaAssetId::new(MediaAssetKind::Attachment);
                state.media.insert(
                    id.clone(),
                    "image/png",
                    include_bytes!("../../../assets/icons/icon-256.png").as_slice(),
                );
                if let Some(message) = state.messages.last_mut() {
                    message.body.push(RichNode::Paragraph(vec![RichNode::Media {
                        file: None,
                        id: Some(id),
                        full: None,
                        name: "celebration.gif".into(),
                        download_name: "celebration.png".into(),
                        mime: "image/png".into(),
                        size: None,
                    }]));
                }
            }
            "multi-paragraph-custom-emoji" => {
                keep_recent_messages(&mut state, 8);
                if let Some(message) = state.messages.last_mut() {
                    message.body = vec![
                        RichNode::Paragraph(vec![RichNode::Text(
                            "Hack Piano shipped a clean first paragraph.".into(),
                        )]),
                        RichNode::Paragraph(vec![
                            RichNode::Text("The second paragraph wraps independently with ".into()),
                            RichNode::Emoji {
                                name: "ship".into(),
                                glyph: "🚢".into(),
                            },
                        ]),
                        RichNode::Paragraph(vec![RichNode::Text(
                            "No words float into the previous line.".into(),
                        )]),
                    ];
                }
                state.sync_fixture_messages();
            }
            "slack-connect-avatar" => {
                keep_recent_messages(&mut state, 3);
                let placeholder = include_bytes!("../../../assets/icons/icon-512.png");
                for url in [
                    "https://ca.slack-edge.com/E_VERCEL-U5-12fe3fbf9a8c-48",
                    "https://example.test/vercel-team.png",
                ] {
                    let id = state
                        .media
                        .register(MediaAssetKind::Avatar, url, "image/png", false);
                    state.media.insert(id, "image/png", placeholder.as_slice());
                }
                state.append_projected_fixture_messages(
                    &[serde_json::json!({
                        "type": "message",
                        "ts": "1719801200.000500",
                        "user": "U5",
                        "channel": "C2",
                        "text": "Slack Connect avatars retain their external workspace badge."
                    })],
                    true,
                );
            }
            "composer-multiline" => {
                state.core.composer = super_platinum_core::ComposerState::with_text(
                    "First line\nSecond line with more detail\nThird line ready to send",
                )
            }
            "composer-upload-progress" => {
                use std::sync::Arc;
                use std::sync::atomic::{AtomicBool, AtomicU64};
                use std::time::Instant;
                let progress = Arc::new(AtomicU64::new(420_000));
                let cancel = Arc::new(AtomicBool::new(false));
                let attachment = super_platinum_core::domain::ComposerAttachment {
                    id: 42,
                    path: std::path::PathBuf::from("/tmp/ship-demo.png"),
                    name: "ship-demo.png".into(),
                    bytes: 1_048_576,
                    uploading: true,
                    upload_started: Some(Instant::now()),
                    upload_cancel: Some(cancel),
                    upload_progress: Some(progress),
                    preview_path: None,
                };
                // Composer chip still shows a staged non-uploading file.
                state.core.composer_attachments.push(
                    super_platinum_core::domain::ComposerAttachment {
                        id: 41,
                        path: std::path::PathBuf::from("/tmp/notes.txt"),
                        name: "notes.txt".into(),
                        bytes: 4_096,
                        uploading: false,
                        upload_started: None,
                        upload_cancel: None,
                        upload_progress: None,
                        preview_path: None,
                    },
                );
                let message_ts = "1719800900.000350".to_owned();
                if let Some(workspace) = state.core.workspaces.get_mut("T1") {
                    let message = super_platinum_core::slack::models::Message {
                        user: Some(workspace.self_user_id.clone()),
                        kind: Some("message".into()),
                        ts: Some(message_ts.clone()),
                        client_msg_id: Some("fixture-upload-1".into()),
                        text: Some("Shipping the capture with a file…".into()),
                        channel: Some("C2".into()),
                        ..Default::default()
                    };
                    let bag = workspace.messages.entry("C2".into()).or_default();
                    bag.upsert(message);
                    bag.pending.push(message_ts.clone());
                }
                state.core.pending_file_messages.push(
                    super_platinum_core::domain::PendingFileMessage {
                        finalized: None,
                        reply_broadcast: false,
                        team: "T1".into(),
                        channel: "C2".into(),
                        thread_ts: None,
                        message_ts,
                        client_msg_id: "fixture-upload-1".into(),
                        text: "Shipping the capture with a file…".into(),
                        attachments: vec![attachment],
                    },
                );
                state.refresh_from_core();
            }
            "edit-message-composer" => {
                let target = state
                    .messages
                    .iter()
                    .rev()
                    .find(|message| message.id.starts_with("m-rich") || message.id.starts_with('m'))
                    .map(|message| message.id.clone())
                    .unwrap_or_else(|| "m-rich-1".into());
                state.start_edit("C2".into(), target, "Editing this message in place".into())
            }
            // The rail's connection indicator, in both of the states it has.
            "connection-connecting" => {
                state.connection.status = ConnectionStatus::Connecting;
            }
            "connection-no-network" => {
                state.connection.status = ConnectionStatus::NoNetwork;
                state.connection.routable = false;
            }
            // A raw transport failure, the shape that used to paint itself off
            // the left edge of the window.
            "toast-long-error" => {
                state.show_toast(
                    "Direct messages failed: error sending request for uri \
                     (https://hackclub.enterprise.slack.com/api/client.dms?_x_app_name=client\
                     &_x_csid=038245db49464d279e52d53891556fe6&_x_desktop_ia=true\
                     &_x_foreground=true&slack_route=default): client error (Connect)",
                );
            }
            "dm-history-failed" => {
                state.main_view = MainView::Dms;
                state.show_toast("History refresh failed; cached messages remain visible");
            }
            "dm-cached-refresh-failed" => {
                state.main_view = MainView::Dms;
                state.show_toast("Workspace refresh failed; showing cached direct messages");
            }
            "main-dev" => {
                if let Some(index) = state
                    .channels
                    .iter()
                    .position(|channel| channel.id == "C3" || channel.name == "design")
                {
                    state.select_channel(index, ChannelOpen::Global);
                }
            }
            "message-unfurl-embed" => {
                keep_recent_messages(&mut state, 3);
                if let Some(channel) = state
                    .core
                    .workspaces
                    .get_mut("T1")
                    .and_then(|workspace| workspace.channels.get_mut("C1"))
                {
                    channel.name = Some("kc-crashout-corner-that-wraps-naturally".into());
                }
                state.append_projected_fixture_messages(&[out_of_context_message()], true);
            }
            "block-kit-layout" => {
                keep_recent_messages(&mut state, 3);
                state.append_projected_fixture_messages(&[weather_block_kit_message()], true);
            }
            "timeline-variable-height" | "timeline-latest-opening" => {
                let template = state.messages.last().cloned().unwrap();
                state.messages=(0..360).map(|index| {
                    let mut message=template.clone();
                    message.id=format!("{}.{:06}",1790000000+index,1);
                    message.ts=message.id.clone();
                    message.date_label=(index%20==0).then(||format!("Fixture day {}",index/20));
                    message.body=(0..1+index%6).map(|line|RichNode::Paragraph(vec![RichNode::Text(format!("Reading anchor {index}, paragraph {}. Uneven message heights must not move the reader.",line+1))])).collect();
                    message
                }).collect();
                state.sync_fixture_messages();
                if name == "timeline-latest-opening" {
                    state.core.pending_scroll_to = Some((
                        "C2".into(),
                        super_platinum_core::domain::PendingScrollTarget::Latest,
                    ));
                    state.set_timeline_window(0, 0, [], false);
                }
            }
            "media-failed" => {
                keep_recent_messages(&mut state, 2);
                let broken = super_platinum_core::MediaAssetId::new(MediaAssetKind::Attachment);
                state.media.insert(
                    broken.clone(),
                    "image/png",
                    b"not a decodable image".as_slice(),
                );
                let valid = super_platinum_core::MediaAssetId::new(MediaAssetKind::Attachment);
                state.media.insert(
                    valid.clone(),
                    "image/png",
                    include_bytes!("../../../assets/icons/icon-256.png").as_slice(),
                );
                let emoji = super_platinum_core::MediaAssetId::new(MediaAssetKind::Emoji);
                state
                    .media
                    .insert(emoji.clone(), "image/png", b"not an emoji".as_slice());
                let avatar = super_platinum_core::MediaAssetId::new(MediaAssetKind::Avatar);
                state
                    .media
                    .insert(avatar.clone(), "image/png", b"not an avatar".as_slice());
                if let Some(message) = state.messages.last_mut() {
                    message.avatar = Some(avatar);
                    message.body = vec![
                        RichNode::Paragraph(vec![RichNode::Text(
                            "Unavailable media keeps its place. Valid images still render.".into(),
                        )]),
                        RichNode::ImageBlock {
                            id: broken.clone(),
                            alt: "Corrupt attachment".into(),
                            title: None,
                            size: Some((280, 120)),
                        },
                        RichNode::Paragraph(vec![
                            RichNode::Text("Custom emoji: ".into()),
                            RichNode::EmojiImage {
                                id: emoji,
                                name: "unavailable-fixture".into(),
                            },
                        ]),
                        RichNode::Media {
                            file: None,
                            id: Some(valid),
                            full: None,
                            name: "Valid control image".into(),
                            download_name: "control.png".into(),
                            mime: "image/png".into(),
                            size: Some((96, 96)),
                        },
                    ];
                }
                state.sync_fixture_messages();
            }
            "media-loading-state" => {
                keep_recent_messages(&mut state, 3);
                state.append_projected_fixture_messages(&[out_of_context_message()], false);
                // A cold boot: every source is registered, no bytes have landed
                // yet. The avatar column must fall back to initials and inline
                // images to skeleton boxes — never a broken-image icon.
                let media = state.media.clone();
                for (index, message) in state.messages.iter_mut().enumerate() {
                    message.avatar = Some(media.register_avatar(
                        &format!("cold-{index}"),
                        &format!("https://example.test/cold-{index}.png"),
                    ));
                }
                state.sync_fixture_messages();
            }
            _ => {}
        }
        if state.fixture_channel_messages.is_some() {
            state.sync_fixture_messages();
        }
        state
    }

    /// Project raw Slack JSON through the live `message_vm` path so a fixture
    /// exercises real Block Kit and attachment translation, not hand-built nodes.
    ///
    /// `seed_media` controls whether the assets the projection registers already
    /// have bytes: `false` captures the cold-boot state, where every image is
    /// still downloading.
    fn append_projected_fixture_messages(&mut self, raw: &[serde_json::Value], seed_media: bool) {
        // Pre-seed the bytes for every asset the projection will register, keyed
        // by the same (kind, url) pair `blocks`/`unfurl` use.
        if seed_media {
            let placeholder = include_bytes!("../../../assets/icons/icon-512.png");
            for (kind, url) in [
                (MediaAssetKind::Attachment, "https://example.test/pfp.png"),
                (
                    MediaAssetKind::Attachment,
                    "https://example.test/weather.png",
                ),
                (MediaAssetKind::Avatar, "https://example.test/maya.png"),
                (MediaAssetKind::Emoji, "https://example.test/sob-pray.png"),
            ] {
                let id = self.media.register(kind, url, "image/png", false);
                self.media.insert(id, "image/png", placeholder.as_slice());
            }
        }
        let Some(workspace) = self.core.workspaces.get_mut("T1") else {
            return;
        };
        workspace.apply_emojis(vec![
            serde_json::from_value(serde_json::json!({
                "name": "sob-pray",
                "value": "https://example.test/sob-pray.png"
            }))
            .expect("fixture emoji"),
        ]);
        let workspace = &self.core.workspaces["T1"];
        if let Some(all) = self.fixture_channel_messages.take() {
            self.messages = all;
        }
        for value in raw {
            let message: super_platinum_core::slack::models::Message =
                serde_json::from_value(value.clone()).expect("fixture slack message");
            self.messages.push(crate::message_vm::message_vm(
                workspace,
                &message,
                &self.media,
            ));
        }
        self.sync_fixture_messages();
    }

    fn open_fixture_viewer(&mut self, mime: &str) {
        let id = super_platinum_core::MediaAssetId::new(MediaAssetKind::Attachment);
        let bytes: &[u8] = if mime.starts_with("image/") {
            include_bytes!("../../../assets/icons/icon-512.png")
        } else {
            &[]
        };
        self.media.insert(id.clone(), mime, bytes);
        self.viewer = Some(ViewerVm {
            file: None,
            id,
            name: if mime.starts_with("video/") {
                "demo.mp4"
            } else {
                "super-platinum.png"
            }
            .into(),
            download_name: if mime.starts_with("video/") {
                "demo.mp4"
            } else {
                "super-platinum.png"
            }
            .into(),
            mime: mime.into(),
        });
        self.overlay = Some(Overlay::Viewer);
    }
}

/// The #out-of-context shape, trimmed from real `conversations.history` JSON:
/// an APP post whose body is a `context` block and whose embed is a reply unfurl.
fn out_of_context_message() -> serde_json::Value {
    serde_json::json!({
        "type": "message",
        "ts": "1719801200.000400",
        "user": "U4",
        "bot_id": "B1",
        "app_id": "A1",
        "bot_profile": {
            "id": "B1",
            "name": "Out of Context",
            "user_id": "U4",
            // Deliberately unregistered: Slack shows the bot user's avatar, so a
            // regression back to this generic icon shows up as a broken image.
            "icons": {"image_48": "https://a.slack-edge.com/img/plugins/app/bot_48.png"}
        },
        "edited": {"ts": "1719801300.000000", "user": "U4"},
        "text": "user pfp <@U1>",
        "blocks": [{
            "type": "context",
            "block_id": "ctx-1",
            "elements": [
                {"type": "image", "image_url": "https://example.test/pfp.png", "alt_text": "user pfp"},
                {"type": "mrkdwn", "text": "<@U1>", "verbatim": false}
            ]
        }],
        "attachments": [{
            "id": 1,
            "author_icon": "https://example.test/maya.png",
            "author_id": "U1",
            "author_name": "Maya Chen",
            "author_subname": "Maya Chen",
            "blocks": [{
                "type": "rich_text",
                "elements": [{
                    "type": "rich_text_section",
                    "elements": [
                        {"type": "text", "text": "6.1 inches is crazy "},
                        {"type": "emoji", "name": "sob-pray"}
                    ]
                }]
            }],
            "channel_id": "C1",
            "channel_team": "T1",
            "color": "D0D0D0",
            "fallback": "[…] 6.1 inches is crazy",
            "footer": "Thread in Slack Conversation",
            "from_url": "https://example.test/archives/C1/p1719800900000100",
            "is_msg_unfurl": true,
            "is_reply_unfurl": true,
            "is_share": true,
            "mrkdwn_in": ["text"],
            "text": "6.1 inches is crazy :sob-pray:",
            "ts": "1719800900.000100"
        }],
        "reactions": [
            {"name": "sob-pray", "count": 1, "users": ["U1"]},
            {"name": "interrobang", "count": 6, "users": ["U0", "U1", "U2"]}
        ],
        "reply_count": 7,
        "reply_users": ["U1", "U2"],
        "latest_reply": "1719801000.000100"
    })
}

/// A Block Kit layout post (image, header, sections, divider, actions, context),
/// trimmed from a real weather-bot message.
fn weather_block_kit_message() -> serde_json::Value {
    serde_json::json!({
        "type": "message",
        "ts": "1719801600.000500",
        "user": "U4",
        "bot_id": "B1",
        "bot_profile": {"id": "B1", "name": "Weather", "user_id": "U4"},
        "text": "Shelburne - Vermont",
        "blocks": [
            {
                "type": "image",
                "block_id": "img-1",
                "image_url": "https://example.test/weather.png",
                "alt_text": "Patchy rain nearby",
                "image_width": 64,
                "image_height": 64,
                "title": {"type": "plain_text", "text": "Patchy rain nearby", "emoji": true}
            },
            {
                "type": "header",
                "block_id": "hdr-1",
                "text": {"type": "plain_text", "text": "Shelburne - Vermont", "emoji": true}
            },
            {
                "type": "section",
                "block_id": "sec-1",
                "text": {"type": "mrkdwn", "text": "*Temp:* 62.4 (16.9)", "verbatim": false}
            },
            {
                "type": "section",
                "block_id": "sec-2",
                "fields": [
                    {"type": "mrkdwn", "text": "*Wind*\n6.5 (10.4)"},
                    {"type": "mrkdwn", "text": "*Gust*\n7.6 (12.3)"}
                ]
            },
            {"type": "divider", "block_id": "div-1"},
            {
                "type": "actions",
                "block_id": "act-1",
                "elements": [
                    {
                        "type": "button",
                        "action_id": "open",
                        "text": {"type": "plain_text", "text": "Weather on accuweather", "emoji": true},
                        "url": "https://example.test/weather"
                    },
                    {
                        "type": "static_select",
                        "action_id": "pick",
                        "placeholder": {"type": "plain_text", "text": "Change location"}
                    }
                ]
            },
            {
                "type": "context",
                "block_id": "ctx-2",
                "elements": [{"type": "mrkdwn", "text": "Made by <@U1>"}]
            }
        ]
    })
}
