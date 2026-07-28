mod account;
mod discovery;
mod messaging;

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use serde_json::json;

use super::subscription::visible_media_animation_interval;
use super::update::{
    begin_mark, channel_needs_hydration, channel_open_scroll_target, emoji_preview_from_bytes,
    import_background_sync, is_permanent_mark_error, needs_user_hydration,
    notification_for_message, pending_target_ts, preferred_channel, scope_message,
    should_auto_load_activity, should_load_older_activity, should_load_older_history,
    thread_panel_x_range, unique_download_path, update,
};
use super::*;
use crate::slack::Error as SlackError;
use crate::slack::events::RtEvent;
use crate::slack::models::{
    ActivityItem, Channel, File, HistoryPage, Message as SlackMessage, ProfileFieldValue,
    ResponseMetadata, SearchItem, SearchMessagesPage, SearchPagination, SentMessage,
    TeamProfileField, User, UserProfile,
};
use crate::slack::realtime::Connection;
use crate::state::{ChannelMessages, MainView, Presence, RealtimeStatus};

pub(super) const SELF_USER: &str = "U_SELF";

pub(super) fn msg(user: &str, ts: &str, text: &str) -> SlackMessage {
    SlackMessage {
        user: Some(user.into()),
        ts: Some(ts.into()),
        text: Some(text.into()),
        ..Default::default()
    }
}

pub(super) fn loaded_channel(user: &str, ts: &str, text: &str) -> ChannelMessages {
    let mut cm = ChannelMessages::default();
    cm.upsert(msg(user, ts, text));
    cm.loaded = true;
    cm
}

pub(super) fn loaded_history(page: HistoryPage) -> LoadedHistory {
    LoadedHistory {
        page,
        replace_cached: false,
    }
}

pub(super) fn live_test_app() -> App {
    let mut app = test_app();
    app.session = Some(account_session("T_TEST", SELF_USER, "Test"));
    app.transport = Some(Arc::new(Transport::new("test-cookie").unwrap()));
    app
}

fn test_user(id: &str, display_name: &str) -> User {
    User {
        id: id.into(),
        name: Some(display_name.to_ascii_lowercase()),
        real_name: Some(display_name.into()),
        profile: Some(UserProfile {
            display_name: Some(display_name.into()),
            real_name: Some(display_name.into()),
            ..Default::default()
        }),
        ..Default::default()
    }
}

pub(super) fn test_workspace() -> Workspace {
    let mut channels = BTreeMap::new();
    for (id, name) in [("C_GENERAL", "general"), ("C_DEV", "dev")] {
        channels.insert(
            id.into(),
            Channel {
                id: id.into(),
                name: Some(name.into()),
                is_channel: true,
                ..Default::default()
            },
        );
    }

    let mut general = ChannelMessages::default();
    general.upsert(msg(
        "U_ALICE",
        "1783372300.000100",
        "morning — shipping the agent UI harness today",
    ));
    general.upsert(msg(
        "U_BOB",
        "1783372310.000200",
        "sounds good. can we keep the layout dense?",
    ));
    general.upsert(msg(
        SELF_USER,
        "1783372320.000300",
        "yep — quiet chrome, no marketing fluff",
    ));
    general.loaded = true;

    let mut dev = ChannelMessages::default();
    dev.upsert(msg(
        "U_BOB",
        "1783370000.000100",
        "question: how do agents capture screenshots?",
    ));
    dev.upsert(msg(
        "U_ALICE",
        "1783370010.000200",
        "headless iced_test + fixture state",
    ));
    dev.loaded = true;

    let messages = HashMap::from([("C_GENERAL".into(), general), ("C_DEV".into(), dev)]);

    let users = HashMap::from([
        (SELF_USER.into(), test_user(SELF_USER, "You")),
        ("U_ALICE".into(), test_user("U_ALICE", "Alice")),
        ("U_BOB".into(), test_user("U_BOB", "Bob")),
    ]);

    Workspace {
        team_id: "T_TEST".into(),
        name: "Test".into(),
        url: "https://test.slack.com".into(),
        self_user_id: SELF_USER.into(),
        activity_unread_count: None,
        channels,
        starred_order: Vec::new(),
        dm_order: Vec::new(),
        recent_channels: Vec::new(),
        last_active_channel: None,
        priority_scores: BTreeMap::new(),
        frecency: BTreeMap::new(),
        hide_read_channels_unless_starred: false,
        priority_sidebar_section: false,
        vip_users: std::collections::HashSet::new(),
        sidebar: Default::default(),
        users,
        custom_emoji: HashMap::new(),
        messages,
        typing: HashMap::new(),
        presence: HashMap::new(),
        active_huddles: HashMap::new(),
        rt: RealtimeStatus::default(),
        rt_generation: 1,
    }
}

