use super::*;
use crate::bootstrap::test_support::{harness, pending};
use serde_json::json;
use std::{cell::Cell, rc::Rc};
fn info(id: &str) -> serde_json::Value {
    json!({"ok":true,"channel":{"id":id,"is_im":true,"is_open":false,"user":"U2","unread_count":0,"has_unreads":false}})
}
fn setup(mut state: Signal<ShellState>, ids: &[&str]) {
    let mut s = state.write();
    let old = s.core.active_channel.clone().unwrap();
    s.surfaces.insert(
        MainView::Home,
        crate::state::SurfaceTarget {
            channel: old,
            thread_root: None,
        },
    );
    s.main_view = MainView::Dms;
    s.overlay = None;
    s.set_dm_query(String::new());
    s.core.dms.entries=ids.iter().map(|id|serde_json::from_value(json!({"id":id,"latest":"9.1","channel":{"id":id,"is_im":true,"user":"U2","is_open":false}})).unwrap()).collect();
    s.core.dms.loaded = true;
}
fn source(state: Signal<ShellState>, id: &str) -> Source {
    Source::for_row(&Frame::capture(&state.peek()).unwrap(), id).unwrap()
}
fn begin(mut state: Signal<ShellState>, source: Source) -> Arc<Opening> {
    source.begin(&mut state.write()).unwrap()
}
#[tokio::test]
async fn cached_canonical_but_unprojected_dm_opens_in_its_surface_with_draft_and_anchor_preserved()
{
    let h = harness();
    let mut state = h.state;
    setup(state, &["D_CACHED"]);
    let old = state.peek().core.active_channel.clone().unwrap();
    state
        .write()
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .channels
        .insert(
            "D_CACHED".into(),
            serde_json::from_value(info("D_CACHED")["channel"].clone()).unwrap(),
        );
    assert!(!state.peek().channels.iter().any(|c| c.id == "D_CACHED"));
    let before = serde_json::to_value(&state.peek().core.workspaces["T1"].channels).unwrap();
    let opening = begin(state, source(state, "D_CACHED"));
    let refreshes = Rc::new(Cell::new(0));
    let called = refreshes.clone();
    assert!(
        navigate_with(
            state,
            opening,
            |_| async { panic!("canonical DM must not fetch info") },
            move || async move { called.set(called.get() + 1) }
        )
        .await
    );
    assert_eq!(refreshes.get(), 1);
    assert_eq!(state.peek().main_view, MainView::Dms);
    assert!(state.peek().detail_open());
    assert_eq!(
        state.peek().core.active_channel.as_deref(),
        Some("D_CACHED")
    );
    assert_eq!(state.peek().surfaces[&MainView::Home].channel, old);
    assert!(
        matches!(state.peek().core.pending_scroll_to.as_ref(),Some((id,super_platinum_core::domain::PendingScrollTarget::Latest)) if id=="D_CACHED")
    );
    assert_eq!(
        serde_json::to_value(&state.peek().core.workspaces["T1"].channels).unwrap(),
        before
    );
    assert!(state.peek().dm_open.is_none());
    let index = state
        .peek()
        .channels
        .iter()
        .position(|c| c.id == old)
        .unwrap();
    state.write().select_channel(index, ChannelOpen::Global);
    assert_eq!(state.peek().core.composer.text, "Keep draft");
}
#[tokio::test]
async fn unknown_closed_dm_uses_actual_info_read_then_selects_without_sparse_membership_or_open_writes()
 {
    let h = harness();
    let state = h.state;
    setup(state, &["D_COLD"]);
    let opening = begin(state, source(state, "D_COLD"));
    let (release, held) = tokio::sync::oneshot::channel();
    let calls = Rc::new(Cell::new(0));
    let refreshed = calls.clone();
    let mut work = Box::pin(navigate_with(
        state,
        opening,
        |request| async move {
            assert!(request.url.contains("conversations.info"));
            assert!(!request.url.contains("conversations.open"));
            let super_platinum_core::slack::client::RequestBody::Form(fields) = request.body else {
                panic!("form")
            };
            let f = fields
                .into_iter()
                .collect::<std::collections::BTreeMap<_, _>>();
            assert_eq!(f["channel"], "D_COLD");
            held.await.unwrap();
            Ok(info("D_COLD"))
        },
        move || async move { refreshed.set(refreshed.get() + 1) },
    ));
    pending(work.as_mut()).await;
    assert!(
        !state.peek().core.workspaces["T1"]
            .channels
            .contains_key("D_COLD")
    );
    release.send(()).unwrap();
    assert!(work.await);
    assert_eq!(calls.get(), 1);
    let s = state.peek();
    let channel = &s.core.workspaces["T1"].channels["D_COLD"];
    assert!(channel.is_im);
    assert_eq!(channel.extra["is_open"], false);
    assert!(!channel.extra.contains_key("is_member"));
    assert_eq!(s.main_view, MainView::Dms);
    assert_eq!(s.core.active_channel.as_deref(), Some("D_COLD"));
    assert!(s.dm_open.is_none());
}
#[tokio::test]
async fn info_failure_wrong_identity_and_type_leave_rows_retryable_until_the_exact_dm_succeeds() {
    let h = harness();
    let state = h.state;
    setup(state, &["D_RETRY"]);
    let row = source(state, "D_RETRY");
    let responses = [
        Err(Error::Transport(
            "https://example.test/?token=HIDDEN".into(),
        )),
        Ok(info("D_WRONG")),
        Ok(json!({"ok":true,"channel":{"id":"D_RETRY","is_channel":true}})),
    ];
    for result in responses {
        let opening = begin(state, row.clone());
        assert!(
            !navigate_with(
                state,
                opening,
                |_| async { result },
                || async { panic!("failed info must not refresh another conversation") }
            )
            .await
        );
        assert!(state.peek().dm_open.is_none());
        assert!(
            !state.peek().core.workspaces["T1"]
                .channels
                .contains_key("D_RETRY")
        );
        assert_eq!(state.peek().core.dms.entries[0].id, "D_RETRY");
        assert!(
            state
                .peek()
                .toast
                .as_ref()
                .is_none_or(|t| !t.text.contains("HIDDEN"))
        );
    }
    let opening = begin(state, row);
    assert!(
        navigate_with(
            state,
            opening,
            |_| async { Ok(info("D_RETRY")) },
            || async {}
        )
        .await
    );
    assert_eq!(state.peek().core.active_channel.as_deref(), Some("D_RETRY"));
}
#[tokio::test]
async fn query_change_and_return_permanently_cancels_the_original_pending_open() {
    let h = harness();
    let mut state = h.state;
    setup(state, &["D_OLD"]);
    let opening = begin(state, source(state, "D_OLD"));
    let (release, held) = tokio::sync::oneshot::channel();
    let mut work = Box::pin(navigate_with(
        state,
        opening,
        |_| async move {
            held.await.unwrap();
            Ok(info("D_OLD"))
        },
        || async { panic!("retired refresh") },
    ));
    pending(work.as_mut()).await;
    state.write().set_dm_query("other".into());
    state.write().set_dm_query(String::new());
    assert!(!work.await);
    assert!(release.send(()).is_err());
    assert!(state.peek().dm_open.is_none());
    assert!(
        !state.peek().core.workspaces["T1"]
            .channels
            .contains_key("D_OLD")
    );
    let rendered = source(state, "D_OLD");
    state.write().core.session.as_mut().unwrap().d_cookie = "replaced-cookie".into();
    assert!(
        rendered.begin(&mut state.write()).is_none(),
        "old rendered row cannot reacquire replacement credentials"
    );
}
#[tokio::test]
async fn competing_open_keeps_the_newer_row_busy_and_only_its_result_can_select() {
    let h = harness();
    let mut state = h.state;
    setup(state, &["D_A", "D_B"]);
    let frame = Frame::capture(&state.peek()).unwrap();
    let a = Source::for_row(&frame, "D_A").unwrap();
    let b = Source::for_row(&frame, "D_B").unwrap();
    let (release_a, held_a) = tokio::sync::oneshot::channel();
    let first = begin(state, a);
    let mut old = Box::pin(navigate_with(
        state,
        first,
        |_| async move {
            held_a.await.unwrap();
            Ok(info("D_A"))
        },
        || async { panic!("old refresh") },
    ));
    pending(old.as_mut()).await;
    let (release_b, held_b) = tokio::sync::oneshot::channel();
    let second = begin(state, b);
    let mut fresh = Box::pin(navigate_with(
        state,
        second,
        |_| async move {
            held_b.await.unwrap();
            Ok(info("D_B"))
        },
        || async {},
    ));
    pending(fresh.as_mut()).await;
    release_a.send(()).unwrap();
    assert!(!old.await);
    assert!(is_opening(&state.peek(), "D_B"));
    assert!(
        !state.peek().core.workspaces["T1"]
            .channels
            .contains_key("D_A")
    );
    release_b.send(()).unwrap();
    assert!(fresh.await);
    assert_eq!(state.peek().core.active_channel.as_deref(), Some("D_B"));
    assert!(state.peek().dm_open.is_none());
}

