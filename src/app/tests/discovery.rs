use super::*;

fn search_page(page: u32, page_count: u32, total: u64) -> SearchMessagesPage {
    SearchMessagesPage {
        items: vec![SearchItem {
            channel: Some(Channel {
                id: "C_GENERAL".into(),
                name: Some("general".into()),
                is_channel: true,
                ..Default::default()
            }),
            messages: vec![SlackMessage {
                thread_ts: Some("1783372200.000000".into()),
                ..msg("U_ALICE", "1783372300.000100", "morning standup")
            }],
            ..Default::default()
        }],
        pagination: Some(SearchPagination {
            page: Some(page),
            page_count: Some(page_count),
            total_count: Some(total),
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn searching(app: &App) -> SearchState {
    SearchState {
        query: "standup".into(),
        team: app.active_team.clone().unwrap(),
        page: 1,
        page_count: 0,
        total: 0,
        hits: Vec::new(),
        loading: true,
    }
}

#[test]
fn search_submitted_starts_loading_state() {
    let mut app = test_app();
    let _ = update(
        &mut app,
        Message::Discovery(crate::app::DiscoveryMessage::SearchInputChanged(
            "  standup ".into(),
        )),
    );
    let _ = update(
        &mut app,
        Message::Discovery(crate::app::DiscoveryMessage::SearchSubmitted),
    );
    let state = app.search.as_ref().expect("search active");
    assert_eq!(state.query, "standup");
    assert_eq!(state.page, 1);
    assert!(state.loading);
}

#[test]
fn empty_search_is_noop() {
    let mut app = test_app();
    app.search_input = "   ".into();
    let _ = update(
        &mut app,
        Message::Discovery(crate::app::DiscoveryMessage::SearchSubmitted),
    );
    assert!(app.search.is_none());
}

#[test]
fn search_loaded_populates_hits_and_pagination() {
    let mut app = test_app();
    let team = app.active_team.clone().unwrap();
    app.search = Some(searching(&app));

    let _ = update(
        &mut app,
        Message::Discovery(crate::app::DiscoveryMessage::SearchLoaded {
            team: team.clone(),
            query: "standup".into(),
            page: 1,
            result: Ok(search_page(1, 3, 42)),
        }),
    );

    let state = app.search.as_ref().unwrap();
    assert!(!state.loading);
    assert_eq!(state.hits.len(), 1);
    assert_eq!(state.hits[0].channel, "C_GENERAL");
    assert_eq!(state.hits[0].channel_label, "#general");
    assert_eq!(state.page_count, 3);
    assert_eq!(state.total, 42);
}

#[test]
fn search_loaded_ignores_stale_query() {
    let mut app = test_app();
    let team = app.active_team.clone().unwrap();
    app.search = Some(searching(&app));

    let _ = update(
        &mut app,
        Message::Discovery(crate::app::DiscoveryMessage::SearchLoaded {
            team,
            query: "different".into(),
            page: 1,
            result: Ok(search_page(1, 3, 42)),
        }),
    );

    let state = app.search.as_ref().unwrap();
    assert!(state.hits.is_empty());
    assert!(state.loading);
}

#[test]
fn search_page_request_out_of_bounds_is_noop() {
    let mut app = test_app();
    let mut state = searching(&app);
    state.page = 1;
    state.page_count = 3;
    state.loading = false;
    app.search = Some(state);

    let _ = update(
        &mut app,
        Message::Discovery(crate::app::DiscoveryMessage::SearchPageRequested(9)),
    );

    let state = app.search.as_ref().unwrap();
    assert_eq!(state.page, 1);
    assert!(!state.loading);
}

#[test]
fn search_result_opens_channel_and_thread_and_clears_search() {
    let mut app = test_app();
    app.active_channel = Some("C_DEV".into());
    app.search = Some(searching(&app));

    let _ = update(
        &mut app,
        Message::Discovery(crate::app::DiscoveryMessage::SearchResultSelected {
            channel: "C_GENERAL".into(),
            ts: "1783372300.000100".into(),
            thread_ts: Some("1783372200.000000".into()),
        }),
    );

    assert!(app.search.is_none());
    assert_eq!(app.active_channel.as_deref(), Some("C_GENERAL"));
    assert_eq!(
        app.active_thread.as_ref(),
        Some(&("C_GENERAL".into(), "1783372200.000000".into()))
    );
}

#[test]
fn permanent_mark_errors_are_classified() {
    assert!(is_permanent_mark_error(&SlackError::Api(
        "channel_not_found".into()
    )));
    assert!(is_permanent_mark_error(&SlackError::Api(
        "is_archived".into()
    )));
    assert!(!is_permanent_mark_error(&SlackError::Api(
        "fatal_error".into()
    )));
    assert!(!is_permanent_mark_error(&SlackError::Transport(
        "connection reset".into()
    )));
    assert!(!is_permanent_mark_error(&SlackError::RateLimited {
        retry_after_secs: Some(1)
    }));
}

#[test]
fn mark_gate_dedupes_in_flight_and_blocks_channel_not_found() {
    let mut app = test_app();
    let team = app.active_team.clone().unwrap();
    let channel = "C_DEAD".to_string();
    let ts = "1783370000.000200".to_string();

    assert!(begin_mark(&mut app, &team, &channel, &ts));
    assert!(
        !begin_mark(&mut app, &team, &channel, &ts),
        "identical in-flight (team, channel, ts) must not reschedule"
    );
    assert_eq!(app.pending_marks.len(), 1);

    let _ = update(
        &mut app,
        Message::Workspace(crate::app::WorkspaceMessage::ChannelMarked(
            team.clone(),
            channel.clone(),
            ts.clone(),
            Err(SlackError::Api("channel_not_found".into())),
        )),
    );

    assert!(app.pending_marks.is_empty());
    assert!(app.mark_blocked.contains(&ReadTarget::Conversation {
        team: team.clone(),
        channel: channel.clone(),
    }));
    assert!(
        !begin_mark(&mut app, &team, &channel, &ts),
        "channel_not_found must block further marks this session"
    );
    assert!(!begin_mark(
        &mut app,
        &team,
        &channel,
        &"1783370000.000300".into()
    ));
    assert!(begin_mark(
        &mut app,
        &team,
        &"C_GENERAL".into(),
        &"1783372300.000100".into()
    ));
}

#[test]
fn mark_gate_allows_retry_after_transient_error() {
    let mut app = test_app();
    let team = app.active_team.clone().unwrap();
    let channel = "C_GENERAL".to_string();
    let ts = "1783372300.000100".to_string();

    assert!(begin_mark(&mut app, &team, &channel, &ts));
    let _ = update(
        &mut app,
        Message::Workspace(crate::app::WorkspaceMessage::ChannelMarked(
            team.clone(),
            channel.clone(),
            ts.clone(),
            Err(SlackError::Transport("timeout".into())),
        )),
    );

    assert!(app.pending_marks.is_empty());
    assert!(!app.mark_blocked.contains(&ReadTarget::Conversation {
        team: team.clone(),
        channel: channel.clone(),
    }));
    assert!(
        begin_mark(&mut app, &team, &channel, &ts),
        "transient failures must remain retryable"
    );
}

#[test]
fn mark_success_updates_last_read_without_optimistic_write() {
    let mut app = test_app();
    let team = app.active_team.clone().unwrap();
    let channel = "C_GENERAL".to_string();
    let ts = "1783372300.000100".to_string();
    app.active_workspace_mut().unwrap().activity_unread_count = Some(2);
    app.activity.items.push(
        serde_json::from_value(json!({
            "is_unread": true,
            "feed_ts": ts,
            "key": "channel-C_GENERAL",
            "item": {
                "type": "channel",
                "bundle_info": {"payload": {"channel_entry": {
                    "latest_message": {"channel": channel, "ts": ts},
                    "unread_msg_count": 1
                }}}
            }
        }))
        .unwrap(),
    );

    {
        let cm = app
            .workspaces
            .get_mut(&team)
            .unwrap()
            .messages
            .get_mut(&channel)
            .unwrap();
        cm.last_read = Some("1783370000.000100".into());
    }
    assert!(begin_mark(&mut app, &team, &channel, &ts));
    assert_eq!(
        app.workspaces[&team].messages[&channel]
            .last_read
            .as_deref(),
        Some("1783370000.000100")
    );

    let _ = update(
        &mut app,
        Message::Workspace(crate::app::WorkspaceMessage::ChannelMarked(
            team.clone(),
            channel.clone(),
            ts.clone(),
            Ok(()),
        )),
    );

    assert!(app.pending_marks.is_empty());
    let cm = &app.workspaces[&team].messages[&channel];
    assert_eq!(cm.last_read.as_deref(), Some(ts.as_str()));
    assert_eq!(cm.unread_count, 0);
    assert_eq!(cm.mention_count, 0);
    assert!(!app.activity.items[0].is_unread);
    assert_eq!(app.activity.items[0].unread_msg_count(), 0);
    assert_eq!(
        app.active_workspace().unwrap().activity_unread_count,
        Some(1)
    );
}

#[test]
fn activity_upsert_dedups_thread_by_identity() {
    use crate::slack::models::ActivityItem;

    let mk = |key: &str, latest: &str, feed_ts: &str| -> ActivityItem {
        serde_json::from_value(json!({
            "is_unread": true,
            "feed_ts": feed_ts,
            "key": key,
            "item": {
                "type": "thread_v2",
                "bundle_info": {"payload": {"thread_entry": {
                    "channel_id": "C1",
                    "thread_ts": "100.000",
                    "latest_ts": latest,
                    "unread_msg_count": 1
                }}}
            }
        }))
        .unwrap()
    };

    let mut state = ActivityState::default();
    state.upsert(mk("thread-C1-101", "101.000", "101.000"));
    state.upsert(mk("thread-C1-102", "102.000", "102.000"));

    assert_eq!(state.items.len(), 1, "same thread must collapse to one row");
    assert_eq!(state.items[0].latest_ts(), Some("102.000"));

    let mut other = mk("thread-C2-1", "5.0", "5.0");
    other.item.bundle_info = serde_json::from_value(json!({"payload": {"thread_entry": {
        "channel_id": "C2", "thread_ts": "9.0", "latest_ts": "9.0", "unread_msg_count": 1
    }}}))
    .unwrap();
    state.upsert(other);
    assert_eq!(state.items.len(), 2);
}

#[test]
fn selecting_thread_activity_routes_into_conversation_reducer() {
    let mut app = test_app();
    let item: ActivityItem = serde_json::from_value(json!({
        "is_unread": true,
        "feed_ts": "1783372400.000100",
        "key": "thread-C_GENERAL-1783372300",
        "item": {
            "type": "thread_v2",
            "bundle_info": {"payload": {"thread_entry": {
                "channel_id": "C_GENERAL",
                "thread_ts": "1783372300.000100",
                "latest_ts": "1783372400.000100",
                "min_unread_ts": "1783372350.000100",
                "unread_msg_count": 1
            }}}
        }
    }))
    .unwrap();
    let key = item.key.clone();
    app.activity.items.push(item);

    let _ = update(
        &mut app,
        Message::Runtime(crate::app::RuntimeMessage::ActivitySelected(key.clone())),
    );

    assert_eq!(app.activity.selected.as_deref(), Some(key.as_str()));
    assert_eq!(
        app.active_thread.as_ref(),
        Some(&("C_GENERAL".into(), "1783372300.000100".into()))
    );
    assert!(app.thread_open);
}

#[test]
fn older_activity_page_appends_items_and_advances_cursor() {
    let mut app = activity_app();
    let team = app.active_team.clone().unwrap();
    let existing = app.activity.items.len();
    app.activity.loading = true;
    app.activity.next_cursor = Some("page-2".into());
    let older: ActivityItem = serde_json::from_value(json!({
        "is_unread": false,
        "feed_ts": "1783199500.000100",
        "key": "older-mention",
        "item": {
            "type": "at_user",
            "message": {
                "ts": "1783199500.000100",
                "channel": "C_GENERAL",
                "author_user_id": "U_ALICE"
            }
        }
    }))
    .unwrap();
    let seq = app.activity.load_seq;

    let _ = update(
        &mut app,
        Message::Runtime(crate::app::RuntimeMessage::ActivityLoaded {
            team,
            cursor: Some("page-2".into()),
            seq,
            result: Ok(ActivityFeedPage {
                items: vec![older],
                response_metadata: Some(ResponseMetadata {
                    next_cursor: Some("page-3".into()),
                    ..Default::default()
                }),
                ..Default::default()
            }),
        }),
    );

    assert!(!app.activity.loading);
    assert_eq!(app.activity.items.len(), existing + 1);
    assert!(
        app.activity
            .items
            .iter()
            .any(|item| item.key == "older-mention")
    );
    assert_eq!(app.activity.next_cursor.as_deref(), Some("page-3"));
}

#[test]
fn profile_hover_waits_then_survives_pointer_transfer_to_card() {
    let mut app = test_app();
    let _ = update(
        &mut app,
        Message::Workspace(crate::app::WorkspaceMessage::ProfileHoverEntered {
            user: "U_ALICE".into(),
            key: "alice-name".into(),
        }),
    );
    let generation = app.profile_hover.as_ref().unwrap().generation;
    assert!(!app.profile_hover.as_ref().unwrap().visible);

    let expected = iced::Point::new(240.0, 160.0);
    let _ = update(
        &mut app,
        Message::Workspace(crate::app::WorkspaceMessage::CursorMoved(expected)),
    );

    let _ = update(
        &mut app,
        Message::Workspace(crate::app::WorkspaceMessage::ProfileHoverReady {
            user: "U_ALICE".into(),
            key: "alice-name".into(),
            generation,
        }),
    );
    assert!(app.profile_hover.as_ref().unwrap().visible);
    assert_eq!(app.profile_hover.as_ref().unwrap().position, Some(expected));

    let _ = update(
        &mut app,
        Message::Workspace(crate::app::WorkspaceMessage::ProfileHoverExited {
            user: "U_ALICE".into(),
            key: "alice-name".into(),
        }),
    );
    let dismiss_generation = app.profile_hover.as_ref().unwrap().generation;
    let _ = update(
        &mut app,
        Message::Workspace(crate::app::WorkspaceMessage::ProfileCardEntered),
    );
    let _ = update(
        &mut app,
        Message::Workspace(crate::app::WorkspaceMessage::ProfileHoverDismissReady(
            dismiss_generation,
        )),
    );
    assert!(app.profile_hover.is_some());
}

#[test]
fn profile_close_waits_for_panel_animation_before_clearing() {
    let mut app = profile_app();
    let _ = update(
        &mut app,
        Message::Workspace(crate::app::WorkspaceMessage::ProfilePressed(
            "U_ALICE".into(),
        )),
    );
    assert!(app.profile_open);
    assert!(app.profile_pane.is_some());

    let _ = update(
        &mut app,
        Message::Workspace(crate::app::WorkspaceMessage::ProfileDismissed),
    );
    assert!(!app.profile_open);
    assert!(app.profile_pane.is_some());

    let _ = update(
        &mut app,
        Message::Workspace(crate::app::WorkspaceMessage::ProfilePaneDismissed),
    );
    assert!(app.profile_pane.is_none());
}

#[test]
fn profile_result_merges_details_into_cached_user() {
    let mut app = test_app();
    let team = app.active_team.clone().unwrap();
    app.profile_pane = Some(ProfilePaneState {
        user: "U_ALICE".into(),
        loading: true,
        error: None,
    });
    let profile = UserProfile {
        display_name: Some("Alice".into()),
        title: Some("Product designer".into()),
        pronouns: Some("she/her".into()),
        email: Some("alice@example.com".into()),
        ..Default::default()
    };
    let _ = update(
        &mut app,
        Message::Workspace(crate::app::WorkspaceMessage::ProfileLoaded {
            team: team.clone(),
            user: "U_ALICE".into(),
            result: Ok(profile),
        }),
    );

    let alice = app.workspaces[&team].users["U_ALICE"]
        .profile
        .as_ref()
        .unwrap();
    assert_eq!(alice.title.as_deref(), Some("Product designer"));
    assert_eq!(alice.pronouns.as_deref(), Some("she/her"));
    assert!(!app.profile_pane.as_ref().unwrap().loading);
}

#[test]
fn profile_extras_result_updates_recent_conversations() {
    let mut app = test_app();
    let team = app.active_team.clone().unwrap();
    let _ = update(
        &mut app,
        Message::Workspace(crate::app::WorkspaceMessage::ProfileExtrasLoaded {
            team: team.clone(),
            user: "U_ALICE".into(),
            result: Ok(crate::slack::models::ProfileExtrasPage {
                im_mpim_ids: vec!["D_ALICE".into(), "G_TEAM".into()],
                has_more_mpims: true,
                ..Default::default()
            }),
        }),
    );
    let alice = &app.workspaces[&team].users["U_ALICE"];
    assert_eq!(alice.im_mpim_ids, ["D_ALICE", "G_TEAM"]);
    assert!(alice.has_more_mpims);
}

fn test_image_viewer_source() -> ImageViewerSource {
    ImageViewerSource {
        kind: MediaViewerKind::Image,
        preview_key: "F_IMAGE".into(),
        full_url: "https://files.slack.com/files-pri/T/F/launch.png".into(),
        download_url: "https://files.slack.com/files-pri/T/F/launch.png".into(),
        fetch_auth: ImageFetchAuth::Slack,
        filename: "launch.png".into(),
        author_name: "Alice".into(),
        avatar_key: Some("U_ALICE".into()),
        timestamp: "1783372300.000100".into(),
        conversation: "#general".into(),
    }
}

#[test]
fn image_viewer_open_zoom_close_and_dismiss_are_stateful() {
    let mut app = test_app();
    let _ = update(
        &mut app,
        Message::Discovery(crate::app::DiscoveryMessage::ImageViewerOpened(
            test_image_viewer_source(),
        )),
    );
    let generation = app.image_viewer.as_ref().unwrap().generation;
    assert!(app.image_viewer.as_ref().unwrap().open);
    assert_eq!(app.image_viewer.as_ref().unwrap().zoom, 1.0);

    let _ = update(
        &mut app,
        Message::Discovery(crate::app::DiscoveryMessage::ImageViewerZoomChanged(2.25)),
    );
    assert_eq!(app.image_viewer.as_ref().unwrap().zoom, 2.25);
    let _ = update(
        &mut app,
        Message::Discovery(crate::app::DiscoveryMessage::ImageViewerZoomChanged(99.0)),
    );
    assert_eq!(app.image_viewer.as_ref().unwrap().zoom, 5.0);
    let _ = update(
        &mut app,
        Message::Discovery(crate::app::DiscoveryMessage::ImageViewerZoomChanged(1.0)),
    );
    assert_eq!(
        app.image_viewer.as_ref().unwrap().offset,
        iced::Vector::ZERO
    );

    let _ = update(
        &mut app,
        Message::Discovery(crate::app::DiscoveryMessage::ImageViewerFullLoaded {
            generation: generation.wrapping_sub(1),
            result: Ok(vec![1, 2, 3]),
        }),
    );
    assert!(matches!(
        app.image_viewer.as_ref().unwrap().image,
        ImageViewerImage::Failed
    ));

    let _ = update(
        &mut app,
        Message::Discovery(crate::app::DiscoveryMessage::ImageViewerClosed),
    );
    assert!(!app.image_viewer.as_ref().unwrap().open);
    let _ = update(
        &mut app,
        Message::Discovery(crate::app::DiscoveryMessage::ImageViewerFullLoaded {
            generation,
            result: Ok(vec![1, 2, 3]),
        }),
    );
    assert!(matches!(
        app.image_viewer.as_ref().unwrap().image,
        ImageViewerImage::Failed
    ));
    let _ = update(
        &mut app,
        Message::Discovery(crate::app::DiscoveryMessage::ImageViewerDismissed),
    );
    assert!(app.image_viewer.is_none());
}

#[test]
fn image_viewer_transform_clamps_zoom_and_resets_fit_offset() {
    let mut app = test_app();
    let _ = update(
        &mut app,
        Message::Discovery(crate::app::DiscoveryMessage::ImageViewerOpened(
            test_image_viewer_source(),
        )),
    );
    let _ = update(
        &mut app,
        Message::Discovery(crate::app::DiscoveryMessage::ImageViewerTransformed {
            zoom: 3.0,
            offset: iced::Vector::new(24.0, -12.0),
        }),
    );
    let viewer = app.image_viewer.as_ref().unwrap();
    assert_eq!(viewer.zoom, 3.0);
    assert_eq!(viewer.offset, iced::Vector::new(24.0, -12.0));

    let _ = update(
        &mut app,
        Message::Discovery(crate::app::DiscoveryMessage::ImageViewerTransformed {
            zoom: 0.1,
            offset: iced::Vector::new(99.0, 99.0),
        }),
    );
    let viewer = app.image_viewer.as_ref().unwrap();
    assert_eq!(viewer.zoom, 1.0);
    assert_eq!(viewer.offset, iced::Vector::ZERO);
}

#[test]
fn thread_drop_zone_hugs_the_right_edge_behind_the_profile_pane() {
    let gap = crate::ui::theme::gap();
    let thread = crate::ui::theme::THREAD_WIDTH;
    let pane = crate::ui::profile::PANE_WIDTH;

    let zone = thread_panel_x_range(1400.0, false);
    assert_eq!(zone, (1400.0 - gap - thread)..(1400.0 - gap));
    assert!(zone.contains(&(1400.0 - gap - 1.0)));
    assert!(!zone.contains(&(1400.0 - gap - thread - 1.0)));

    let zone = thread_panel_x_range(1400.0, true);
    let end = 1400.0 - gap - pane - gap;
    assert_eq!(zone, (end - thread)..end);
}
