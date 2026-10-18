//! Disconnected native coverage for the real rich-link click handlers.
use crate::state::{ChannelOpen, ShellState};
use dioxus::prelude::*;
use serde_json::{Value, json};
use super_platinum_core::{
    config::{Session, WorkspaceSession},
    slack::models::Message,
    state::ChannelMessages,
};

const ROOT: &str = "1719800000.000100";
const REPLY: &str = "1719800001.000001";

pub(crate) fn install(shell: &mut ShellState) {
    let session = WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: shell.core.workspaces["T1"].self_user_id.clone(),
        name: "Echonet".into(),
        url: "https://echonet.slack.com".into(),
        token: String::new(),
    };
    shell.core.session = Some(Session {
        unified_user_client_enabled: Default::default(),
        d_cookie: String::new(),
        workspaces: [("T1".into(), session)].into(),
    });
    shell.core.transport = None;
    shell.toast = None;
    shell.overlay = None;
    let links = "Slack links stay in this workspace.\n<slack://channel?team=T1&id=C1|Open linked channel>\n<slack://channel?team=T1&id=C2&message=1719800001.000001&thread_ts=1719800000.000100|Open exact reply>\n<https://echonet.slack.com/archives/C2/p1719700000000100|Open message permalink>\n<slack://unsupported?team=T1|Unsupported Slack link>";
    let main = message("C2", ROOT, links);
    let mut replies = ChannelMessages::default();
    replies.upsert(main.clone());
    replies.upsert(message(
        "C2",
        REPLY,
        "This is the exact linked reply. Timestamp precision is preserved.",
    ));
    for index in 2..=32 {
        replies.upsert(message(
            "C2",
            &format!("{}.000001", 1719800000 + index),
            "A later reply keeps the linked reply away from the bottom of the thread.",
        ));
    }
    shell
        .core
        .threads
        .insert(("T1".into(), "C2".into(), ROOT.into()), replies);
    let workspace = shell.core.workspaces.get_mut("T1").unwrap();
    for (channel, item) in [
        ("C2", main),
        (
            "C1",
            message(
                "C1",
                ROOT,
                "The channel link opened here.\n<slack://channel?team=T1&id=C2|Return to links>",
            ),
        ),
    ] {
        let mut bag = ChannelMessages::default();
        if channel == "C2" {
            bag.upsert(message(
                channel,
                "1719700000.000100",
                "The permalink targets this older message, not the latest conversation view.\n<slack://channel?team=T1&id=C2&message=1719800001.000001&thread_ts=1719800000.000100|Open linked reply from history>",
            ));
            for index in 1..=30 {
                bag.upsert(message(
                    channel,
                    &format!("{}.000100", 1719700000 + index),
                    "Earlier discussion keeps the linked message outside the latest viewport.",
                ));
            }
        }
        bag.upsert(item);
        workspace.messages.insert(channel.into(), bag);
    }
    let index = shell
        .channels
        .iter()
        .position(|channel| channel.id == "C2")
        .unwrap();
    shell.select_channel(index, ChannelOpen::Global);
}

fn message(channel: &str, ts: &str, text: &str) -> Message {
    serde_json::from_value(json!({"channel":channel,"ts":ts,"user":"U1","text":text,
        "blocks":[{"type":"section","text":{"type":"mrkdwn","text":text}}]}))
    .unwrap()
}

pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if std::env::var("SUPER_PLATINUM_SLACK_LINKS_CHECK").as_deref() != Ok("1")
        || !matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("slack-links" | "slack-links-narrow")
        )
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let result = run(state, &desktop)
            .await
            .unwrap_or_else(|error| json!({"ok":false,"error":error}));
        eprintln!("slack-links-check: {result}");
    });
}

