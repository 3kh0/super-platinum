//! Disconnected coverage for authored broadcasts in the channel and thread.
use crate::state::{MessageVm, ShellState};
use dioxus::prelude::*;
use serde_json::{Value, json};
use super_platinum_core::{slack::models::Message, state::ChannelMessages};
const ROOT: &str = "1719930000.000001";
const REPLY: &str = "1719930001.000001";
const OTHER_ROOT: &str = "1719930000.000002";
const LEGACY: &str = "1719930002.000001";
const PLAIN: &str = "1719930003.000001";
fn message(ts: &str, text: &str, root: Option<&str>, subtype: Option<&str>) -> Message {
    serde_json::from_value(
        json!({"channel":"C2","ts":ts,"text":text,"user":"U1","thread_ts":root,"subtype":subtype}),
    )
    .unwrap()
}
pub(crate) fn install(shell: &mut ShellState) {
    shell.core.transport = None;
    shell.core.session = None;
    shell.overlay = None;
    shell.toast = None;
    shell.close_thread();
    let parent = message(
        ROOT,
        "A cached parent keeps its context beside the authored reply.",
        None,
        None,
    );
    let reply = message(
        REPLY,
        "The channel sees this reply without losing its thread.",
        Some(ROOT),
        Some("thread_broadcast"),
    );
    let legacy = message(
        LEGACY,
        "A reply also shared from an older thread.",
        Some(OTHER_ROOT),
        Some("reply_broadcast"),
    );
    let plain = message(
        PLAIN,
        "An ordinary message has no thread preamble.",
        None,
        None,
    );
    let mut channel = ChannelMessages::default();
    for message in [&parent, &reply, &legacy, &plain] {
        channel.upsert(message.clone());
    }
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .insert("C2".into(), channel);
    for (root, items) in [
        (ROOT, vec![parent, reply.clone()]),
        (
            OTHER_ROOT,
            vec![
                message(
                    OTHER_ROOT,
                    "The uncached channel parent is available in its thread.",
                    None,
                    None,
                ),
                legacy.clone(),
            ],
        ),
    ] {
        let mut bag = ChannelMessages::default();
        for message in items {
            bag.upsert(message);
        }
        shell
            .core
            .threads
            .insert(("T1".into(), "C2".into(), root.into()), bag);
    }
    // The older parent remains cached outside the displayed channel window.
    let rows: Vec<MessageVm> = [reply, legacy, plain]
        .iter()
        .map(|m| super::message_vm(&shell.core.workspaces["T1"], m, &shell.media))
        .collect();
    shell.install_fixture_messages(rows);
}
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if std::env::var("SUPER_PLATINUM_MESSAGE_BROADCAST_CHECK").as_deref() != Ok("1")
        || !matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("message-broadcast" | "message-broadcast-narrow")
        )
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = match run(state, &desktop).await {
            Ok(value) => value,
            Err(error) => json!({"ok":false,"error":error}),
        };
        eprintln!("message-broadcast-check: {result}");
    });
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    if state.peek().core.transport.is_some() || state.peek().core.session.is_some() {
        return Err("Connected fixture refused".into());
    }
    wait(desktop,&format!(r#"(()=>{{const p=document.getElementById('message-timeline'),a=p?.querySelector('[data-message-id="{REPLY}"]'),b=p?.querySelector('[data-message-id="{LEGACY}"]'),c=p?.querySelector('[data-message-id="{PLAIN}"]');return {{ok:p?.querySelectorAll('.message-broadcast-context').length===2&&a?.querySelector('.message-broadcast-preview')?.textContent.includes('cached parent')&&!b?.querySelector('.message-broadcast-preview')&&!c?.querySelector('.message-broadcast-context')&&a.querySelector('.message-body')?.textContent.includes('The channel sees this reply without losing its thread.')&&b.querySelector('.message-body')?.textContent.includes('A reply also shared from an older thread.')&&![a,b].some(e=>e?.querySelector('.message-system'))}};}})()"#)).await?;
    let initial = layout(desktop, REPLY).await?;
    for (reply, root) in [(REPLY, ROOT), (LEGACY, OTHER_ROOT)] {
        act(desktop,&format!("const b=document.querySelector('#message-timeline [data-thread-reply=\"{reply}\"]');b.focus();b.click()" )).await?;
        crate::slack_links_check::wait_state(state, |shell| {
            shell.thread_root.as_deref() == Some(root)
        })
        .await?;
        wait(desktop,&format!(r#"(()=>{{const p=document.getElementById('thread-body'),r=p?.querySelector('[data-message-id="{reply}"]'),c=p?.getBoundingClientRect(),b=r?.getBoundingClientRect();return {{ok:!!b&&b.height>0&&b.bottom>Math.max(0,c.top)&&b.top<Math.min(innerHeight,c.bottom)&&p.querySelectorAll('.message-broadcast-context').length===0}};}})()"#)).await?;
        state.write().close_thread();
        wait(desktop, "({ok:!document.getElementById('thread-body')})").await?;
    }
    // The normal row gesture must share the same parent destination as the link.
    act(desktop,&format!("document.querySelector('#message-timeline [data-message-id=\"{REPLY}\"]').dispatchEvent(new MouseEvent('dblclick',{{bubbles:true,cancelable:true}}))")).await?;
    crate::slack_links_check::wait_state(state, |shell| shell.thread_root.as_deref() == Some(ROOT))
        .await?;
    wait(
        desktop,
        "({ok:document.activeElement?.id==='thread-composer'})",
    )
    .await?;
    state.write().close_thread();
    wait(desktop, "({ok:!document.getElementById('thread-body')})").await?;
    let final_layout = layout(desktop, REPLY).await?;
    Ok(
        json!({"ok":true,"modernContext":true,"legacyContext":true,"cachedParentPreview":true,"missingParentFallback":true,"authoredBodyUnchanged":true,"exactParentJump":true,"noThreadPreamble":true,"doubleClickParent":true,"threadComposerFocus":true,"noTransport":true,"layout":initial,"finalLayout":final_layout}),
    )
}
async fn act(desktop: &dioxus::desktop::DesktopContext, script: &str) -> Result<(), String> {
    wait(
        desktop,
        &format!("(()=>{{{script};return {{ok:true}};}})()"),
    )
    .await
    .map(|_| ())
}
async fn wait(desktop: &dioxus::desktop::DesktopContext, script: &str) -> Result<Value, String> {
    crate::slack_links_check::wait_dom(desktop, script)
        .await
        .map_err(|error| format!("{error}: {script}"))
}
async fn layout(desktop: &dioxus::desktop::DesktopContext, reply: &str) -> Result<Value, String> {
    act(desktop,&format!("document.querySelector('#message-timeline [data-thread-reply=\"{reply}\"]').scrollIntoView({{block:'nearest'}})" )).await?;
    wait(desktop,&format!(r#"(()=>{{const b=document.querySelector('#message-timeline [data-thread-reply="{reply}"]'),p=document.getElementById('message-timeline'),r=b?.getBoundingClientRect(),c=p?.getBoundingClientRect(),label=b?.querySelector('.message-broadcast-label')?.getBoundingClientRect();return {{ok:!!r&&r.width>0&&r.left>=c.left&&r.right<=Math.min(innerWidth,c.right)&&r.top>=Math.max(0,c.top)&&r.bottom<=Math.min(innerHeight,c.bottom)&&r.height<=28&&label.width>0&&document.documentElement.scrollWidth<=innerWidth+1,viewport:[innerWidth,innerHeight],context:r?[r.left,r.top,r.right,r.bottom]:null}};}})()"#)).await
}