pub(super) fn test_app() -> App {
    let mut app = App::empty();
    app.settings = config::Settings::default();
    ui::theme::apply(&app.settings);
    let ws = test_workspace();
    let team = ws.team_id.clone();
    app.workspaces.insert(team.clone(), ws);
    app.active_team = Some(team);
    app.active_channel = Some("C_GENERAL".into());
    app.screen = Screen::Main;
    app
}

pub(super) fn gif_picker_app() -> App {
    let mut app = test_app();
    let url = "https://media0.giphy.com/media/OIKS4GcqcKqNtndh1o/200w.gif?rid=200w.gif";
    let message = SlackMessage {
        user: Some("U_ALICE".into()),
        ts: Some("1783372400.000100".into()),
        text: Some(String::new()),
        attachments: vec![crate::slack::models::Attachment {
            blocks: vec![json!({
                "type": "image",
                "image_url": url,
                "image_width": 200,
                "image_height": 206,
                "image_bytes": 17943,
                "is_animated": true,
                "alt_text": "Main Character Instagram GIF"
            })],
            extra: BTreeMap::from([("fallback".into(), json!("shared a GIF"))]),
            ..Default::default()
        }],
        ..Default::default()
    };
    app.active_workspace_mut()
        .expect("workspace")
        .messages
        .get_mut("C_GENERAL")
        .expect("general")
        .upsert(message);
    app.file_previews.insert(
        url.into(),
        FilePreview::Animated {
            frames: vec![
                ImageHandle::from_rgba(
                    2,
                    2,
                    vec![
                        210, 62, 90, 255, 210, 62, 90, 255, 210, 62, 90, 255, 210, 62, 90, 255,
                    ],
                ),
                ImageHandle::from_rgba(
                    2,
                    2,
                    vec![
                        67, 160, 120, 255, 67, 160, 120, 255, 67, 160, 120, 255, 67, 160, 120, 255,
                    ],
                ),
            ],
            allocations: Vec::new(),
            delays: vec![Duration::from_millis(40), Duration::from_millis(60)],
            total: Duration::from_millis(100),
        },
    );
    app
}

pub(super) fn animated_reaction_app() -> App {
    let mut app = test_app();
    let ws = app.active_workspace_mut().expect("workspace");
    ws.custom_emoji.insert(
        "party".into(),
        crate::slack::models::Emoji {
            name: "party".into(),
            value: "https://emoji.slack-edge.com/T_TEST/party/animated.gif".into(),
            ..Default::default()
        },
    );
    ws.messages.get_mut("C_GENERAL").expect("general").messages[0]
        .reactions
        .push(crate::slack::models::Reaction {
            name: "party".into(),
            users: vec!["U_ALICE".into()],
            count: 1,
            ..Default::default()
        });
    app.emoji_previews.insert(
        crate::state::emoji_preview_key("T_TEST", "party"),
        FilePreview::Animated {
            frames: vec![
                ImageHandle::from_bytes(&include_bytes!("../../assets/icons/png/icon_32.png")[..]),
                ImageHandle::from_bytes(&include_bytes!("../../assets/icons/png/icon_48.png")[..]),
            ],
            allocations: Vec::new(),
            delays: vec![Duration::from_millis(20), Duration::from_millis(50)],
            total: Duration::from_millis(70),
        },
    );
    app
}

