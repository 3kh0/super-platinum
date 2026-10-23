//! Opt-in actual composer edits; raw fixture history is the only seeded state.
use super::*;
use backend::drafts as data;

pub(crate) fn requested() -> bool {
    data::requested()
}
pub(crate) fn snapshot(s: &ShellState) -> Value {
    data::snapshot(s)
}
pub(crate) fn script(bind: &str) -> String {
    include_str!("drafts/native.js").replace("BIND", &json!(bind).to_string())
}
pub(crate) fn wrap(base: &str) -> String {
    let base = include_str!("drafts/exposure.js")
        .replace("BASE", base)
        .replace("GEOMETRY", include_str!("drafts/geometry.js"));
    format!(
        "(()=>{{const base={base},drafts={};return{{...base,ok:base.ok&&drafts.ok,shareDrafts:drafts}}}})()",
        script("observe")
    )
}
async fn observed(state: Signal<ShellState>, desktop: &DesktopContext) -> Result<Value, String> {
    let native = super::evaluate(desktop, &script("observe")).await?;
    let s = state.peek();
    if !data::enabled(&s) {
        return Err("Share draft original owner lost".into());
    }
    Ok(json!({"model":data::snapshot(&s),"native":native}))
}
async fn wait_state(
    state: Signal<ShellState>,
    label: &str,
    test: impl Fn(&ShellState) -> bool,
) -> Result<(), String> {
    for _ in 0..100 {
        if test(&state.peek()) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!(
        "Share draft {label}: {}",
        data::snapshot(&state.peek())
    ))
}
async fn action(
    desktop: &DesktopContext,
    kind: &str,
    label: &str,
    value: &str,
    expected: Value,
) -> Result<Value, String> {
    let script = include_str!("drafts/actions.js")
        .replace("GEOMETRY", include_str!("drafts/geometry.js"))
        .replace("KIND", &json!(kind).to_string())
        .replace("LABEL", &json!(label).to_string())
        .replace("VALUE", &json!(value).to_string())
        .replace("EXPECTED", &expected.to_string())
        .replace("ROOT", &json!(data::ROOT).to_string());
    let activation = super::evaluate(desktop, &script).await?;
    if activation["kind"] != "keyboard-request" {
        return Ok(activation);
    }
    if !requested() || !matches!(label, "Finish Up" | "Messages") || activation["key"] != "Enter" {
        return Err("Unexpected Share draft keyboard request".into());
    }
    let request = json!({"version":1,
        "fixture":std::env::var("SUPER_PLATINUM_FIXTURE").map_err(|_| "Share keyboard fixture absent")?,
        "nonce":std::env::var("SUPER_PLATINUM_WORKFLOW_MANAGEMENT_NONCE").map_err(|_| "Share keyboard nonce absent")?,
        "pid":std::process::id(),"nodeId":activation["nodeId"],"key":"Enter","label":label});
    eprintln!("workflow-thread-keyboard: {request}");
    wait_dom(
        desktop,
        &include_str!("drafts/keyboard.js")
            .replace("EXPECTED", &activation["nodeId"].to_string())
            .replace("LABEL", &json!(label).to_string()),
    )
    .await
}