#[tokio::test]
async fn held_info_respects_intervening_lifecycle_fields_or_removal_before_selection() {
    for removed in [false, true] {
        let h = harness();
        let mut state = h.state;
        setup(state, &["D_LIFECYCLE"]);
        let opening = begin(state, source(state, "D_LIFECYCLE"));
        let (release, held) = tokio::sync::oneshot::channel();
        let mut work = Box::pin(navigate_with(
            state,
            opening,
            |_| async move {
                held.await.unwrap();
                let mut value = info("D_LIFECYCLE");
                value["channel"]["is_member"] = true.into();
                Ok(value)
            },
            || async {},
        ));
        pending(work.as_mut()).await;
        {
            let mut shell = state.write();
            let ws = shell.core.workspaces.get_mut("T1").unwrap();
            let mut channel: Channel =
                serde_json::from_value(info("D_LIFECYCLE")["channel"].clone()).unwrap();
            channel.last_read = Some("8.0".into());
            ws.channels.insert(channel.id.clone(), channel);
            let since = ws.channel_lifecycle.revision();
            ws.confirm_channel_membership("D_LIFECYCLE", false, since);
            if removed {
                ws.channels.remove("D_LIFECYCLE");
            }
        }
        release.send(()).unwrap();
        assert_eq!(work.await, !removed);
        let shell = state.peek();
        if removed {
            assert!(
                !shell.core.workspaces["T1"]
                    .channels
                    .contains_key("D_LIFECYCLE")
            );
            assert_ne!(shell.core.active_channel.as_deref(), Some("D_LIFECYCLE"));
        } else {
            let c = &shell.core.workspaces["T1"].channels["D_LIFECYCLE"];
            assert_eq!(c.extra["is_member"], false);
            assert_eq!(c.extra["is_open"], false);
            assert_eq!(c.last_read.as_deref(), Some("8.0"));
            assert_eq!(c.unread_count, Some(0));
        }
    }
}

