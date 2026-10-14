//! Disconnected native Activity clicks, including an older reply in a busy thread.
use crate::slack_links_check::{wait_dom, wait_state};
use crate::state::{MainView, ShellState};
use dioxus::prelude::*;
use serde_json::{Value, json};

mod cursors;

const ROOT: &str = "1719800000.000100";
const REPLY: &str = "1719800001.000001";

pub(super) fn install(shell: &mut ShellState) {
    // Reuse the disconnected 32-reply transcript, with the target far from
    // latest. All session values are empty fixture values; no transport exists.
    crate::slack_links_check::install(shell);
    shell.main_view = MainView::Activity;
    shell
        .core
        .activity
        .ensure_scope(shell.core.account_epoch, "T1");
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
    let workspace = shell.core.workspaces.get_mut("T1").unwrap();
    // Six DM conversations contribute 46 highlights, independently of the
    // Activity category dm:4 used below (and not simply six conversations).
    for channel in workspace
        .channels
        .values_mut()
        .filter(|c| c.is_im || c.is_mpim)
    {
        channel.has_unreads = false;
        channel.unread_count = Some(0);
        channel.mention_count = Some(0);
        if let Some(messages) = workspace.messages.get_mut(&channel.id) {
            messages.unread_count = 0;
            messages.mention_count = 0;
        }
    }
    for (index, highlights) in [39, 3, 1, 1, 1, 1].into_iter().enumerate() {
        let id = format!("DFIXTURE{index}");
        workspace.channels.insert(
            id.clone(),
            super_platinum_core::slack::models::Channel {
                id,
                is_im: true,
                has_unreads: true,
                unread_count: Some(highlights),
                mention_count: Some(highlights),
                ..Default::default()
            },
        );
        let bag = workspace
            .messages
            .entry(format!("DFIXTURE{index}"))
            .or_default();
        bag.unread_count = highlights;
        bag.mention_count = highlights;
    }
    // Opposing high counts make muted/app exclusions observable in the DOM:
    // neither can be accidentally included while still rendering 46.
    for (id, user, highlights) in [
        ("DFIXTURE_MUTED", "UFIXTURE_MUTED", 901),
        ("DFIXTURE_BOT", "UFIXTURE_BOT", 902),
    ] {
        workspace.channels.insert(
            id.into(),
            super_platinum_core::slack::models::Channel {
                id: id.into(),
                user: Some(user.into()),
                is_im: true,
                has_unreads: true,
                unread_count: Some(highlights),
                mention_count: Some(highlights),
                ..Default::default()
            },
        );
        let bag = workspace.messages.entry(id.into()).or_default();
        bag.unread_count = highlights;
        bag.mention_count = highlights;
    }
    workspace.sidebar.muted.insert("DFIXTURE_MUTED".into());
    workspace.users.insert(
        "UFIXTURE_BOT".into(),
        super_platinum_core::slack::models::User {
            id: "UFIXTURE_BOT".into(),
            is_bot: true,
            ..Default::default()
        },
    );
    set_policy(workspace, false);
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
    badge_updates(state, desktop).await?;
    cursors::run(state, desktop).await?;
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
    receipt["canonicalBadgeUpdates"] = json!(true);
    receipt["zeroClearsUnreadRows"] = json!(true);
    receipt["cursorBoundedReads"] = json!(true);
    receipt["cursorPageReconciliation"] = json!(true);
    for flag in [
        "filteredCategories",
        "dmHighlightCount",
        "dmExcludedChannels",
        "optOutConservation",
        "guardedCategoryFlags",
        "unknownMapNoFallback",
    ] {
        receipt[flag] = json!(true);
    }
    Ok(receipt)
}