pub(crate) async fn edit_metadata(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    title: &str,
) -> Result<Value, String> {
    let registry = super::evaluate(desktop, &script("observe")).await?;
    if registry["sameThreadNodes"] != true {
        return Err("Original thread absent for metadata edit".into());
    }
    let title_event = action(
        desktop,
        "metadata",
        "workflow-management-title",
        title,
        Value::Null,
    )
    .await?;
    let description_event = action(
        desktop,
        "metadata",
        "workflow-management-description",
        DESCRIPTION,
        Value::Null,
    )
    .await?;
    wait(state, "accepted exact proposed text", |v| {
        v["title"] == title && v["description"] == DESCRIPTION && v["dirty"] == true
    })
    .await?;
    Ok(json!({"title":title_event,"description":description_event}))
}
pub(crate) async fn open_finish(desktop: &DesktopContext) -> Result<Value, String> {
    action(desktop, "button", "Finish Up", "", Value::Null).await
}
async fn mounted(desktop: &DesktopContext, id: &str) -> Result<(), String> {
    let script = format!(
        "(()=>{{const e=document.getElementById({});return{{ok:!!e?.isConnected&&!!e.dataset.draftScope&&!!e.dataset.projectionVisit}}}})()",
        json!(id)
    );
    wait_dom(desktop, &script).await?;
    Ok(())
}
async fn capture(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    label: &str,
) -> Result<Value, String> {
    let before = observed(state, desktop).await?;
    let directory = std::env::var_os("SUPER_PLATINUM_UI_CAPTURE_DIR")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("Absolute draft capture directory required")?;
    let path = directory.join(format!("{name}-{label}.png"));
    let check = path.clone();
    if tokio::task::spawn_blocking(move || check.try_exists())
        .await
        .map_err(|_| "Draft capture worker failed")?
        .map_err(|_| "Draft capture path failed")?
    {
        return Err("Draft capture must be fresh".into());
    }
    eprintln!(
        "workflow-management-capture: {}",
        json!({"stage":label,"edge":"before"})
    );
    let (path, width, height) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), desktop).await?;
    let after = observed(state, desktop).await?;
    if before != after {
        return Err(format!(
            "Share draft capture changed: before={before} after={after}"
        ));
    }
    eprintln!(
        "workflow-management-capture: {}",
        json!({"stage":label,"edge":"after"})
    );
    Ok(
        json!({"path":path.to_string_lossy(),"pngDimensions":[width,height],"before":before,"after":after}),
    )
}
pub(crate) async fn setup(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    data::seed(&mut state.write())?;
    let before = observed(state, desktop).await?;
    let messages_activation = action(desktop, "tab", "Messages", "", Value::Null).await?;
    mounted(desktop, "channel-composer").await?;
    wait_dom(desktop,&format!("({{ok:!!document.querySelector('#message-timeline article[data-message-id=\"{}\"]')}})",data::ROOT)).await?;
    super::evaluate(desktop, &script("channel")).await?;
    let messages = observed(state, desktop).await?;
    let channel_input = action(
        desktop,
        "input",
        "channel-composer",
        data::MAIN_DRAFT,
        messages["native"]["channel"]["nodeId"].clone(),
    )
    .await?;
    wait_state(state, "channel input", |s| {
        s.core.composer.text == data::MAIN_DRAFT
    })
    .await?;
    let channel_drafted = observed(state, desktop).await?;
    let reply_activation = action(desktop, "reply", "Reply", "", Value::Null).await?;
    mounted(desktop, "thread-composer").await?;
    wait_state(state, "Reply", |s| {
        s.thread_root.as_deref() == Some(data::ROOT) && !s.thread_loading
    })
    .await?;
    let opened = observed(state, desktop).await?;
    let thread_input = action(
        desktop,
        "input",
        "thread-composer",
        data::THREAD_DRAFT,
        opened["native"]["thread"]["nodeId"].clone(),
    )
    .await?;
    wait_state(state, "thread input", |s| {
        s.core.thread_composer.text == data::THREAD_DRAFT
    })
    .await?;
    super::evaluate(desktop, &script("thread")).await?;
    let thread_drafted = observed(state, desktop).await?;
    let screenshot = capture(state, desktop, name, "draft-setup").await?;
    let about_activation = action(desktop, "tab", "About", "", Value::Null).await?;
    wait_dom(desktop,"({ok:!!document.getElementById('workflow-configuration')&&!document.getElementById('channel-composer')})").await?;
    let after = observed(state, desktop).await?;
    if after["native"]["sameThreadNodes"] != true
        || after["native"]["thread"]["value"] != data::THREAD_DRAFT
    {
        return Err(format!(
            "Original Share thread changed across About: {after}"
        ));
    }
    Ok(
        json!({"before":before,"messagesActivation":messages_activation,"messages":messages,
        "channelInput":channel_input,"channelDrafted":channel_drafted,"replyActivation":reply_activation,
        "opened":opened,"threadInput":thread_input,"threadDrafted":thread_drafted,
        "aboutActivation":about_activation,"after":after,"capture":screenshot}),
    )
}
pub(crate) async fn returned(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let before = observed(state, desktop).await?;
    let sidebar_activation = if state.peek().core.active_channel.as_deref() != Some(data::CHANNEL) {
        let a = action(desktop, "sidebar", "Review requests", "", Value::Null).await?;
        wait_state(state, "original sidebar", |s| {
            s.core.active_channel.as_deref() == Some(data::CHANNEL)
        })
        .await?;
        a
    } else {
        Value::Null
    };
    wait_dom(desktop,"({ok:[...document.querySelectorAll('.channel-tabs button')].some(e=>e.textContent.trim()==='Messages')})").await?;
    let messages_activation = action(desktop, "tab", "Messages", "", Value::Null).await?;
    mounted(desktop, "channel-composer").await?;
    wait_state(state, "restored channel draft", |s| {
        s.core.composer.text == data::MAIN_DRAFT
    })
    .await?;
    let after = observed(state, desktop).await?;
    let reply_activation = if state.peek().thread_root.as_deref() != Some(data::ROOT) {
        let a = action(desktop, "reply", "Reply", "", Value::Null).await?;
        mounted(desktop, "thread-composer").await?;
        a
    } else {
        Value::Null
    };
    wait_state(state, "restored thread draft", |s| {
        s.thread_root.as_deref() == Some(data::ROOT)
            && !s.thread_loading
            && s.core.thread_composer.text == data::THREAD_DRAFT
    })
    .await?;
    let restored = observed(state, desktop).await?;
    if restored["native"]["channel"]["value"] != data::MAIN_DRAFT
        || restored["native"]["thread"]["value"] != data::THREAD_DRAFT
    {
        return Err(format!("Restored native Share drafts differ: {restored}"));
    }
    let screenshot = capture(state, desktop, name, "draft-returned").await?;
    Ok(
        json!({"before":before,"sidebarActivation":sidebar_activation,"messagesActivation":messages_activation,"after":after,"replyActivation":reply_activation,"restored":restored,"capture":screenshot}),
    )
}
