//! Native disconnected overflow interaction check; no HTTP or browser URL opens.
use super::overflow_fixture as fixture;
use crate::state::ShellState;
use dioxus::prelude::*;
use serde_json::{Value, json};

pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !fixture::enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let receipt = match run(state, &desktop).await {
            Ok(value) => value,
            Err(error) => json!({"ok":false,"error":error}),
        };
        eprintln!("message-overflow-check: {receipt}");
    });
}

async fn run(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let mut stage = "initial controls";
    let result:Result<Value,String>=async {
        idle(desktop).await?;
        stage="menu keyboard and Escape";
        open(desktop,"pick").await?;
        checked(desktop,r#"(()=>{const m=document.querySelector('.message-overflow-menu');window.__overflowMenu=m;window.__overflowChoice=m.querySelector('button');m.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true}));return {ok:true};})()"#).await?;
        wait_dom(desktop,r#"({ok:document.activeElement?.dataset.optionValue==='guide'})"#).await?;
        checked(desktop,r#"(()=>{document.querySelector('.message-overflow-menu').dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true}));return {ok:true};})()"#).await?;
        wait_dom(desktop,r#"({ok:!document.querySelector('.message-overflow-menu')&&document.activeElement?.dataset.actionId==='pick'})"#).await?;
        attempts(state,0,0)?;

        stage="confirmation cancel never dispatches or opens URL";
        open(desktop,"pick").await?;
        click(desktop,"[data-option-value='guide']").await?;
        confirmation(desktop).await?;
        click(desktop,"#button-confirmation-cancel").await?;
        idle(desktop).await?; attempts(state,0,0)?;

        stage="confirmed exact action URL and duplicate pending";
        open(desktop,"pick").await?;
        checked(desktop,r#"(()=>{window.__overflowPending=document.querySelector('[data-option-value="guide"]');window.__overflowPending.click();return {ok:true};})()"#).await?;
        confirmation(desktop).await?;
        click(desktop,".button-confirmation-submit").await?;
        wait_attempts(state,1,1).await?;
        wait_dom(desktop,r#"({ok:[...document.querySelectorAll('.block-overflow,.block-select,.block-button')].every(e=>e.disabled)})"#).await?;
        checked(desktop,r#"(()=>{window.__overflowPending.click();window.__overflowChoice.click();return {ok:true};})()"#).await?;
        wire(state,0,"pick","guide")?;
        fixture::complete(state); idle(desktop).await?; attempts(state,1,1)?;

        stage="unconfirmed action and full sibling state";
        open(desktop,"other").await?;
        click(desktop,"[data-option-value='save']").await?;
        wait_attempts(state,2,1).await?; wire(state,1,"other","save")?;
        fixture::complete(state);idle(desktop).await?;
        click(desktop,".block-button:not(.block-overflow)").await?;
        wait_attempts(state,3,1).await?; fixture::complete(state);idle(desktop).await?;

        stage="navigation retires open menu and confirmation";
        open(desktop,"pick").await?;
        fixture::navigate_away_back(state);
        wait_dom(desktop,r#"({ok:!document.querySelector('.message-overflow-menu')})"#).await?;
        idle(desktop).await?;
        open(desktop,"pick").await?;click(desktop,"[data-option-value='approve']").await?;confirmation(desktop).await?;
        fixture::navigate_away_back(state);
        wait_dom(desktop,r#"({ok:!document.getElementById('button-confirmation-form')})"#).await?;idle(desktop).await?;
        attempts(state,3,1)?;

        stage="canonical edit retires confirmation";
        open(desktop,"pick").await?;click(desktop,"[data-option-value='approve']").await?;confirmation(desktop).await?;
        fixture::change_initial(state);
        idle(desktop).await?;attempts(state,3,1)?;

        stage="uncertain choice never replays";
        open(desktop,"other").await?;click(desktop,"[data-option-value='link']").await?;
        wait_attempts(state,4,2).await?;wire(state,3,"other","link")?;
        fixture::uncertain(state);
        wait_dom(desktop,r#"({ok:document.querySelector('.block-overflow[data-action-id="other"]')?.disabled&&!document.querySelector('.block-overflow[data-action-id="pick"]')?.disabled})"#).await?;
        checked(desktop,r#"(()=>{const b=document.querySelector('.block-overflow[data-action-id="other"]');b.dispatchEvent(new MouseEvent('click',{bubbles:true}));document.querySelector('.toast button')?.click();return {ok:true};})()"#).await?;
        open(desktop,"pick").await?;
        let mut receipt=wait_dom(desktop,LAYOUT).await?;
        attempts(state,4,2)?;
        receipt["preparedRequests"]=json!(4);
        for flag in ["exactOptionPayload","fullSiblingState","confirmationCancelNoDispatch","urlAfterConfirmation","pendingDuplicateRejected","uncertainNoReplay","navigationRetiresMenuAndConfirmation","staleConfirmationRetired","keyboardMenu","noTransport"] {receipt[flag]=json!(true);}
        Ok(receipt)
    }.await;
    result.map_err(|error| format!("{stage}: {error}"))
}

fn attempts(state: Signal<ShellState>, count: usize, urls: usize) -> Result<(), String> {
    let shell = state.read();
    let fixture = shell
        .message_buttons
        .overflow_fixture
        .as_ref()
        .ok_or("Missing fixture")?;
    if fixture.attempts.len() != count
        || fixture.urls.len() != urls
        || !fixture.prepared_valid
        || shell.core.transport.is_some()
    {
        return Err(format!(
            "Expected {count} prepared requests/{urls} recorded URLs, got {}/{}",
            fixture.attempts.len(),
            fixture.urls.len()
        ));
    }
    Ok(())
}
async fn wait_attempts(state: Signal<ShellState>, count: usize, urls: usize) -> Result<(), String> {
    for _ in 0..60 {
        if attempts(state, count, urls).is_ok() {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    attempts(state, count, urls)
}
fn wire(state: Signal<ShellState>, index: usize, action: &str, value: &str) -> Result<(), String> {
    let shell = state.read();
    let fixture = shell
        .message_buttons
        .overflow_fixture
        .as_ref()
        .ok_or("Missing fixture")?;
    let request = &fixture.attempts[index];
    let option = &request["action"]["selected_option"];
    if request["action"]["type"] != "overflow"
        || request["action"]["action_id"] != action
        || option["value"] != value
        || option.as_object().is_none_or(|o| o.len() != 2)
        || !option["text"].is_object()
        || request["state"]["review"]["environment"]["selected_option"]["value"] != "prod"
        || request["state"]["choice"].get("pick").is_some()
        || request["state"]["review"].get("other").is_some()
    {
        return Err(format!("Wrong overflow wire: {request}"));
    }
    Ok(())
}
async fn open(desktop: &dioxus::desktop::DesktopContext, action: &str) -> Result<(), String> {
    click(
        desktop,
        &format!(".block-overflow[data-action-id='{action}']"),
    )
    .await?;
    wait_dom(desktop,r#"({ok:document.querySelectorAll('.message-overflow-menu [role="menuitem"]').length===2&&document.activeElement?.getAttribute('role')==='menuitem'})"#).await?;
    Ok(())
}
async fn idle(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    wait_dom(desktop,r#"({ok:document.querySelectorAll('.block-overflow').length===2&&!document.getElementById('button-confirmation-form')&&[...document.querySelectorAll('.block-overflow,.block-select,.block-button')].every(e=>!e.disabled)})"#).await?;
    Ok(())
}
async fn confirmation(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    wait_dom(desktop,r#"({ok:document.getElementById('button-confirmation-title')?.textContent==='Apply review action?'&&document.activeElement===document.getElementById('button-confirmation-cancel')})"#).await?;
    Ok(())
}
async fn click(desktop: &dioxus::desktop::DesktopContext, selector: &str) -> Result<(), String> {
    wait_dom(desktop,&format!(r#"(()=>{{const e=document.querySelector({});if(!e||e.disabled)return {{ok:false}};e.click();return {{ok:true}};}})()"#,serde_json::to_string(selector).unwrap())).await?;
    Ok(())
}
async fn wait_dom(
    desktop: &dioxus::desktop::DesktopContext,
    script: &str,
) -> Result<Value, String> {
    let mut last = Value::Null;
    for _ in 0..60 {
        last = eval(desktop, script).await?;
        if last["ok"] == true {
            return Ok(last);
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Err(format!(
        "Native overflow did not reach expected state: {last}"
    ))
}
async fn checked(desktop: &dioxus::desktop::DesktopContext, script: &str) -> Result<Value, String> {
    let result = eval(desktop, script).await?;
    if result["ok"] == true {
        Ok(result)
    } else {
        Err(format!("Native overflow assertion failed: {result}"))
    }
}
async fn eval(desktop: &dioxus::desktop::DesktopContext, script: &str) -> Result<Value, String> {
    let (send, mut receive) = tokio::sync::mpsc::unbounded_channel();
    desktop
        .webview
        .evaluate_script_with_callback(script, move |value| {
            let _ = send.send(value);
        })
        .map_err(|_| "Native evaluation request failed")?;
    let raw = tokio::time::timeout(std::time::Duration::from_secs(2), receive.recv())
        .await
        .map_err(|_| "Native evaluation timeout")?
        .ok_or("Native callback closed")?;
    let value: Value = serde_json::from_str(&raw).map_err(|_| "Invalid native callback")?;
    match value {
        Value::String(value) => {
            serde_json::from_str(&value).map_err(|_| "Invalid encoded callback".into())
        }
        value => Ok(value),
    }
}
const LAYOUT: &str = r#"(()=>{const m=document.querySelector('.message-overflow-menu');if(!m)return {ok:false};const r=m.getBoundingClientRect(),items=[...m.querySelectorAll('[role="menuitem"]')];const bounded=r.width>0&&r.left>=0&&r.right<=innerWidth&&r.top>=0&&r.bottom<=innerHeight;const reachable=items.every(e=>{const b=e.getBoundingClientRect();return e.contains(document.elementFromPoint(b.x+b.width/2,b.y+b.height/2));});const timeline=document.getElementById('message-timeline');const noOverflow=timeline.scrollWidth<=timeline.clientWidth+1;return {ok:bounded&&reachable&&noOverflow,bounded,reachable,noOverflow,viewport:[innerWidth,innerHeight]};})()"#;
