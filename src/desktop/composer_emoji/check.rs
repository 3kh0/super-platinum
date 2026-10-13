//! Disconnected native insertion checks; never sends messages or reactions.
use crate::state::{ChannelOpen, ShellState};
use dioxus::prelude::*;
use serde_json::{Value, json};

pub(crate) fn install(shell: &mut ShellState) {
    shell.core.transport = None;
    shell.core.session = None;
    shell.overlay = None;
    shell.toast = None;
    shell.close_thread();
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .custom_emoji
        .insert(
            "party_fixture".into(),
            super_platinum_core::slack::models::Emoji {
                name: "party_fixture".into(),
                value: "alias:rocket".into(),
                ..Default::default()
            },
        );
    shell.core.composer.text = "Emoji stay in this draft until you send.".into();
    shell.checkpoint_composers();
}

pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if std::env::var("SUPER_PLATINUM_COMPOSER_EMOJI_CHECK").as_deref() != Ok("1")
        || !matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("composer-emoji" | "composer-emoji-narrow")
        )
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let receipt = match run(state, &desktop).await {
            Ok(v) => v,
            Err(error) => json!({"ok":false,"error":error}),
        };
        eprintln!("composer-emoji-check: {receipt}");
    });
}

async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    if state.peek().core.transport.is_some() || state.peek().core.session.is_some() {
        return Err("Connected fixture refused".into());
    }
    let before = messages(&state.peek());
    let channel = "channel-composer";
    ready(desktop, channel).await?;
    seed(desktop, channel, "Hi <@U1> 🦊 replace end", 12, 19).await?;
    open(state, desktop, channel, "channel-selection").await?;
    let initial_layout = layout(desktop).await?;
    pick(desktop, "rocket").await?;
    value(desktop, channel, "Hi <@U1> 🦊 🚀 end", 14, 14, true).await?;
    model(state, false, "Hi <@U1> 🦊 🚀 end").await?;

    // Escape cancels without replacing the exact selected occurrence.
    seed(desktop, channel, "🦊 <@U1> keep", 3, 8).await?;
    open(state, desktop, channel, "cancel-selection").await?;
    act(desktop,r#"document.getElementById('emoji-query').dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}))"#).await?;
    value(desktop, channel, "🦊 <@U1> keep", 3, 8, true).await?;
    model(state, false, "🦊 <@U1> keep").await?;

    // A newer underlying draft wins even while a previously opened picker lives.
    open(state, desktop, channel, "stale-text").await?;
    seed(desktop, channel, "Newer draft", 11, 11).await?;
    act(desktop, "document.getElementById('emoji-query').focus()").await?;
    pick(desktop, "rocket").await?;
    value(desktop, channel, "Newer draft", 11, 11, false).await?;
    model(state, false, "Newer draft").await?;

    open(state, desktop, channel, "navigation").await?;
    {
        let mut shell = state.write();
        let index = shell.channels.iter().position(|c| c.id == "C1").unwrap();
        shell.select_channel(index, ChannelOpen::Global);
    }
    wait(desktop, r#"({ok:!document.getElementById('emoji-query')})"#).await?;
    {
        let mut shell = state.write();
        let index = shell.channels.iter().position(|c| c.id == "C2").unwrap();
        shell.select_channel(index, ChannelOpen::Global);
    }
    ready(desktop, channel).await?;
    model(state, false, "Newer draft").await?;

    // Mount the same ordinary thread composer, with its independent draft.
    {
        let mut shell = state.write();
        let root = shell.core.workspaces["T1"].messages["C2"].messages[0].clone();
        let ts = root.ts.clone().unwrap();
        let mut bag = super_platinum_core::state::ChannelMessages::default();
        bag.upsert(root);
        shell
            .core
            .threads
            .insert(("T1".into(), "C2".into(), ts.clone()), bag);
        shell.begin_thread(ts, None);
        shell.thread_loading = false;
        shell.refresh_thread_from_core();
    }
    let thread = "thread-composer";
    ready(desktop, thread).await?;
    seed(desktop, thread, "🦊 reply <@U1>", 3, 3).await?;
    open(state, desktop, thread, "thread-custom").await?;
    pick(desktop, "party_fixture").await?;
    value(
        desktop,
        thread,
        "🦊 :party_fixture:reply <@U1>",
        18,
        18,
        true,
    )
    .await?;
    model(state, true, "🦊 :party_fixture:reply <@U1>").await?;
    model(state, false, "Newer draft").await?;
    if messages(&state.peek()) != before {
        return Err("Draft emoji changed channel messages".into());
    }
    // Final capture shows the bounded picker above the real thread draft.
    open(state, desktop, thread, "final-capture").await?;
    let final_layout = layout(desktop).await?;
    Ok(
        json!({"ok":true,"channelSelection":true,"threadCaret":true,"unicodeAndMentions":true,
        "customShortcode":true,"cancelPreservesSelection":true,"staleTextRejected":true,
        "navigationRetiresPicker":true,"duplicateChoiceBlocked":true,"focusReturned":true,
        "noMessagesOrReactions":true,"noTransport":true,"layout":initial_layout,"finalLayout":final_layout}),
    )
}

fn messages(shell: &ShellState) -> Value {
    Value::Object(
        shell.core.workspaces["T1"]
            .messages
            .iter()
            .map(|(id, bag)| {
                (
                    id.clone(),
                    json!({"messages":bag.messages,"pending":bag.pending}),
                )
            })
            .collect(),
    )
}
async fn wait(desktop: &dioxus::desktop::DesktopContext, script: &str) -> Result<Value, String> {
    crate::slack_links_check::wait_dom(desktop, script)
        .await
        .map_err(|error| format!("{error}: {script}"))
}
async fn act(desktop: &dioxus::desktop::DesktopContext, script: &str) -> Result<(), String> {
    wait(
        desktop,
        &format!("(()=>{{{script};return {{ok:true}};}})()"),
    )
    .await
    .map(|_| ())
}
async fn ready(desktop: &dioxus::desktop::DesktopContext, id: &str) -> Result<(), String> {
    wait(desktop,&format!(r#"(()=>{{const f=document.getElementById('{id}'),b=document.getElementById('{id}-emoji'),r=b?.getBoundingClientRect();return {{ok:f?.dataset.completionReady==='true'&&!!b&&r.height>=24&&r.height<=32&&r.width===r.height}};}})()"#)).await.map(|_|())
}
async fn seed(
    desktop: &dioxus::desktop::DesktopContext,
    id: &str,
    text: &str,
    start: usize,
    end: usize,
) -> Result<(), String> {
    let text = serde_json::to_string(text).unwrap();
    act(desktop,&format!("const f=document.getElementById('{id}');f.focus();f.value={text};f.setSelectionRange({start},{end});f.dispatchEvent(new Event('input',{{bubbles:true}}))")).await
}
async fn open(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    id: &str,
    stage: &str,
) -> Result<(), String> {
    eprintln!("composer-emoji-stage: {stage}");
    act(
        desktop,
        &format!("document.getElementById('{id}-emoji').click()"),
    )
    .await?;
    if let Err(error) = wait(desktop,r#"({ok:document.querySelector('#emoji-picker-title')?.textContent==='Insert emoji'&&document.activeElement?.id==='emoji-query'})"#).await {
        let dom = wait(desktop, &format!(r#"(()=>{{const f=document.getElementById('{id}');return {{ok:true,title:document.querySelector('#emoji-picker-title')?.textContent,active:document.activeElement?.id,dialog:!!document.getElementById('shell-overlay-dialog'),field:{{connected:f?.isConnected,scope:f?.dataset.draftScope,ready:f?.dataset.completionReady,composing:f?.dataset.composing,disabled:f?.disabled,readOnly:f?.readOnly,start:f?.selectionStart,end:f?.selectionEnd}}}};}})()"#)).await;
        let shell = state.peek();
        let active = shell.composer_emoji.active.as_ref().map(|visit|json!({"serial":visit.serial,"scopeCurrent":visit.scope.current(&shell)}));
        return Err(format!("{stage}: {error}; overlay={:?}, serial={}, active={active:?}, DOM={dom:?}",shell.overlay,shell.composer_emoji.serial));
    }
    Ok(())
}
async fn pick(desktop: &dioxus::desktop::DesktopContext, name: &str) -> Result<(), String> {
    let name = serde_json::to_string(name).unwrap();
    act(desktop,&format!("const q=document.getElementById('emoji-query');q.value={name};q.dispatchEvent(new Event('input',{{bubbles:true}}))")).await?;
    wait(desktop,&format!("(()=>{{const b=[...document.querySelectorAll('#emoji-options button')].find(b=>b.getAttribute('aria-label')==={name});return {{ok:!!b}};}})()")).await?;
    act(desktop,&format!("const b=[...document.querySelectorAll('#emoji-options button')].find(b=>b.getAttribute('aria-label')==={name});b.click();b.click()")).await?;
    wait(desktop, "({ok:!document.getElementById('emoji-query')})")
        .await
        .map(|_| ())
}
async fn value(
    desktop: &dioxus::desktop::DesktopContext,
    id: &str,
    text: &str,
    start: usize,
    end: usize,
    focus: bool,
) -> Result<(), String> {
    let text = serde_json::to_string(text).unwrap();
    wait(desktop,&format!("(()=>{{const f=document.getElementById('{id}');return {{ok:f?.value==={text}&&f.selectionStart==={start}&&f.selectionEnd==={end}&&(!{focus}||document.activeElement===f),value:f?.value,start:f?.selectionStart,end:f?.selectionEnd,active:document.activeElement?.id}};}})()")).await.map(|_|())
}
async fn model(state: Signal<ShellState>, thread: bool, text: &str) -> Result<(), String> {
    crate::slack_links_check::wait_state(state, |s| {
        if thread {
            s.core.thread_composer.text == text
        } else {
            s.core.composer.text == text
        }
    })
    .await
}
async fn layout(desktop: &dioxus::desktop::DesktopContext) -> Result<Value, String> {
    wait(desktop,r#"(()=>{const d=document.querySelector('.emoji-picker'),q=document.getElementById('emoji-query'),options=document.getElementById('emoji-options');if(!d||!q||!options)return {ok:false};const r=d.getBoundingClientRect(),s=q.getBoundingClientRect();return {ok:r.left>=0&&r.top>=0&&r.right<=innerWidth&&r.bottom<=innerHeight&&s.top>=r.top&&s.bottom<=r.bottom&&options.querySelectorAll('button').length<=120,viewport:[innerWidth,innerHeight],bounds:[r.left,r.top,r.right,r.bottom],choices:options.querySelectorAll('button').length};})()"#).await
}
