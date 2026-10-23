//! Optional real Reply/composer setup, never a thread-controller shortcut.
use super::*;
use crate::channel_workflows::details::fixture::thread as data;

pub(crate) fn exposure(script: &str) -> String {
    if data::requested() {
        script.replace("document.getElementById('channel-workflow-details')", "(document.getElementById('channel-workflow-details')||document.querySelector('.thread-pane'))")
    } else {
        script.into()
    }
}
async fn action(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
    kind: &str,
    label: &str,
    expected: Value,
) -> Result<Value, String> {
    let script = include_str!("thread/actions.js")
        .replace("ACTION", &json!(kind).to_string())
        .replace("LABEL", &json!(label).to_string())
        .replace("EXPECTED", &expected.to_string())
        .replace("DRAFT", &json!(data::DRAFT).to_string());
    let activation = evidence::observed_wait(
        state,
        desktop,
        visit,
        "original thread setup action",
        &script,
    )
    .await?;
    if activation["kind"] != "keyboard-request" {
        return Ok(activation);
    }
    if kind != "tab" || label != "Workflows" || activation["key"] != "Enter" {
        return Err("Unexpected thread keyboard request".into());
    }
    let request = json!({"version":1,
        "fixture":std::env::var("SUPER_PLATINUM_FIXTURE").map_err(|_| "Thread keyboard fixture absent")?,
        "nonce":std::env::var("SUPER_PLATINUM_WORKFLOW_MANAGEMENT_NONCE").map_err(|_| "Thread keyboard nonce absent")?,
        "pid":std::process::id(),"nodeId":activation["nodeId"],"key":"Enter","label":"Workflows"});
    eprintln!("workflow-thread-keyboard: {request}");
    let witness =
        include_str!("thread/keyboard.js").replace("EXPECTED", &activation["nodeId"].to_string());
    evidence::observed_wait(
        state,
        desktop,
        visit,
        "original trusted Workflows Enter",
        &witness,
    )
    .await
}
pub(super) async fn setup(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
) -> Result<Value, String> {
    let before = observe(state, desktop, visit).await?;
    let messages_activation = action(state, desktop, visit, "tab", "Messages", Value::Null).await?;
    evidence::observed_wait(state,desktop,visit,"original seeded message parent",r#"({ok:!!document.querySelector('#message-timeline article[data-message-id="1781524900.000001"]')&&!document.getElementById('channel-workflows-panel')})"#).await?;
    let messages = observe(state, desktop, visit).await?;
    let reply_activation = action(state, desktop, visit, "reply", "Reply", Value::Null).await?;
    evidence::observed_wait(state,desktop,visit,"original thread field mounted",r#"(()=>{const e=document.getElementById('thread-composer');return{ok:!!e?.isConnected&&!!e.dataset.projectionVisit&&!!e.dataset.draftScope&&!!document.getElementById('thread-body')}})()"#).await?;
    for _ in 0..100 {
        let s = state.peek();
        if s.thread_root.as_deref() == Some(data::ROOT) && !s.thread_loading {
            break;
        }
        drop(s);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    if state.peek().thread_root.as_deref() != Some(data::ROOT) || state.peek().thread_loading {
        return Err("Actual Reply did not open original thread".into());
    }
    let opened = observe(state, desktop, visit).await?;
    let input_activation = action(
        state,
        desktop,
        visit,
        "input",
        "thread-composer",
        opened["native"]["detailsThread"]["composer"]["nodeId"].clone(),
    )
    .await?;
    for _ in 0..100 {
        if state.peek().core.thread_composer.text == data::DRAFT {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    if state.peek().core.thread_composer.text != data::DRAFT {
        return Err("Original native thread draft was not accepted".into());
    }
    native(desktop, "thread-bind").await?;
    let drafted = observe(state, desktop, visit).await?;
    let tab_activation = action(state, desktop, visit, "tab", "Workflows", Value::Null).await?;
    workflows::settled(desktop, "ready").await?;
    let after = observe(state, desktop, visit).await?;
    if after["native"]["detailsThread"]["originalNodes"] != true
        || after["native"]["detailsThread"]["composer"]["value"] != data::DRAFT
        || after["native"]["detailsThread"]["composer"]["selectionStart"] != 7
        || after["native"]["detailsThread"]["composer"]["selectionEnd"] != 22
    {
        return Err(format!(
            "Original thread nodes/draft changed across Workflows tab: {after}"
        ));
    }
    Ok(
        json!({"before":before,"messagesActivation":messages_activation,"messages":messages,
        "replyActivation":reply_activation,"opened":opened,"inputActivation":input_activation,
        "drafted":drafted,"tabActivation":tab_activation,"after":after}),
    )
}
