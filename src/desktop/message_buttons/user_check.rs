//! Signed, disconnected ordinary-message user selection verification.
use super::user_fixture as fixture;
use crate::state::ShellState;
use dioxus::prelude::*;
use serde_json::{Value, json};

pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !fixture::enabled() || super::conversation_fixture::enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let receipt = match run(state, &desktop).await {
            Ok(value) => value,
            Err(error) => json!({"ok":false,"error":error}),
        };
        eprintln!("message-user-check: {receipt}");
    });
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let mut stage = "initial user controls";
    let result:Result<Value,String>=async {
        idle(desktop,"Ada Self","Choose follow-up reviewer").await?;
        wait_dom(desktop,CONTROL_LAYOUT).await?;
        stage="untouched button carries initial user and absent sibling";
        click(desktop,".block-button").await?;wait_attempts(state,1).await?;pending(desktop).await?;
        wire(state,0,"button","U00000001",None)?;
        fixture::complete(state);idle(desktop,"Ada Self","Choose follow-up reviewer").await?;

        stage="ordinary user eligibility includes bots self guests external; excludes special users";
        open(desktop,"pick").await?;
        checked(desktop,r#"(()=>{const ids=[...document.querySelectorAll('#shell-overlay-dialog [data-user-id]')].map(e=>e.dataset.userId).sort();return {ok:JSON.stringify(ids)===JSON.stringify(['U00000001','U00000002','U00000003','U00000004','U00000005','USLACKBOT','USLACKSECURITY'].sort())&&document.getElementById('message-user-availability')?.textContent==='Some app users are unavailable in this picker.',ids};})()"#).await?;
        stage="search and cancel never dispatch; keyboard moves to result";
        query(desktop,"Blair").await?;
        wait_dom(desktop,r#"({ok:document.querySelectorAll('#shell-overlay-dialog [data-user-id]').length===1&&!!document.querySelector('[data-user-id="U00000002"]')})"#).await?;
        checked(desktop,r#"(()=>{const e=document.getElementById('message-user-search');e.focus();e.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true,cancelable:true}));return {ok:true};})()"#).await?;
        wait_dom(desktop,r#"({ok:document.activeElement?.dataset.userId==='U00000002'})"#).await?;
        attempts(state,1)?;click(desktop,"#message-user-cancel").await?;
        idle(desktop,"Ada Self","Choose follow-up reviewer").await?;attempts(state,1)?;

        stage="confirmation cancel does not commit selection";
        open(desktop,"pick").await?;click(desktop,"[data-user-id='U00000002']").await?;confirmation(desktop).await?;
        checked(desktop,r#"({ok:document.querySelector('.block-users-select[data-action-id="pick"]').textContent.trim()==='Ada Self'})"#).await?;
        attempts(state,1)?;click(desktop,"#button-confirmation-cancel").await?;
        idle(desktop,"Ada Self","Choose follow-up reviewer").await?;

        stage="confirmed user commits once and locks sibling actions";
        open(desktop,"pick").await?;click(desktop,"[data-user-id='U00000002']").await?;confirmation(desktop).await?;
        checked(desktop,r#"(()=>{const go=document.querySelector('#button-confirmation-form button[type=submit]');go.click();go.click();return {ok:true};})()"#).await?;
        wait_attempts(state,2).await?;pending(desktop).await?;
        checked(desktop,r#"(()=>{document.querySelector('.block-users-select[data-action-id="other"]').click();return {ok:!document.getElementById('message-user-search')};})()"#).await?;
        attempts(state,2)?;wire(state,1,"users_select","U00000002",None)?;
        fixture::complete(state);idle(desktop,"Blair Reviewer","Choose follow-up reviewer").await?;

        stage="unconfirmed user and later button carry complete sibling state";
        open(desktop,"other").await?;click(desktop,"[data-user-id='U00000003']").await?;
        wait_attempts(state,3).await?;pending(desktop).await?;
        wire(state,2,"users_select","U00000002",Some("U00000003"))?;
        fixture::complete(state);idle(desktop,"Blair Reviewer","Casey Guest").await?;
        click(desktop,".block-button").await?;wait_attempts(state,4).await?;
        wire(state,3,"button","U00000002",Some("U00000003"))?;
        fixture::complete(state);idle(desktop,"Blair Reviewer","Casey Guest").await?;

        stage="navigation retires picker and preserves committed mounted values";
        open(desktop,"pick").await?;
        checked(desktop,r#"(()=>{window.__userOld=[document.querySelector('.block-users-select'),document.getElementById('message-user-search')];return {ok:window.__userOld.every(Boolean)};})()"#).await?;
        fixture::navigate_away_back(state);
        wait_dom(desktop,r#"({ok:window.__userOld.every(e=>!e.isConnected)&&!document.getElementById('message-user-search')})"#).await?;
        idle(desktop,"Blair Reviewer","Casey Guest").await?;attempts(state,4)?;

        stage="stale metadata retires confirmation without commit";
        open(desktop,"pick").await?;click(desktop,"[data-user-id='U00000004']").await?;confirmation(desktop).await?;
        fixture::change_initial(state);
        wait_dom(desktop,r#"({ok:!document.getElementById('button-confirmation-form')&&[...document.querySelectorAll('.block-users-select')].length===2&&[...document.querySelectorAll('.block-users-select')].every(e=>!e.disabled)})"#).await?;
        attempts(state,4)?;
        state.write().toast = None;
        wait_dom(desktop,r#"({ok:!document.querySelector('.toast')})"#).await?;
        stage="bounded normal or narrow picker geometry";
        open(desktop,"other").await?;
        let mut receipt=checked(desktop,LAYOUT).await?;
        receipt["preparedRequests"]=json!(4);
        for key in ["compactSingleRowControls","initialAndAbsentState","candidatePolicy","searchCancelNoDispatch","keyboardResultNavigation","confirmationCancelNoCommit","pendingDuplicateRejected","nextButtonCarriedFullState","navigationPickerRetired","navigationCommittedUsersRetained","staleConfirmationRetired","noTransport"] {receipt[key]=json!(true);}
        Ok(receipt)
    }.await;
    result.map_err(|error| format!("{stage}: {error}"))
}
fn attempts(state: Signal<ShellState>, count: usize) -> Result<(), String> {
    let shell = state.read();
    let fixture = shell
        .message_buttons
        .user_fixture
        .as_ref()
        .ok_or("Missing user fixture")?;
    if fixture.attempts.len() != count || !fixture.prepared_valid || shell.core.transport.is_some()
    {
        return Err(format!(
            "Expected {count} disconnected requests, got {}",
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
        .user_fixture
        .as_ref()
        .and_then(|f| f.attempts.get(index))
        .ok_or("Missing user request")?;
    let other_matches = match other {
        Some(id) => {
            request["state"]["review"].get("other")
                == Some(&json!({"type":"users_select","selected_user":id}))
        }
        None => request["state"]["review"].get("other").is_none(),
    };
    let (block, action, selected) = if index == 1 {
        ("choice", "pick", Some(primary))
    } else if index == 2 {
        ("review", "other", other)
    } else {
        ("review", "submit", None)
    };
    if request["state"]["choice"]["pick"] != json!({"type":"users_select","selected_user":primary})
        || !other_matches
        || request["state"]["review"]["environment"]
            != json!({"type":"static_select","selected_option":{"text":{"type":"plain_text","text":"Production"},"value":"prod"}})
        || request["action"]["type"] != kind
        || request["action"]["block_id"] != block
        || request["action"]["action_id"] != action
        || (kind == "users_select" && request["action"]["selected_user"] != json!(selected))
        || (index == 1 && request["action"].get("initial_user") != Some(&json!("U00000001")))
        || (index == 2 && request["action"].get("initial_user").is_some())
        || request["prepared_valid"] != true
    {
        return Err(format!(
            "Request {index} changed user identity/full state: {request}"
        ));
    }
    Ok(())
}
async fn click(desktop: &dioxus::desktop::DesktopContext, selector: &str) -> Result<(), String> {
    wait_dom(desktop,&format!(r#"(()=>{{const e=document.querySelector({});if(!e||e.disabled)return {{ok:false,present:!!e,disabled:e?.disabled}};e.click();return {{ok:true}};}})()"#,serde_json::to_string(selector).unwrap())).await?;
    Ok(())
}
async fn open(desktop: &dioxus::desktop::DesktopContext, action: &str) -> Result<(), String> {
    click(
        desktop,
        &format!(".block-users-select[data-action-id='{action}']"),
    )
    .await?;
    wait_dom(desktop,r#"({ok:!!document.getElementById('message-user-search')&&document.activeElement===document.getElementById('message-user-search')})"#).await?;
    Ok(())
}
async fn query(desktop: &dioxus::desktop::DesktopContext, value: &str) -> Result<(), String> {
    checked(desktop,&format!(r#"(()=>{{const e=document.getElementById('message-user-search');if(!e)return {{ok:false}};e.value={};e.dispatchEvent(new Event('input',{{bubbles:true}}));return {{ok:true}};}})()"#,serde_json::to_string(value).unwrap())).await?;
    Ok(())
}
async fn confirmation(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    wait_dom(desktop,r#"({ok:document.getElementById('button-confirmation-title')?.textContent==='Change reviewer?'&&document.activeElement===document.getElementById('button-confirmation-cancel')})"#).await?;
    Ok(())
}
async fn pending(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    wait_dom(desktop,r#"(()=>{const es=[...document.querySelectorAll('.block-select,.block-button')];return {ok:es.length===4&&es.every(e=>e.disabled)&&!document.getElementById('button-confirmation-form')&&!document.getElementById('message-user-search')};})()"#).await?;
    Ok(())
}
async fn idle(
    desktop: &dioxus::desktop::DesktopContext,
    primary: &str,
    other: &str,
) -> Result<(), String> {
    wait_dom(desktop,&format!(r#"(()=>{{const a=document.querySelector('.block-users-select[data-action-id="pick"]'),b=document.querySelector('.block-users-select[data-action-id="other"]');return {{ok:!!a&&!!b&&!document.getElementById('button-confirmation-form')&&!document.getElementById('message-user-search')&&a.textContent.trim()==={}&&b.textContent.trim()==={}&&[...document.querySelectorAll('.block-select,.block-button')].every(e=>!e.disabled),primary:a?.textContent,other:b?.textContent}};}})()"#,serde_json::to_string(primary).unwrap(),serde_json::to_string(other).unwrap())).await?;
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
        "Native users select did not reach expected state: {last}"
    ))
}
async fn checked(desktop: &dioxus::desktop::DesktopContext, script: &str) -> Result<Value, String> {
    let receipt = eval(desktop, script).await?;
    if receipt["ok"] == true {
        Ok(receipt)
    } else {
        Err(format!("Native users select assertion failed: {receipt}"))
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
const LAYOUT: &str = r#"(()=>{const d=document.getElementById('shell-overlay-dialog'),s=document.getElementById('message-user-search'),c=document.getElementById('message-user-cancel');const rect=d.getBoundingClientRect();const eachVisible=[s,c].every(e=>{const r=e.getBoundingClientRect();return r.width>0&&r.height>0&&r.left>=0&&r.right<=innerWidth&&r.top>=0&&r.bottom<=innerHeight;});const noOverflow=rect.left>=0&&rect.right<=innerWidth&&rect.top>=0&&rect.bottom<=innerHeight&&d.scrollWidth<=d.clientWidth+1;const searchStyle=getComputedStyle(s),compactSearch=Math.abs(s.getBoundingClientRect().height-32)<=1&&searchStyle.getPropertyValue("-webkit-appearance")==="none";return {ok:eachVisible&&noOverflow&&compactSearch&&document.activeElement===s,compactSearch,eachVisible,noOverflow,keyboardFocused:document.activeElement===s,viewport:[innerWidth,innerHeight]};})()"#;

const CONTROL_LAYOUT: &str = r#"(()=>{const controls=[...document.querySelectorAll('.block-users-select')].map(e=>{const r=e.getBoundingClientRect(),label=e.querySelector('span')?.getBoundingClientRect(),icon=e.querySelector('svg')?.getBoundingClientRect();return {height:r.height,labelHeight:label?.height,iconHeight:icon?.height,sameRow:!!label&&!!icon&&Math.abs((label.top+label.bottom)-(icon.top+icon.bottom))<=2,labelBeforeIcon:!!label&&!!icon&&label.right<=icon.left+1};});return {ok:controls.length===2&&controls.every(e=>Math.abs(e.height-32)<=1&&e.labelHeight<=32&&e.iconHeight<=32&&e.sameRow&&e.labelBeforeIcon),controls};})()"#;