#[tokio::test]
async fn accepted_metadata_that_clears_feed_unreads_still_completes_the_clicked_row() {
    let h = harness();
    let mut state = h.state;
    setup(state, &["D_UNREAD"]);
    {
        let mut s = state.write();
        s.core.dms.entries[0].channel.as_mut().unwrap().unread_count = Some(3);
        s.set_dm_unread_only(true);
        assert!(s.dm_row_visible(&s.core.dms.entries[0]));
    }
    let opening = begin(state, source(state, "D_UNREAD"));
    assert!(
        navigate_with(
            state,
            opening,
            |_| async { Ok(info("D_UNREAD")) },
            || async {}
        )
        .await
    );
    let s = state.peek();
    assert_eq!(s.main_view, MainView::Dms);
    assert_eq!(s.core.active_channel.as_deref(), Some("D_UNREAD"));
    assert!(!s.dm_row_visible(&s.core.dms.entries[0]));
    assert!(s.dm_open.is_none());
}
#[tokio::test]
async fn accepted_canonical_metadata_replaces_stale_projected_unread_state_before_read_opening() {
    let h = harness();
    let mut state = h.state;
    setup(state, &["D_STALE"]);
    {
        let mut s = state.write();
        let mut row = s.channels[0].clone();
        row.id = "D_STALE".into();
        row.unread = false;
        row.unread_count = 0;
        s.channels.push(row);
        // Counts/history are authoritative even while channel metadata is cold.
        // Metadata insertion may establish only a read-revision bag; info counts
        // must not replace an existing bag's unread state or cursor.
        let bag = s
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .messages
            .entry("D_STALE".into())
            .or_default();
        bag.unread_count = 3;
        bag.last_read = Some("8.0".into());
        s.core.workspaces.get_mut("T1").unwrap().read_preferences =
            super_platinum_core::state::ReadPreferences::from_prefs(
                &super_platinum_core::state::ReadMode::LeftOffAndRead.patch(),
            );
    }
    let opening = begin(state, source(state, "D_STALE"));
    assert!(
        navigate_with(
            state,
            opening,
            |_| async {
                let mut v = info("D_STALE");
                v["channel"]["unread_count"] = 0.into();
                v["channel"]["has_unreads"] = false.into();
                v["channel"]["last_read"] = "9.0".into();
                Ok(v)
            },
            || async {}
        )
        .await
    );
    let s = state.peek();
    let row = s.channels.iter().find(|c| c.id == "D_STALE").unwrap();
    assert!(row.unread);
    assert_eq!(row.unread_count, 3);
    let workspace = &s.core.workspaces["T1"];
    assert_eq!(workspace.channels["D_STALE"].unread_count, Some(0));
    assert_eq!(workspace.messages["D_STALE"].unread_count, 3);
    assert_eq!(workspace.read_cursor("D_STALE"), Some("8.0"));
    assert_eq!(
        s.core.pending_scroll_to,
        Some((
            "D_STALE".into(),
            super_platinum_core::domain::PendingScrollTarget::FirstUnreadAfter("8.0".into())
        ))
    );
}