/// Exercise decoded socket frames through the production reducer and real DOM.
/// Fixture rows are deliberately fewer than the authoritative category total.
async fn badge_updates(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<(), String> {
    let categories = json!({"at_user":5,"dm":4,"thread_v2":18,"message_reaction":9,"future_unknown":11,"list_todo_notification":2,"list_approval_request":3,"unjoined_channel_mention":4,"team_joiner_invite":5});
    apply_frame(
        &mut state,
        json!({"type":"badge_counts_updated","activity_v2":categories}),
    )?;
    badge_dom(desktop, "27", "46").await?;
    {
        let shell = state.peek();
        let w = &shell.core.workspaces["T1"];
        let conversations = w
            .channels
            .values()
            .filter(|c| (c.is_im || c.is_mpim) && !c.is_archived && w.unread_total(c) > 0)
            .count();
        if conversations != 8
            || w.messages["DFIXTURE_MUTED"].mention_count != 901
            || w.messages["DFIXTURE_BOT"].mention_count != 902
            || !w.sidebar.is_muted("DFIXTURE_MUTED")
            || !w.users["UFIXTURE_BOT"].is_bot
        {
            return Err(format!(
                "Fixture requires six eligible and two high-count excluded DMs, got {conversations} unread conversations"
            ));
        }
    }
    apply_frame(
        &mut state,
        json!({"type":"pref_change","name":"activity_dms_opt_out","value":true}),
    )?;
    badge_dom(desktop, "23", "46").await?;
    {
        let mut shell = state.write();
        set_policy(shell.core.workspaces.get_mut("T1").unwrap(), true);
    }
    badge_dom(desktop, "37", "46").await?;
    apply_frame(
        &mut state,
        json!({"type":"pref_change","name":"activity_dms_opt_out","value":false}),
    )?;
    badge_dom(desktop, "41", "46").await?;
    // Known Inbox plus temporarily absent categories must never fall back to
    // the raw sum. The independent DM highlight total remains 46.
    state
        .write()
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .set_activity_unread_count(61);
    badge_dom(desktop, "", "46").await?;
    apply_frame(
        &mut state,
        json!({"type":"badge_counts_updated","activity_v2":{"dm":17}}),
    )?;
    badge_dom(desktop, "17", "46").await?;
    apply_frame(
        &mut state,
        json!({"type":"badge_counts_updated","activity_v2":{"message_reaction":9,"future_unknown":11}}),
    )?;
    badge_dom(desktop, "", "46").await?;
    if state.peek().core.activity.unread_count() != 3 {
        return Err("Filtered zero incorrectly marked unread feed read".into());
    }
    apply_frame(
        &mut state,
        json!({"type":"badge_counts_updated","activity_v2":{}}),
    )?;
    badge_dom(desktop, "", "46").await?;
    let shell = state.peek();
    if shell.core.activity.items.len() != 3
        || shell.core.activity.unread_count() != 0
        || shell.core.workspaces["T1"].activity_unread_count != Some(0)
    {
        return Err("Zero badge did not reconcile loaded rows while preserving history".into());
    }
    Ok(())
}

fn set_policy(workspace: &mut super_platinum_core::state::Workspace, enabled: bool) {
    let group = if enabled { "on" } else { "control" };
    let policy=super_platinum_core::slack::api::parse_badge_experiments(&json!({"ok":true,"assignments":{
        "activity_inbox":{"group":"on"},"lists_monster_step":{"group":group},"approvals_in_todos":{"group":group},
        "unjoined_channel_mention_in_activity":{"group":group},"inviter_joiner_first_dm_v2":{"group":if enabled{"treatment"}else{"on"}}
    }})).unwrap();
    let ticket = workspace.activity_counts.begin_experiments();
    assert!(workspace.activity_counts.apply_experiments(ticket, policy));
}
fn apply_frame(state: &mut Signal<ShellState>, frame: Value) -> Result<(), String> {
    let event = super_platinum_core::slack::realtime::parse_event(&frame.to_string())
        .ok_or("Badge fixture frame did not decode")?;
    let mut shell = state.write();
    let generation = shell.core.workspaces["T1"].rt_generation;
    crate::realtime::apply_in_view(&mut shell.core, "T1", generation, event, false);
    Ok(())
}
async fn badge_dom(
    desktop: &dioxus::desktop::DesktopContext,
    activity: &str,
    dm: &str,
) -> Result<(), String> {
    wait_dom(desktop,&format!(r#"(()=>{{const text=title=>document.querySelector('.rail button[title="'+title+'"] .rail-badge')?.textContent?.trim()||'';const a=text('Activity'),d=text('Direct messages'),h=document.querySelector('.list-panel-header .ping-badge')?.textContent?.trim()||'';return{{ok:a==={activity:?}&&h==={activity:?}&&d==={dm:?},activity:a,header:h,dm:d}};}})()"#)).await.map(|_|())
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
