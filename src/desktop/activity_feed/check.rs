//! Disconnected native Activity clicks, including an older reply in a busy thread.
use crate::slack_links_check::{wait_dom, wait_state};
use crate::state::{MainView, ShellState};
use dioxus::prelude::*;
use serde_json::{Value, json};

const ROOT: &str = "1719800000.000100";
const REPLY: &str = "1719800001.000001";

pub(super) fn install(shell: &mut ShellState) {
    // Reuse the disconnected 32-reply transcript, with the target far from
    // latest. All session values are empty fixture values; no transport exists.
    crate::slack_links_check::install(shell);
    shell.main_view = MainView::Activity;
    shell.core.activity.items.clear();
    shell.core.activity.unread_only = false;
    shell.core.activity.next_cursor = None;
    for value in [
        json!({"key":"activity-exact-reply","is_unread":true,"feed_ts":"1719801500.000800",
            "item":{"type":"mention","message":{"channel":"C2","ts":REPLY,"thread_ts":ROOT,"author_user_id":"U1","text":"Review this earlier reply"}}}),
        json!({"key":"activity-thread-bundle","is_unread":true,"feed_ts":"1719801400.000700",
            "item":{"type":"thread_v2","bundle_info":{"payload":{"thread_entry":{"channel_id":"C2","thread_ts":ROOT,"latest_ts":"1719800032.000001","unread_msg_count":4}}}}}),
        json!({"key":"activity-channel-post","is_unread":true,"feed_ts":"1719801300.000600",
            "item":{"type":"mention","message":{"channel":"C1","ts":ROOT,"author_user_id":"U1","text":"A channel notification"}}}),
    ] {
        shell
            .core
            .activity
            .upsert(serde_json::from_value(value).unwrap());
    }
    shell.refresh_from_core();
}

pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if std::env::var("SUPER_PLATINUM_ACTIVITY_CHECK").as_deref() != Ok("1")
        || !matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("activity-navigation" | "activity-navigation-narrow")
        )
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let result = run(state, &desktop)
            .await
            .unwrap_or_else(|error| json!({"ok":false,"error":error}));
        let generation = state.peek().thread_generation;
        eprintln!("activity-navigation-check: {result}");
        if result["ok"] == true {
            if let Err(error) = agent_route(state, &desktop, generation).await {
                eprintln!(
                    "activity-agent-check: {}",
                    json!({"ok":false,"error":error})
                );
            }
        }
    });
}

async fn run(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    if state.peek().core.transport.is_some() {
        return Err("Activity check requires no transport".into());
    }
    click(desktop, "activity-thread-bundle").await?;
    wait_state(state, |s| {
        s.main_view == MainView::Activity
            && s.thread_root.as_deref() == Some(ROOT)
            && s.thread_at_bottom
    })
    .await?;
    let bundle_geometry = latest_geometry(desktop).await?;
    click(desktop, "activity-channel-post").await?;
    wait_state(state, |s| {
        s.main_view == MainView::Activity
            && s.thread_root.is_none()
            && s.core.active_channel.as_deref() == Some("C1")
    })
    .await?;
    click(desktop, "activity-exact-reply").await?;
    wait_state(state, |s| {
        s.main_view == MainView::Activity
            && s.thread_root.as_deref() == Some(ROOT)
            && !s.thread_at_bottom
            && s.core.active_channel.as_deref() == Some("C2")
    })
    .await?;
    let mut receipt = reply_geometry(desktop).await?;
    if state.peek().thread_at_bottom || state.peek().core.transport.is_some() {
        return Err("Exact Activity reply lost its non-latest position".into());
    }
    receipt["bundleOpensLatest"] = json!(true);
    receipt["bundleGeometry"] = bundle_geometry;
    receipt["channelClosesThread"] = json!(true);
    receipt["exactReplyVisible"] = json!(true);
    receipt["activitySurfacePreserved"] = json!(true);
    receipt["noTransport"] = json!(true);
    Ok(receipt)
}

async fn reply_geometry(desktop: &dioxus::desktop::DesktopContext) -> Result<Value, String> {
    wait_dom(desktop,&format!(r#"(()=>{{const panel=document.getElementById('thread-body'),clip=panel?.getBoundingClientRect(),reply=panel?.querySelector('[data-message-id="{REPLY}"]'),r=reply?.getBoundingClientRect();return {{ok:!!r&&r.width>0&&r.bottom>Math.max(0,clip.top)&&r.top<Math.min(innerHeight,clip.bottom)&&document.documentElement.scrollWidth<=innerWidth+1,replyVisible:!!r,replyId:reply?.dataset.messageId,viewport:[innerWidth,innerHeight]}};}})()"#)).await
}

async fn latest_geometry(desktop: &dioxus::desktop::DesktopContext) -> Result<Value, String> {
    wait_dom(desktop,r#"(()=>{const panel=document.getElementById('thread-body'),clip=panel?.getBoundingClientRect(),reply=panel?.querySelector('[data-message-id="1719800032.000001"]'),r=reply?.getBoundingClientRect(),gap=panel?panel.scrollHeight-panel.clientHeight-panel.scrollTop:null;return {ok:!!r&&r.width>0&&r.bottom>Math.max(0,clip.top)&&r.top<Math.min(innerHeight,clip.bottom)&&gap<=2&&gap>=-2,latestVisible:!!r,bottomGap:gap};})()"#).await
}

// The external harness drives the real agent command after the DOM-click pass.
// Emit a receipt for each step so it cannot queue the next step before this
// observer has measured the rendered destination. No target persistence needed:
// a successful scroll consumes that one-shot state.
async fn agent_route(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    generation: u64,
) -> Result<(), String> {
    wait_state(state, |s| {
        s.thread_generation != generation
            && s.main_view == MainView::Activity
            && s.thread_root.as_deref() == Some(ROOT)
            && s.thread_at_bottom
    })
    .await?;
    let geometry = latest_geometry(desktop).await?;
    let generation = state.peek().thread_generation;
    eprintln!(
        "activity-agent-check: {}",
        json!({"ok":true,"stage":"bundle","geometry":geometry})
    );
    wait_state(state, |s| {
        s.thread_generation != generation
            && s.main_view == MainView::Activity
            && s.thread_root.as_deref() == Some(ROOT)
            && s.core.active_channel.as_deref() == Some("C2")
            && !s.thread_at_bottom
    })
    .await?;
    let geometry = reply_geometry(desktop).await?;
    if state.peek().core.transport.is_some() {
        return Err("Agent Activity check unexpectedly acquired transport".into());
    }
    eprintln!(
        "activity-agent-check: {}",
        json!({"ok":true,"stage":"reply","geometry":geometry,"noTransport":true})
    );
    Ok(())
}

async fn click(desktop: &dioxus::desktop::DesktopContext, key: &str) -> Result<(), String> {
    wait_dom(desktop,&format!(r#"(()=>{{const row=document.querySelector('[data-activity-key="{key}"]');if(!row)return {{ok:false}};row.scrollIntoView({{block:'nearest'}});const r=row.getBoundingClientRect();if(!r.width||!r.height)return {{ok:false}};row.click();return {{ok:true}};}})()"#)).await.map(|_|())
}