#[tokio::test]
async fn cold_group_dm_uses_affirmative_mpim_metadata_and_stays_beside_its_list() {
    let h = harness();
    let state = h.state;
    setup(state, &["G_COLD"]);
    let opening = begin(state, source(state, "G_COLD"));
    assert!(navigate_with(state,opening,|_|async{Ok(json!({"ok":true,"channel":{"id":"G_COLD","is_mpim":true,"is_group":true,"is_open":false,"name":"Release group"}}))},||async{}).await);
    let s = state.peek();
    assert_eq!(s.main_view, MainView::Dms);
    assert_eq!(s.core.active_channel.as_deref(), Some("G_COLD"));
    assert!(s.core.workspaces["T1"].channels["G_COLD"].is_mpim);
    assert_eq!(
        s.core.workspaces["T1"].channels["G_COLD"].extra["is_open"],
        false
    );
    assert_eq!(
        s.channels.iter().find(|c| c.id == "G_COLD").unwrap().name,
        "Release group"
    );
}

#[tokio::test]
async fn filtered_older_page_then_cold_dm_metadata_read_completes_an_in_surface_conversation() {
    let h = harness();
    let mut state = h.state;
    setup(state, &["D_KEEP"]);
    let old = state.peek().core.active_channel.clone().unwrap();
    {
        let mut s = state.write();
        s.set_dm_query("Older Teammate".into());
        s.core.dms.next_cursor = Some("older-page".into());
        s.core.workspaces.get_mut("T1").unwrap().users.insert(
            "U_OLDER".into(),
            serde_json::from_value(
                json!({"id":"U_OLDER","profile":{"display_name":"Older Teammate"}}),
            )
            .unwrap(),
        );
        assert!(!s.core.dms.entries.iter().any(|e| s.dm_row_visible(e)));
    }
    let frame = Frame::capture(&state.peek()).unwrap();
    let page = crate::bootstrap::dms::PageSource::capture(&state.peek(), &frame).unwrap();
    assert_eq!(crate::bootstrap::dms::paging::load_if_with(state,page,async{true},|request|async move{
        assert!(request.url.contains("client.dms"));let super_platinum_core::slack::client::RequestBody::Form(fields)=request.body else{panic!("form")};assert!(fields.iter().any(|(k,v)|k=="cursor"&&v=="older-page"));
        Ok(json!({"ok":true,"dms":[{"id":"D_OLDER","latest":"10.1","channel":{"id":"D_OLDER","is_im":true,"user":"U_OLDER","is_open":false}}],"response_metadata":{"next_cursor":""}}))
    }).await,crate::bootstrap::dms::feed::Outcome::Published);
    {
        let s = state.peek();
        assert_eq!(
            s.core
                .dms
                .entries
                .iter()
                .filter(|e| s.dm_row_visible(e))
                .map(|e| e.id.as_str())
                .collect::<Vec<_>>(),
            ["D_OLDER"]
        );
        assert!(s.core.dms.entries.iter().any(|e| e.id == "D_KEEP"));
        assert_eq!(s.core.active_channel.as_deref(), Some(old.as_str()));
        assert_eq!(s.core.composer.text, "Keep draft");
        assert!(!s.core.workspaces["T1"].channels.contains_key("D_OLDER"));
    }
    let opening = begin(state, source(state, "D_OLDER"));
    assert!(navigate_with(state,opening,|request|async move{assert!(request.url.contains("conversations.info"));Ok(json!({"ok":true,"channel":{"id":"D_OLDER","is_im":true,"is_open":false,"user":"U_OLDER"}}))},||async{}).await);
    let s = state.peek();
    assert_eq!(s.main_view, MainView::Dms);
    assert_eq!(s.core.active_channel.as_deref(), Some("D_OLDER"));
    assert_eq!(s.surfaces[&MainView::Home].channel, old);
    assert!(
        !s.core.workspaces["T1"].channels["D_OLDER"]
            .extra
            .contains_key("is_member")
    );
    drop(s);
    let index = state
        .peek()
        .channels
        .iter()
        .position(|c| c.id == old)
        .unwrap();
    state.write().select_channel(index, ChannelOpen::Global);
    assert_eq!(state.peek().core.composer.text, "Keep draft");
}

