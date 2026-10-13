//! Signed, disconnected ordinary-message time action verification.
use super::time_fixture as fixture;
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
        eprintln!("message-time-check: {receipt}");
    });
}
async fn run(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let mut stage = "initial time controls";
    let result: Result<Value,String> = async {
        idle(desktop, "09:30", "").await?;
        checked(desktop,r#"(()=>{const inputs=[...document.querySelectorAll('.block-timepicker')];window.__timeInitial=inputs;return {ok:inputs.length===2&&inputs.every(e=>e.type==='time'&&e.getAttribute('aria-label'))&&[...document.querySelectorAll('.block-timezone')].map(e=>e.textContent).join('|')==='America/New_York|Europe/Berlin'};})()"#).await?;
        stage = "untouched button carries initial time and timezone-only sibling";
        click(desktop,".block-button").await?;wait_attempts(state,1).await?;pending(desktop).await?;
        wire(state,0,"button","09:30",None)?;
        fixture::complete(state);idle(desktop,"09:30","").await?;

        stage = "invalid time and empty partial serialization cannot dispatch";
        // WebKit sanitizes an impossible programmatic time to empty. This tests
        // the event boundary used by partial native times, not trusted OS typing.
        checked(desktop,r#"(()=>{const e=document.querySelector('.block-timepicker[data-action-id="pick"]');e.value='25:00';const empty=e.value==='';e.dispatchEvent(new Event('change',{bubbles:true}));return {ok:empty};})()"#).await?;
        idle(desktop,"09:30","").await?;attempts(state,1)?;
        change(desktop,"pick","").await?;idle(desktop,"09:30","").await?;attempts(state,1)?;

        stage = "confirmation cancel does not commit";
        change(desktop,"pick","10:45").await?;confirmation(desktop).await?;
        checked(desktop,r#"({ok:document.querySelector('.block-timepicker[data-action-id="pick"]').value==='09:30'})"#).await?;
        attempts(state,1)?;click(desktop,"#button-confirmation-cancel").await?;
        idle(desktop,"09:30","").await?;
        checked(desktop,r#"({ok:window.__timeInitial.every((e,i)=>e===document.querySelectorAll('.block-timepicker')[i])})"#).await?;

        stage = "confirmed time commits once and locks sibling actions";
        change(desktop,"pick","10:45").await?;confirmation(desktop).await?;
        checked(desktop,r#"(()=>{const go=document.querySelector('#button-confirmation-form button[type=submit]');go.click();go.click();return {ok:true};})()"#).await?;
        wait_attempts(state,2).await?;pending(desktop).await?;
        change(desktop,"other","11:00").await?;
        // Require event handling to restore the old sibling value before the
        // checker releases the write; a queued event cannot slip past completion.
        wait_dom(desktop,r#"({ok:document.querySelector('.block-timepicker[data-action-id="other"]')?.value===''&&document.querySelector('.block-timepicker[data-action-id="other"]')?.disabled})"#).await?;
        pending(desktop).await?;attempts(state,2)?;wire(state,1,"timepicker","10:45",None)?;
        fixture::complete(state);idle(desktop,"10:45","").await?;attempts(state,2)?;

        stage = "unconfirmed time action carries full sibling state";
        change(desktop,"other","12:15").await?;wait_attempts(state,3).await?;
        pending(desktop).await?;wire(state,2,"timepicker","10:45",Some("12:15"))?;
        fixture::complete(state);idle(desktop,"10:45","12:15").await?;
        click(desktop,".block-button").await?;wait_attempts(state,4).await?;
        wire(state,3,"button","10:45",Some("12:15"))?;
        fixture::complete(state);idle(desktop,"10:45","12:15").await?;

        stage = "navigation retires uncommitted confirmation and remount keeps times";
        change(desktop,"pick","13:20").await?;confirmation(desktop).await?;
        checked(desktop,r#"(()=>{window.__timeBeforeNavigation=[...document.querySelectorAll('.block-timepicker')];window.__timeBeforeNavigationConfirm=document.getElementById('button-confirmation-form');return {ok:!!window.__timeBeforeNavigationConfirm};})()"#).await?;
        fixture::navigate_away_back(state);
        wait_dom(desktop,r#"({ok:window.__timeBeforeNavigation.every(e=>!e.isConnected)&&!window.__timeBeforeNavigationConfirm.isConnected&&!document.getElementById('button-confirmation-form')})"#).await?;
        idle(desktop,"10:45","12:15").await?;attempts(state,4)?;

        stage = "stale message metadata retires confirmation without write";
        change(desktop,"pick","13:20").await?;confirmation(desktop).await?;
        fixture::change_initial(state);
        wait_dom(desktop,r#"({ok:!document.getElementById('button-confirmation-form')&&[...document.querySelectorAll('.block-timepicker')].length===2&&[...document.querySelectorAll('.block-timepicker')].every(e=>!e.disabled)})"#).await?;
        attempts(state,4)?;
        stage = "normal or narrow bounded keyboard layout";
        let mut receipt=checked(desktop,LAYOUT).await?;
        receipt["preparedRequests"]=json!(4);
        for key in ["initialAndTimezoneState","invalidAndEmptyNoDispatch","confirmationCancelNoCommit","pendingDuplicateRejected","nextButtonCarriedFullState","navigationRemountRetainedTimes","navigationConfirmationRetired","staleConfirmationRetired","noTransport"] { receipt[key]=json!(true); }
        Ok(receipt)
    }.await;
    result.map_err(|error| format!("{stage}: {error}"))
}

fn attempts(state: Signal<ShellState>, count: usize) -> Result<(), String> {
    let shell = state.read();
    let fixture = shell
        .message_buttons
        .time_fixture
        .as_ref()
        .ok_or("Missing time fixture")?;
    if fixture.attempts.len() != count || !fixture.prepared_valid || shell.core.transport.is_some()
    {
        return Err(format!(
            "Expected {count} valid disconnected requests, got {}",
            fixture.attempts.len()
        ));
    }
    Ok(())
}
async fn wait_attempts(state: Signal<ShellState>, count: usize) -> Result<(), String> {
    for _ in 0..60 {
        if attempts(state, count).is_ok() {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    attempts(state, count)
}
fn wire(
    state: Signal<ShellState>,
    index: usize,
    kind: &str,
    primary: &str,
    other: Option<&str>,
) -> Result<(), String> {
    let shell = state.read();
    let request = shell
        .message_buttons
        .time_fixture
        .as_ref()
        .and_then(|f| f.attempts.get(index))
        .ok_or("Missing time request")?;
    let expected_primary =
        json!({"type":"timepicker","selected_time":primary,"timezone":"America/New_York"});
    // A timezone-only timepicker has initial state, but selected_time is absent.
    // Explicit null is reserved for a committed null action.
    let other_matches = match other {
        Some(time) => {
            request["state"]["review"].get("other")
                == Some(
                    &json!({"type":"timepicker","selected_time":time,"timezone":"Europe/Berlin"}),
                )
        }
        None => {
            request["state"]["review"].get("other")
                == Some(&json!({"type":"timepicker","timezone":"Europe/Berlin"}))
        }
    };
    let expected_environment = json!({"type":"static_select","selected_option":{"text":{"type":"plain_text","text":"Production"},"value":"prod"}});
    let (block, action, time) = if index == 1 {
        ("choice", "pick", Some(primary))
    } else if index == 2 {
        ("review", "other", other)
    } else {
        ("review", "submit", None)
    };
    if request["state"]["choice"]["pick"] != expected_primary
        || !other_matches
        || request["state"]["review"]["environment"] != expected_environment
        || request["action"]["type"] != kind
        || request["action"]["block_id"] != block
        || request["action"]["action_id"] != action
        || (kind == "timepicker" && request["action"]["selected_time"] != json!(time))
        || (kind == "timepicker"
            && request["action"]["timezone"]
                != if index == 1 {
                    "America/New_York"
                } else {
                    "Europe/Berlin"
                })
        || request["prepared_valid"] != true
    {
        return Err(format!(
            "Request {index} changed time identity/full state: {request}"
        ));
    }
    Ok(())
}
async fn change(
    desktop: &dioxus::desktop::DesktopContext,
    action: &str,
    value: &str,
) -> Result<(), String> {
    checked(desktop,&format!(r#"(()=>{{const e=document.querySelector('.block-timepicker[data-action-id='+{}+']');if(!e)return {{ok:false}};e.value={};e.dispatchEvent(new Event('change',{{bubbles:true}}));return {{ok:true}};}})()"#,serde_json::to_string(action).unwrap(),serde_json::to_string(value).unwrap())).await?;
    Ok(())
}
async fn click(desktop: &dioxus::desktop::DesktopContext, selector: &str) -> Result<(), String> {
    wait_dom(desktop,&format!(r#"(()=>{{const e=document.querySelector({});if(!e||e.disabled)return {{ok:false,present:!!e,disabled:e?.disabled}};e.click();return {{ok:true}};}})()"#,serde_json::to_string(selector).unwrap())).await?;
    Ok(())
}
async fn confirmation(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    wait_dom(desktop,r#"({ok:document.getElementById('button-confirmation-title')?.textContent==='Change review time?'&&document.activeElement===document.getElementById('button-confirmation-cancel')})"#).await?;
    Ok(())
}
async fn pending(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    wait_dom(desktop,r#"(()=>{const controls=[...document.querySelectorAll('.block-timepicker,.block-select,.block-button')];return {ok:controls.length===4&&controls.every(e=>e.disabled)&&!document.getElementById('button-confirmation-form')};})()"#).await?;
    Ok(())
}
async fn idle(
    desktop: &dioxus::desktop::DesktopContext,
    primary: &str,
    other: &str,
) -> Result<(), String> {
    wait_dom(desktop,&format!(r#"(()=>{{const a=document.querySelector('.block-timepicker[data-action-id="pick"]'),b=document.querySelector('.block-timepicker[data-action-id="other"]');return {{ok:!!a&&!!b&&!document.getElementById('button-confirmation-form')&&a.value==={}&&b.value==={}&&[...document.querySelectorAll('.block-timepicker,.block-select,.block-button')].every(e=>!e.disabled),primary:a?.value,other:b?.value}};}})()"#,serde_json::to_string(primary).unwrap(),serde_json::to_string(other).unwrap())).await?;
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
    Err(format!("Native time did not reach expected state: {last}"))
}
async fn checked(desktop: &dioxus::desktop::DesktopContext, script: &str) -> Result<Value, String> {
    let receipt = eval(desktop, script).await?;
    if receipt["ok"] == true {
        Ok(receipt)
    } else {
        Err(format!("Native time assertion failed: {receipt}"))
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
        .map_err(|_| "Native evaluation timed out")?
        .ok_or("Native callback closed")?;
    let value: Value = serde_json::from_str(&raw).map_err(|_| "Invalid native callback")?;
    match value {
        Value::String(value) => {
            serde_json::from_str(&value).map_err(|_| "Invalid encoded native callback".into())
        }
        value => Ok(value),
    }
}
const LAYOUT: &str = r#"(()=>{document.querySelector('.toast button')?.click();const timeline=document.getElementById('message-timeline');timeline.scrollTop=timeline.scrollHeight;const inputs=[...document.querySelectorAll('.block-timepicker')],controls=[...timeline.querySelectorAll('.block-timepicker,.block-select,.block-button')],bounds=timeline.getBoundingClientRect();const eachVisible=controls.every(e=>{const r=e.getBoundingClientRect();return r.width>0&&r.height>0&&r.left>=bounds.left-1&&r.right<=bounds.right+1&&r.top>=bounds.top-1&&r.bottom<=bounds.bottom+1;});const noOverflow=timeline.scrollWidth<=timeline.clientWidth+1&&[...timeline.querySelectorAll('.block-actions,.block-section')].every(e=>e.scrollWidth<=e.clientWidth+1);inputs[0]?.focus({preventScroll:true});return {ok:eachVisible&&noOverflow&&document.activeElement===inputs[0],eachVisible,noOverflow,keyboardFocused:document.activeElement===inputs[0],viewport:[innerWidth,innerHeight]};})()"#;
