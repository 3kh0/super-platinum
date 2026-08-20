use std::collections::HashMap;
use std::sync::Arc;

use super_platinum_core::MediaAssetKind;

use super::{ShellState, account_vms, keep_recent_messages};
use crate::channel_vm::{project_channels, project_messages_for_channel};
use crate::fixture::fixture_core;
use crate::media::MediaRegistry;
use crate::model::*;

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
        match fixture.as_str() {
            "login" => Self::login(media),
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
            "search" => {
                let mut state = Self::fixture(media);
                state.overlay = Some(Overlay::Search);
                state.search_query = "launch".into();
                state.search_results = vec![SearchResultVm {
                    channel_id: "C2".into(),
                    channel_name: "ship".into(),
                    ts: "1719800400.000200".into(),
                    author: "Jules".into(),
                    text: "The launch checklist is ready for desktop review.".into(),
                }];
                state
            }
            "threads" | "thread-unread-divider" => {
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
                        reactions: vec![("white_check_mark".into(), 6, false)],
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
                    cache.load_workspace(workspace_session)?.unwrap_or_else(|| {
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
        let workspace = cached_workspaces
            .get(&workspace_session.team_id)
            .cloned()
            .unwrap_or_else(|| {
                super_platinum_core::state::Workspace::from_session(workspace_session)
            });
        let mut core =
            super_platinum_core::CoreAppState::new(super_platinum_core::config::load_settings());
        core.screen = super_platinum_core::state::Screen::Main;
        core.accounts = accounts.sessions.clone();
        core.active_account = Some(accounts.active_account.clone());
        core.session = Some(session.clone());
        core.transport = super_platinum_core::slack::Transport::new(session.d_cookie.clone())
            .ok()
            .map(Arc::new);
        core.cache = Some(cache);
        core.active_team = Some(workspace_session.team_id.clone());
        core.active_channel = workspace.last_active_channel.clone();
        core.workspaces = cached_workspaces;

        let preferred = core.active_channel.clone();
        let (channels, sidebar_sections, default_active, _) = project_channels(&workspace, &media);
        let active_channel = preferred
            .as_ref()
            .and_then(|id| channels.iter().position(|channel| &channel.id == id))
            .unwrap_or(default_active);
        let (messages_by_channel, messages) = channels
            .get(active_channel)
            .map(|channel| {
                let messages = project_messages_for_channel(&workspace, &channel.id, &media);
                (
                    HashMap::from([(channel.id.clone(), messages.clone())]),
                    messages,
                )
            })
            .unwrap_or_default();
        let timeline_end = messages.len();

        Ok(Self {
            core,
            media,
            media_epoch: 0,
            background_uri: None,
            signed_in: true,
            loading: channels.is_empty(),
            accounts: account_vms(&accounts),
            workspaces,
            active_workspace: 0,
            channels,
            sidebar_sections,
            active_channel,
            messages,
            messages_by_channel,
            timeline_start: timeline_end.saturating_sub(100),
            timeline_end,
            row_heights: HashMap::new(),
            message_arrivals: HashMap::new(),
            selection_pinned: false,
            upload_ui_epoch: 0,
            stick_to_bottom: true,
            loading_older: false,
            chat_paused: false,
            main_view: MainView::Home,
            dm_query: String::new(),
            dm_unread_only: false,
            activity_tab: ActivityTab::default(),
            activity_detail_open: false,
            profile_hover: None,
            overlay: None,
            palette_query: String::new(),
            palette_selected: 0,
            search_query: String::new(),
            search_results: Vec::new(),
            search_loading: false,
            thread_root: None,
            thread_messages: Vec::new(),
            profile_user: None,
            viewer: None,
            toast: None,
            performance: PerformanceVm::default(),
            channel_switch_started: None,
            realtime_insert_started: None,
        })
    }

    pub fn login(media: MediaRegistry) -> Self {
        let core =
            super_platinum_core::CoreAppState::new(super_platinum_core::config::load_settings());
        Self {
            core,
            media,
            media_epoch: 0,
            background_uri: None,
            signed_in: false,
            loading: false,
            accounts: Vec::new(),
            workspaces: Vec::new(),
            active_workspace: 0,
            channels: Vec::new(),
            sidebar_sections: Vec::new(),
            active_channel: 0,
            messages: Vec::new(),
            messages_by_channel: HashMap::new(),
            timeline_start: 0,
            timeline_end: 0,
            row_heights: HashMap::new(),
            message_arrivals: HashMap::new(),
            selection_pinned: false,
            upload_ui_epoch: 0,
            stick_to_bottom: true,
            loading_older: false,
            chat_paused: false,
            main_view: MainView::Home,
            dm_query: String::new(),
            dm_unread_only: false,
            activity_tab: ActivityTab::default(),
            activity_detail_open: false,
            profile_hover: None,
            overlay: None,
            palette_query: String::new(),
            palette_selected: 0,
            search_query: String::new(),
            search_results: Vec::new(),
            search_loading: false,
            thread_root: None,
            thread_messages: Vec::new(),
            profile_user: None,
            viewer: None,
            toast: None,
            performance: PerformanceVm::default(),
            channel_switch_started: None,
            realtime_insert_started: None,
        }
    }

    pub fn fixture(media: MediaRegistry) -> Self {
        let core = fixture_core();
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
            reactions: vec![("🚀".into(), 4, false), ("eyes".into(), 2, false)],
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
            reactions: vec![("white_check_mark".into(), 6, false)],
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
        let (channels, sidebar_sections, default_active, _) = project_channels(&workspace, &media);
        let active_channel = channels
            .iter()
            .position(|channel| channel.id == "C2")
            .unwrap_or(default_active);
        let mut messages_by_channel = HashMap::from([("C2".into(), messages.clone())]);
        messages_by_channel.insert(
            "D1".into(),
            vec![MessageVm {
                id: "dm1".into(),
                ts: "1719801000.000400".into(),
                user_id: Some("U1".into()),
                author: "Maya Chen".into(),
                timestamp: super_platinum_core::state::format_ts_hm("1719801000.000400"),
                avatar_initials: "MC".into(),
                avatar: Some(maya_avatar.clone()),
                body: vec![RichNode::Text("Can you review the desktop capture?".into())],
                ..Default::default()
            }],
        );
        let timeline_end = messages.len();
        Self {
            core,
            media,
            media_epoch: 0,
            background_uri: None,
            signed_in: true,
            loading: false,
            accounts: vec![AccountVm {
                id: "fixture".into(),
                label: "Fixture account".into(),
                active: true,
            }],
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
            active_channel,
            messages,
            messages_by_channel,
            timeline_start: timeline_end.saturating_sub(100),
            timeline_end,
            row_heights: HashMap::new(),
            message_arrivals: HashMap::new(),
            selection_pinned: false,
            upload_ui_epoch: 0,
            stick_to_bottom: true,
            loading_older: false,
            chat_paused: false,
            main_view: MainView::Home,
            dm_query: String::new(),
            dm_unread_only: false,
            activity_tab: ActivityTab::default(),
            activity_detail_open: false,
            profile_hover: None,
            overlay: None,
            palette_query: String::new(),
            palette_selected: 0,
            search_query: String::new(),
            search_results: Vec::new(),
            search_loading: false,
            thread_root: None,
            thread_messages: Vec::new(),
            profile_user: None,
            viewer: None,
            toast: None,
            performance: PerformanceVm::default(),
            channel_switch_started: None,
            realtime_insert_started: None,
        }
    }

    pub(crate) fn fixture_variant(media: MediaRegistry, name: &str) -> Self {
        let mut state = Self::fixture(media);
        match name {
            "unreads" => state.main_view = MainView::Unreads,
            "dms" => state.main_view = MainView::Dms,
            "dm-header-compact" => {
                state.main_view = MainView::Dms;
                if let Some(index) = state.channels.iter().position(|channel| channel.id == "D1") {
                    state.select_channel(index);
                }
            }
            "activity-unread" => {
                state.main_view = MainView::Activity;
                state.core.activity.unread_only = true;
            }
            "activity-channel-post" => state.main_view = MainView::Activity,
            "accounts" => state.overlay = Some(Overlay::Accounts),
            "palette" => state.overlay = Some(Overlay::Palette),
            "animated-reaction" => {
                keep_recent_messages(&mut state, 8);
                if let Some(message) = state.messages.last_mut() {
                    message.reactions = vec![("🚀".into(), 12, true), ("eyes".into(), 4, false)];
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
                state.profile_user = Some("U1".into());
                state.profile_hover = Some("U1".into());
            }
            "profile-pane" | "channel-thread-profile-headers" => {
                state.profile_user = Some("U1".into());
                state.overlay = Some(Overlay::Profile);
            }
            "channel-huddle" => {
                if let Some(workspace) = state.core.workspaces.get_mut("T1") {
                    workspace.apply_room(serde_json::from_value(serde_json::json!({"id":"R1","channels":["C2"],"huddle_link":"https://app.slack.com/huddle/T1/C2","participants":["U1","U2"]})).expect("fixture room"));
                }
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
                state.background_uri = Some(id.uri());
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
            "video-viewer" => state.open_fixture_viewer("video/mp4"),
            "gif-picker-attachment" => {
                let id = super_platinum_core::MediaAssetId::new(MediaAssetKind::Attachment);
                state.media.insert(
                    id.clone(),
                    "image/png",
                    include_bytes!("../../../assets/icons/icon-256.png").as_slice(),
                );
                if let Some(message) = state.messages.last_mut() {
                    message.body.push(RichNode::Paragraph(vec![RichNode::Media {
                        id,
                        name: "celebration.gif".into(),
                        mime: "image/png".into(),
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
            "dm-history-failed" => {
                state.main_view = MainView::Dms;
                state.toast = Some("History refresh failed; cached messages remain visible".into());
            }
            "dm-cached-refresh-failed" => {
                state.main_view = MainView::Dms;
                state.toast =
                    Some("Workspace refresh failed; showing cached direct messages".into());
            }
            "main-dev" => {
                if let Some(index) = state
                    .channels
                    .iter()
                    .position(|channel| channel.id == "C3" || channel.name == "design")
                {
                    state.select_channel(index);
                }
            }
            _ => {}
        }
        state
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
            id,
            name: if mime.starts_with("video/") {
                "demo.mp4"
            } else {
                "super-platinum.png"
            }
            .into(),
            mime: mime.into(),
        });
        self.overlay = Some(Overlay::Viewer);
    }

    fn sync_fixture_messages(&mut self) {
        self.messages_by_channel
            .insert("C2".into(), self.messages.clone());
        self.reset_timeline_window();
    }
}