#[tokio::test]
async fn cold_unread_dm_row_remains_eligible_and_opens_at_its_bag_cursor_through_actual_info_read()
{
    let h = harness();
    let mut state = h.state;
    setup(state, &["D_COLD_UNREAD"]);
    {
        let mut s = state.write();
        s.set_dm_unread_only(true);
        s.core.dms.entries[0].channel.as_mut().unwrap().unread_count = Some(0);
        let ws = s.core.workspaces.get_mut("T1").unwrap();
        let bag = ws.messages.entry("D_COLD_UNREAD".into()).or_default();
        bag.unread_count = 3;
        bag.last_read = Some("8.0".into());
        ws.read_preferences = super_platinum_core::state::ReadPreferences::from_prefs(
            &super_platinum_core::state::ReadMode::LeftOffAndRead.patch(),
        );
        assert!(s.dm_row_visible(&s.core.dms.entries[0]));
    }
    assert!(
        !state.peek().core.workspaces["T1"]
            .channels
            .contains_key("D_COLD_UNREAD")
    );
    let opening = begin(state, source(state, "D_COLD_UNREAD"));
    assert!(
        navigate_with(
            state,
            opening,
            |request| async move {
                assert!(request.url.contains("conversations.info"));
                let mut v = info("D_COLD_UNREAD");
                v["channel"]["unread_count"] = 0.into();
                v["channel"]["has_unreads"] = false.into();
                v["channel"]["last_read"] = "9.0".into();
                Ok(v)
            },
            || async {}
        )
        .await
    );
    let s = state.peek();
    assert_eq!(s.main_view, MainView::Dms);
    assert_eq!(s.core.active_channel.as_deref(), Some("D_COLD_UNREAD"));
    assert_eq!(
        s.core.workspaces["T1"].messages["D_COLD_UNREAD"].unread_count,
        3
    );
    assert_eq!(
        s.core.workspaces["T1"].read_cursor("D_COLD_UNREAD"),
        Some("8.0")
    );
    assert_eq!(
        s.core.pending_scroll_to,
        Some((
            "D_COLD_UNREAD".into(),
            super_platinum_core::domain::PendingScrollTarget::FirstUnreadAfter("8.0".into())
        ))
    );
    assert_eq!(
        s.core.workspaces["T1"].channels["D_COLD_UNREAD"].extra["is_open"],
        false
    );
    assert!(
        !s.core.workspaces["T1"].channels["D_COLD_UNREAD"]
            .extra
            .contains_key("is_member")
    );
}
