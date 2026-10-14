//! Parsed cursor frames must reconcile the rendered Activity rows and badges.
use super::{apply_frame, badge_dom, wait_dom};
use crate::state::ShellState;
use dioxus::prelude::*;
use serde_json::{Value, json};
use super_platinum_core::slack::models::{ActivityFeedPage, ActivityItem};

const CHANNEL: &str = "C_ACTIVITY_CURSOR";

fn mention(key: &str, ts: &str, root: Option<&str>) -> ActivityItem {
    serde_json::from_value(json!({"key":key,"is_unread":true,"feed_ts":ts,
        "item":{"type":"at_user","message":{"channel":CHANNEL,"ts":ts,
        "thread_ts":root,"author_user_id":"U1","text":key}}}))
    .unwrap()
}

fn bundle(key: &str, root: &str, min: &str, raw: bool) -> ActivityItem {
    serde_json::from_value(json!({"key":key,"is_unread":raw,"feed_ts":"40.0",
        "item":{"type":"thread_v2","bundle_info":{"payload":{"thread_entry":{
        "channel_id":CHANNEL,"thread_ts":root,"latest_ts":"40.0",
        "min_unread_ts":min,"unread_msg_count":8}}}}}))
    .unwrap()
}

fn thread_mark(root: &str, cursor: &str) -> Value {
    json!({"type":"thread_marked","subscription":{
        "channel":CHANNEL,"thread_ts":root,"last_read":cursor}})
}

pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<(), String> {
    let saved = state.peek().core.activity.clone();
    let items = vec![
        mention("cursor-old", "10.0", None),
        mention("cursor-new", "30.0", None),
        mention("cursor-threaded", "15.0", Some("5.0")),
        bundle("cursor-bundle", "5.0", "15.0", true),
        bundle("cursor-other-root", "6.0", "12.0", true),
        // A separate raw-false bundle
        // must clear its effective unread state without spending a badge unit.
        bundle("cursor-raw-false", "7.0", "12.0", false),
    ];
    let request = {
        let mut shell = state.write();
        shell.core.activity.items = items.clone();
        shell.core.activity.selected = None;
        shell
            .core
            .activity
            .begin(false)
            .ok_or("Cursor fixture request missing")?
    };
    apply_frame(
        &mut state,
        json!({"type":"badge_counts_updated",
        "activity_v2":{"at_user":10,"thread_v2":10}}),
    )?;
    badge_dom(desktop, "20", "46").await?;
    apply_frame(
        &mut state,
        json!({"type":"channel_marked","channel":CHANNEL,"ts":"20.0"}),
    )?;
    badge_dom(desktop, "19", "46").await?;
    rows(
        desktop,
        &["cursor-old"],
        &["cursor-new", "cursor-threaded", "cursor-bundle"],
    )
    .await?;
    apply_frame(&mut state, thread_mark("5.0", "20.0"))?;
    badge_dom(desktop, "17", "46").await?;
    apply_frame(&mut state, thread_mark("7.0", "20.0"))?;
    badge_dom(desktop, "17", "46").await?;
    let mut old_page = items;
    old_page.push(mention("cursor-unseen-page", "9.0", None));
    {
        let mut shell = state.write();
        if !shell.core.activity.apply_page(
            &request,
            ActivityFeedPage {
                items: old_page,
                ..Default::default()
            },
        ) {
            return Err("Cursor fixture old page rejected unexpectedly".into());
        }
    }
    rows(
        desktop,
        &[
            "cursor-old",
            "cursor-threaded",
            "cursor-bundle",
            "cursor-raw-false",
            "cursor-unseen-page",
        ],
        &["cursor-new", "cursor-other-root"],
    )
    .await?;
    // A new bundle beyond the cursor remains unread through duplicate and
    // out-of-order marks; neither mark spends another category count.
    state
        .write()
        .core
        .activity
        .upsert(bundle("cursor-bundle", "5.0", "25.0", true));
    apply_frame(&mut state, thread_mark("5.0", "20.0"))?;
    apply_frame(&mut state, thread_mark("5.0", "10.0"))?;
    badge_dom(desktop, "17", "46").await?;
    rows(
        desktop,
        &["cursor-old"],
        &["cursor-new", "cursor-bundle", "cursor-other-root"],
    )
    .await?;
    {
        let mut shell = state.write();
        let generation = shell.core.workspaces["T1"].rt_generation;
        let event = super_platinum_core::slack::realtime::parse_event(
            &thread_mark("6.0", "99.0").to_string(),
        )
        .unwrap();
        crate::realtime::apply_in_view(
            &mut shell.core,
            "T1",
            generation.wrapping_add(1),
            event,
            false,
        );
    }
    rows(desktop, &["cursor-old"], &["cursor-other-root"]).await?;
    // Restore the existing navigation fixture; this check never owns a Slack
    // transport or persists session/draft data.
    {
        let mut shell = state.write();
        shell.core.activity = saved;
        shell
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .messages
            .remove(CHANNEL);
        shell
            .core
            .threads
            .retain(|(_, channel, _), _| channel != CHANNEL);
    }
    apply_frame(
        &mut state,
        json!({"type":"badge_counts_updated","activity_v2":{}}),
    )?;
    badge_dom(desktop, "", "46").await?;
    Ok(())
}

async fn rows(
    desktop: &dioxus::desktop::DesktopContext,
    read: &[&str],
    unread: &[&str],
) -> Result<(), String> {
    let read = serde_json::to_string(read).unwrap();
    let unread = serde_json::to_string(unread).unwrap();
    wait_dom(desktop, &format!(r#"(()=>{{const read={read},unread={unread};const row=k=>document.querySelector('[data-activity-key="'+k+'"]');return {{ok:read.every(k=>row(k)&&!row(k).classList.contains('unread'))&&unread.every(k=>row(k)?.classList.contains('unread'))}};}})()"#)).await.map(|_| ())
}