pub(super) fn profile_app() -> App {
    let mut app = test_app();
    let ws = app.active_workspace_mut().expect("workspace");
    let alice = ws.users.get_mut("U_ALICE").expect("Alice fixture");
    alice.tz_offset = Some(-14_400);
    alice.im_mpim_ids = vec!["D_ALICE".into(), "G_ALICE_BOB".into(), "G_TEAM".into()];
    alice.has_more_mpims = true;
    let profile = alice.profile.as_mut().expect("Alice profile");
    profile.title = Some("Product designer".into());
    profile.pronouns = Some("she/her".into());
    profile.email = Some("alice@example.com".into());
    profile.phone = Some("+1 212 555 0142".into());
    profile.start_date = Some("2024-03-18".into());
    profile.status_emoji = Some(":seedling:".into());
    profile.status_text = Some("Growing good interfaces".into());
    profile.fields.insert(
        "X_MANAGER".into(),
        ProfileFieldValue {
            value: json!("U_BOB"),
            ..Default::default()
        },
    );
    ws.presence.insert("U_ALICE".into(), Presence::Active);
    ws.vip_users.insert("U_ALICE".into());
    ws.channels.insert(
        "D_ALICE".into(),
        Channel {
            id: "D_ALICE".into(),
            is_im: true,
            user: Some("U_ALICE".into()),
            ..Default::default()
        },
    );
    for (id, name) in [("G_ALICE_BOB", "alice--bob"), ("G_TEAM", "alice--bob--you")] {
        ws.channels.insert(
            id.into(),
            Channel {
                id: id.into(),
                name: Some(name.into()),
                is_group: true,
                is_mpim: true,
                ..Default::default()
            },
        );
    }
    ws.messages.insert(
        "D_ALICE".into(),
        loaded_channel("U_ALICE", "1783372400.000100", "See you there"),
    );
    app.profile_fields.insert(
        "T_TEST".into(),
        vec![TeamProfileField {
            id: "X_MANAGER".into(),
            label: Some("Manager".into()),
            field_type: Some("user".into()),
            ..Default::default()
        }],
    );
    app
}

fn account_session(team: &str, user: &str, name: &str) -> Session {
    Session {
        d_cookie: format!("cookie-{user}"),
        workspaces: BTreeMap::from([(
            team.into(),
            config::WorkspaceSession {
                team_id: team.into(),
                enterprise_id: None,
                user_id: user.into(),
                name: name.into(),
                url: format!("https://{}.slack.com", name.to_ascii_lowercase()),
                token: format!("token-{user}"),
            },
        )]),
    }
}

pub(super) fn account_menu_app() -> App {
    let mut app = test_app();
    let active = "11111111-1111-4111-8111-111111111111".to_owned();
    let other = "22222222-2222-4222-8222-222222222222".to_owned();
    app.accounts = BTreeMap::from([
        (active.clone(), account_session("T_TEST", SELF_USER, "Test")),
        (other, account_session("T_OTHER", "U_OTHER", "Other")),
    ]);
    app.active_account = Some(active);
    app.show_account_menu = true;
    app.account_menu_open = true;
    app
}

pub(super) fn thread_unread_app() -> App {
    let mut app = test_app();
    let team = app.active_team.clone().unwrap();
    let root_ts = "1783372300.000100".to_owned();
    let anchor = "1783372360.000400".to_owned();
    let _ = update(
        &mut app,
        Message::Conversation(crate::app::ConversationMessage::ThreadOpened {
            channel: "C_GENERAL".into(),
            ts: root_ts.clone(),
            unread_range: Some((anchor.clone(), "1783372370.000500".into())),
        }),
    );
    let _ = update(
        &mut app,
        Message::Conversation(crate::app::ConversationMessage::ThreadLoaded {
            team,
            channel: "C_GENERAL".into(),
            root_ts: root_ts.clone(),
            unread_anchor: Some(anchor.clone()),
            result: Ok(HistoryPage {
                messages: vec![
                    msg(
                        "U_ALICE",
                        &root_ts,
                        "morning — shipping the agent UI harness today",
                    ),
                    SlackMessage {
                        thread_ts: Some(root_ts.clone()),
                        ..msg(SELF_USER, "1783372350.000300", "already read this reply")
                    },
                    SlackMessage {
                        thread_ts: Some(root_ts.clone()),
                        ..msg("U_BOB", &anchor, "this reply is still unread")
                    },
                    SlackMessage {
                        thread_ts: Some(root_ts),
                        ..msg("U_ALICE", "1783372370.000500", "so is this one")
                    },
                ],
                ..Default::default()
            }),
        }),
    );
    app.main_view = MainView::Activity;
    app.activity.loaded = true;
    app
}