async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    click(desktop, "Unsupported Slack link").await?;
    wait_state(state, |shell| {
        shell.toast.is_some() && shell.core.active_channel.as_deref() == Some("C2")
    })
    .await?;
    state.write().toast = None;
    click(desktop, "Open linked channel").await?;
    wait_state(state, |shell| {
        shell.core.active_channel.as_deref() == Some("C1")
    })
    .await?;
    click(desktop, "Return to links").await?;
    wait_state(state, |shell| {
        shell.core.active_channel.as_deref() == Some("C2")
    })
    .await?;
    click(desktop, "Open exact reply").await?;
    wait_state(state, |shell| {
        shell.thread_root.as_deref() == Some(ROOT) && !shell.thread_at_bottom
    })
    .await?;
    state.write().close_thread();
    let generation = state.peek().channel_generation;
    click(desktop, "Open message permalink").await?;
    wait_state(state, |shell| {
        shell.channel_generation != generation
            && shell.core.active_channel.as_deref() == Some("C2")
            && shell.thread_root.is_none()
    })
    .await?;
    wait_dom(desktop, r#"(()=>{const p=document.getElementById('message-timeline'),e=p?.querySelector('[data-message-id="1719700000.000100"]');const r=e?.getBoundingClientRect(),c=p?.getBoundingClientRect();return {ok:!!r&&!!c&&r.width>0&&r.bottom>c.top&&r.top<c.bottom};})()"#).await?;
    click(desktop, "Open linked reply from history").await?;
    wait_state(state, |shell| {
        shell.thread_root.as_deref() == Some(ROOT) && !shell.thread_at_bottom
    })
    .await?;
    let mut receipt = wait_dom(desktop, r#"(()=>{const panel=document.getElementById('thread-body'),clip=panel?.getBoundingClientRect();const rows=[...panel.querySelectorAll('[data-message-id]')];const reply=rows.find(e=>e.getAttribute('data-message-id')==='1719800001.000001');const r=reply?.getBoundingClientRect();return {ok:!!r&&r.width>0&&r.bottom>Math.max(0,clip.top)&&r.top<Math.min(innerHeight,clip.bottom)&&document.documentElement.scrollWidth<=innerWidth+1,replyVisible:!!r,viewport:[innerWidth,innerHeight]};})()"#).await?;
    if state.peek().core.transport.is_some() {
        return Err("Fixture unexpectedly has a transport".into());
    }
    receipt["channelClickNavigated"] = json!(true);
    receipt["replyIdentityPreserved"] = json!(true);
    receipt["permalinkHandled"] = json!(true);
    receipt["unsupportedFeedback"] = json!(true);
    receipt["noTransport"] = json!(true);
    Ok(receipt)
}

pub(crate) async fn wait_state(
    state: Signal<ShellState>,
    predicate: impl Fn(&ShellState) -> bool,
) -> Result<(), String> {
    for _ in 0..60 {
        if predicate(&state.peek()) {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let shell = state.peek();
    Err(format!(
        "Slack link did not reach its expected navigation state: {}",
        json!({"channel":shell.core.active_channel,"thread":shell.thread_root,
            "target":shell.thread_target,"atBottom":shell.thread_at_bottom,
            "channelGeneration":shell.channel_generation})
    ))
}

async fn click(desktop: &dioxus::desktop::DesktopContext, label: &str) -> Result<(), String> {
    let label = serde_json::to_string(label).unwrap();
    let script = format!(
        "(()=>{{const a=[...document.querySelectorAll('a')].find(a=>a.textContent==={label});if(!a)return {{ok:false}};a.scrollIntoView({{block:'nearest'}});a.click();return {{ok:true}};}})()"
    );
    wait_dom(desktop, &script)
        .await
        .map(|_| ())
        .map_err(|error| format!("Click {label}: {error}"))
}

pub(crate) async fn wait_dom(
    desktop: &dioxus::desktop::DesktopContext,
    script: &str,
) -> Result<Value, String> {
    for _ in 0..60 {
        let (send, mut receive) = tokio::sync::mpsc::unbounded_channel();
        desktop
            .webview
            .evaluate_script_with_callback(script, move |value| {
                let _ = send.send(value);
            })
            .map_err(|_| "Native evaluation failed")?;
        let raw = tokio::time::timeout(std::time::Duration::from_secs(2), receive.recv())
            .await
            .map_err(|_| "Native evaluation timed out")?
            .ok_or("Native callback closed")?;
        let mut value: Value = serde_json::from_str(&raw).map_err(|_| "Invalid native callback")?;
        if let Value::String(encoded) = value {
            value = serde_json::from_str(&encoded).map_err(|_| "Invalid encoded callback")?;
        }
        if value["ok"] == true {
            return Ok(value);
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Err("Slack link DOM assertion timed out".into())
}