pub(super) fn activity_app() -> App {
    let mut app = test_app();
    app.main_view = MainView::Activity;
    app.activity.loaded = true;
    app.active_workspace_mut().unwrap().activity_unread_count = Some(1);

    for (is_unread, key, ts, text) in [
        (
            true,
            "unread-mention",
            "1783372300.000100",
            "Unread design review",
        ),
        (
            false,
            "read-mention",
            "1783285900.000100",
            "Read launch recap",
        ),
    ] {
        let item: ActivityItem = serde_json::from_value(json!({
            "is_unread": is_unread,
            "feed_ts": ts,
            "key": key,
            "item": {
                "type": "at_user",
                "message": {
                    "ts": ts,
                    "channel": "C_GENERAL",
                    "author_user_id": "U_ALICE"
                }
            }
        }))
        .unwrap();
        app.activity
            .hydrated
            .insert(("C_GENERAL".into(), ts.into()), msg("U_ALICE", ts, text));
        app.activity.upsert(item);
    }

    let channel_post: ActivityItem = serde_json::from_value(json!({
        "is_unread": true,
        "feed_ts": "1783372400.000100",
        "key": "channel-C_DEV",
        "item": {
            "type": "channel",
            "bundle_info": {
                "payload": {
                    "channel_entry": {
                        "latest_message": {
                            "ts": "1783372400.000100",
                            "channel": "C_DEV"
                        },
                        "unread_msg_count": 2
                    }
                }
            }
        }
    }))
    .unwrap();
    app.activity.hydrated.insert(
        ("C_DEV".into(), "1783372400.000100".into()),
        msg("U_ALICE", "1783372400.000100", "Subscribed channel post"),
    );
    app.activity.upsert(channel_post);

    app
}

pub(super) fn dms_app() -> App {
    let mut app = test_app();
    app.main_view = MainView::Dms;
    app.dms.loaded = true;

    let ws = app.active_workspace_mut().unwrap();
    ws.channels.insert(
        "D_ALICE".into(),
        Channel {
            id: "D_ALICE".into(),
            is_im: true,
            user: Some("U_ALICE".into()),
            unread_count: Some(2),
            last_read: Some("1783372000.000100".into()),
            ..Default::default()
        },
    );
    ws.channels.insert(
        "D_BOB".into(),
        Channel {
            id: "D_BOB".into(),
            is_im: true,
            user: Some("U_BOB".into()),
            last_read: Some("1783372310.000200".into()),
            ..Default::default()
        },
    );

    for (id, user, ts, text) in [
        (
            "D_ALICE",
            "U_ALICE",
            "1783372300.000100",
            "unread dm from alice",
        ),
        ("D_BOB", SELF_USER, "1783372310.000200", "read reply to bob"),
    ] {
        app.dms.upsert(crate::slack::models::DmEntry {
            id: id.into(),
            latest: Some(ts.into()),
            message: Some(msg(user, ts, text)),
            channel: None,
            ..Default::default()
        });
    }

    app
}

pub(super) fn multi_paragraph_emoji_app() -> App {
    let mut app = test_app();
    let team = app.active_team.clone().unwrap();
    let ws = app.workspaces.get_mut(&team).unwrap();
    ws.custom_emoji.insert(
        "cryin".into(),
        crate::slack::models::Emoji {
            name: "cryin".into(),
            value: "https://emoji.slack-edge.com/T_TEST/cryin/abc.png".into(),
            ..Default::default()
        },
    );
    ws.users
        .insert("U_SJ".into(), test_user("U_SJ", "Sjpark01"));

    let msg = SlackMessage {
        user: Some("U_SJ".into()),
        ts: Some("1783610969.018589".into()),
        text: Some(
            "<https://stardance.hackclub.com/projects/4909|stardance.hackclub.com/projects/4909> shipped a while back, but forgor to post here\nA Hackpad, arranged like a piano! It can act as a midi keyboard with the right key bindings :slightly_smiling_face:\nI honestly spent way too much time on this, and would have spent more, but I got burnt out and just wanted to move on :cryin: ambition gets me again\ntechnically it's up for funding review, but it's basically the same a ship, right?".into(),
        ),
        blocks: vec![json!({
            "type": "rich_text",
            "elements": [{
                "type": "rich_text_section",
                "elements": [
                    {
                        "type": "link",
                        "url": "https://stardance.hackclub.com/projects/4909",
                        "text": "stardance.hackclub.com/projects/4909"
                    },
                    {
                        "type": "text",
                        "text": " shipped a while back, but forgor to post here\nA Hackpad, arranged like a piano! It can act as a midi keyboard with the right key bindings "
                    },
                    {"type": "emoji", "name": "slightly_smiling_face", "unicode": "1f642"},
                    {
                        "type": "text",
                        "text": "\nI honestly spent way too much time on this, and would have spent more, but I got burnt out and just wanted to move on "
                    },
                    {"type": "emoji", "name": "cryin"},
                    {
                        "type": "text",
                        "text": " ambition gets me again\ntechnically it's up for funding review, but it's basically the same a ship, right?"
                    }
                ]
            }]
        })],
        attachments: vec![crate::slack::models::Attachment {
            title: Some("Hack Piano by @sjpark01 | Stardance".into()),
            text: Some(
                "Hackpad but the keys are arranged like a piano.\nI guess this makes it a MIDI keyboard of sorts".into(),
            ),
            service_name: Some("Stardance - Hack Club".into()),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut general = ChannelMessages::default();
    general.upsert(msg);
    general.loaded = true;
    ws.messages.insert("C_GENERAL".into(), general);
    app
}

pub(super) fn image_viewer_app(embedded: bool) -> App {
    let mut app = test_app();
    let width = 960_u32;
    let height = 540_u32;
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let band = ((x / 80 + y / 60) % 2) as u8;
            pixels.extend_from_slice(&[
                32 + band * 26,
                104 + (x * 90 / width) as u8,
                150 + (y * 70 / height) as u8,
                255,
            ]);
        }
    }
    let handle = ImageHandle::from_rgba(width, height, pixels);
    let avatar = ImageHandle::from_rgba(
        2,
        2,
        vec![
            108, 190, 210, 255, 108, 190, 210, 255, 84, 150, 184, 255, 84, 150, 184, 255,
        ],
    );
    app.avatar_previews
        .insert("U_ALICE".into(), FilePreview::Loaded(avatar));
    let timestamp = format!("{}.000100", crate::state::now_secs() - 12 * 60 * 60);

    let (source, message) = if embedded {
        let preview_key = "https://cdn.example.com/thumb/roadmap.png".to_owned();
        app.file_previews
            .insert(preview_key.clone(), FilePreview::Loaded(handle.clone()));
        (
            ImageViewerSource {
                kind: MediaViewerKind::Image,
                preview_key,
                full_url: "https://cdn.example.com/images/roadmap.png".into(),
                download_url: "https://cdn.example.com/images/roadmap.png".into(),
                fetch_auth: ImageFetchAuth::Public,
                filename: "Product roadmap.png".into(),
                author_name: "Alice".into(),
                avatar_key: Some("U_ALICE".into()),
                timestamp: timestamp.clone(),
                conversation: "#general".into(),
            },
            SlackMessage {
                user: Some("U_ALICE".into()),
                ts: Some(timestamp.clone()),
                text: Some("Latest product roadmap".into()),
                attachments: vec![crate::slack::models::Attachment {
                    title: Some("Product roadmap.png".into()),
                    image_url: Some("https://cdn.example.com/images/roadmap.png".into()),
                    thumb_url: Some("https://cdn.example.com/thumb/roadmap.png".into()),
                    ..Default::default()
                }],
                ..Default::default()
            },
        )
    } else {
        app.file_previews
            .insert("F_LAUNCH".into(), FilePreview::Loaded(handle.clone()));
        (
            ImageViewerSource {
                kind: MediaViewerKind::Image,
                preview_key: "F_LAUNCH".into(),
                full_url: "https://files.slack.com/files-pri/T/F/launch-board.png".into(),
                download_url: "https://files.slack.com/files-pri/T/F/launch-board.png".into(),
                fetch_auth: ImageFetchAuth::Slack,
                filename: "launch-board.png".into(),
                author_name: "Alice".into(),
                avatar_key: Some("U_ALICE".into()),
                timestamp: timestamp.clone(),
                conversation: "#general".into(),
            },
            SlackMessage {
                user: Some("U_ALICE".into()),
                ts: Some(timestamp.clone()),
                text: Some("Launch board".into()),
                files: vec![File {
                    id: Some("F_LAUNCH".into()),
                    name: Some("launch-board.png".into()),
                    mimetype: Some("image/png".into()),
                    url_private: Some(
                        "https://files.slack.com/files-pri/T/F/launch-board.png".into(),
                    ),
                    thumb_360: Some(
                        "https://files.slack.com/files-tmb/T/F/launch-board_360.png".into(),
                    ),
                    extra: BTreeMap::from([
                        ("original_w".into(), json!(960)),
                        ("original_h".into(), json!(540)),
                    ]),
                    ..Default::default()
                }],
                ..Default::default()
            },
        )
    };
    app.active_workspace_mut()
        .unwrap()
        .messages
        .get_mut("C_GENERAL")
        .unwrap()
        .upsert(message);
    app.image_viewer = Some(ImageViewerState {
        source,
        image: ImageViewerImage::Loaded(handle),
        generation: 1,
        open: true,
        zoom: 1.0,
        offset: iced::Vector::ZERO,
        video: None,
    });
    app
}

pub(super) fn video_viewer_app() -> App {
    let mut app = image_viewer_app(false);
    let viewer = app.image_viewer.as_mut().expect("viewer fixture");
    let timestamp = viewer.source.timestamp.clone();
    viewer.source.kind = MediaViewerKind::Video;
    viewer.source.filename = "launch-demo.mp4".into();
    viewer.source.full_url = "https://files.slack.com/files-pri/T/F/launch-demo.mp4".into();
    viewer.source.download_url = viewer.source.full_url.clone();
    viewer.video = Some(VideoViewerPlayback {
        duration: 11.12,
        position: 3.4,
        playing: true,
        volume: 0.72,
        ..VideoViewerPlayback::default()
    });
    app.active_workspace_mut()
        .unwrap()
        .messages
        .get_mut("C_GENERAL")
        .unwrap()
        .upsert(SlackMessage {
            user: Some("U_ALICE".into()),
            ts: Some(timestamp),
            text: Some("Launch demo".into()),
            files: vec![File {
                id: Some("F_LAUNCH".into()),
                name: Some("launch-demo.mp4".into()),
                mimetype: Some("video/mp4".into()),
                filetype: Some("mp4".into()),
                url_private: Some("https://files.slack.com/files-pri/T/F/launch-demo.mp4".into()),
                extra: BTreeMap::from([
                    (
                        "mp4".into(),
                        json!("https://files.slack.com/files-tmb/T/F/launch-demo.mp4"),
                    ),
                    (
                        "thumb_video".into(),
                        json!("https://files.slack.com/files-tmb/T/F/launch-demo.jpeg"),
                    ),
                    ("thumb_video_w".into(), json!(960)),
                    ("thumb_video_h".into(), json!(540)),
                    (
                        "url_private_download".into(),
                        json!("https://files.slack.com/files-pri/T/F/download/launch-demo.mp4"),
                    ),
                ]),
                ..Default::default()
            }],
            ..Default::default()
        });
    app
}

pub(super) fn login_app() -> App {
    let mut app = App::empty();
    app.settings = config::Settings::default();
    ui::theme::apply(&app.settings);
    app.screen = Screen::Login;
    app
}

pub(super) fn settings_app() -> App {
    let mut app = test_app();
    app.show_settings = true;
    app.settings_open = true;
    app
}

pub(super) fn search_app() -> App {
    let mut app = test_app();
    let team = app.active_team.clone().unwrap();
    app.search = Some(SearchState {
        query: "standup".into(),
        team,
        page: 1,
        page_count: 2,
        total: 3,
        hits: vec![SearchHit {
            channel: "C_GENERAL".into(),
            channel_label: "#general".into(),
            message: msg("U_ALICE", "1783372300.000100", "morning standup notes"),
        }],
        loading: false,
    });
    app
}

fn add_second_workspace(app: &mut App) {
    let mut ws = test_workspace();
    ws.team_id = "T_SECOND".into();
    ws.name = "Second".into();
    ws.url = "https://second.slack.com".into();
    ws.channels.clear();
    ws.channels.insert(
        "C_SECOND".into(),
        Channel {
            id: "C_SECOND".into(),
            name: Some("second".into()),
            is_channel: true,
            ..Default::default()
        },
    );
    ws.messages = HashMap::from([(
        "C_SECOND".into(),
        loaded_channel("U_SECOND", "1783375000.000100", "second workspace"),
    )]);
    app.workspaces.insert(ws.team_id.clone(), ws);
}
