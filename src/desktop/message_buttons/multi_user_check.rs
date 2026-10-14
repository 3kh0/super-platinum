//! Disconnected native staged multi-user selection and exact prepared-wire check.
use super::{
    multi_user_fixture as fixture,
    user_check::{checked, click, wait_dom},
    user_fixture as recorder,
};
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
        eprintln!("message-multi-user-check: {receipt}");
    });
}

async fn run(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let mut stage = "initial controls";
    let result:Result<Value,String>=async {
        idle(desktop,"1 selected").await?;
        wait_dom(desktop,super::user_check::CONTROL_LAYOUT).await?;
        click(desktop,".block-button").await?; wait_attempts(state,1).await?;
        wire(state,0,"button",&["U00000001"])?;
        recorder::complete(state);idle(desktop,"1 selected").await?;
        stage="required and maximum staged selection";
        open(desktop,"pick").await?;
        click(desktop,"[data-remove-user='U00000001']").await?;
        wait_dom(desktop,r#"({ok:document.getElementById('message-users-submit')?.disabled&&document.querySelector('.message-multi-status')?.textContent.includes('at least one')})"#).await?;
        for id in ["U00000001","U00000002","U00000003"] {click(desktop,&format!("[data-user-id='{id}']")).await?;}
        wait_dom(desktop,r#"({ok:document.getElementById('message-users-submit')?.disabled&&document.querySelector('.message-multi-status')?.textContent.includes('at most 2')})"#).await?;
        attempts(state,1)?;click(desktop,"#message-user-cancel").await?;idle(desktop,"1 selected").await?;
        stage="search retains draft and cancel does not dispatch";
        open(desktop,"pick").await?;query(desktop,"Blair").await?;
        wait_dom(desktop,r#"({ok:document.querySelectorAll('[data-user-id]').length===1&&!!document.querySelector('[data-user-id="U00000002"]')})"#).await?;
        click(desktop,"[data-user-id='U00000002']").await?;query(desktop,"Casey").await?;
        wait_dom(desktop,r#"({ok:!!document.querySelector('[data-user-id="U00000003"]')&&document.querySelectorAll('[data-remove-user]').length===2&&!document.getElementById('message-users-submit').disabled})"#).await?;
        click(desktop,"#message-user-cancel").await?;idle(desktop,"1 selected").await?;attempts(state,1)?;
        stage="apply commits once and locks siblings";
        open(desktop,"pick").await?;click(desktop,"[data-user-id='U00000002']").await?;
        checked(desktop,r#"(()=>{const e=document.getElementById('message-users-submit');e.click();e.click();return {ok:true};})()"#).await?;
        wait_attempts(state,2).await?;
        wait_dom(desktop,r#"({ok:!document.getElementById('message-user-search')&&[...document.querySelectorAll('.block-select,.block-button')].every(e=>e.disabled)})"#).await?;
        wire(state,1,"multi_users_select",&["U00000001","U00000002"])?;
        recorder::complete(state);idle(desktop,"2 selected").await?;
        stage="following button carries complete sibling state";
        click(desktop,".block-button").await?;wait_attempts(state,3).await?;
        wire(state,2,"button",&["U00000001","U00000002"])?;
        recorder::complete(state);idle(desktop,"2 selected").await?;
        stage="source change retires pending picker";
        open(desktop,"pick").await?;
        fixture::change_initial(state);
        wait_dom(desktop,r#"({ok:!document.getElementById('message-user-search')&&[...document.querySelectorAll('.block-select,.block-button')].every(e=>!e.disabled)})"#).await?;
        attempts(state,3)?;
        stage="normal or narrow geometry";open(desktop,"other").await?;
        let mut receipt=checked(desktop,r#"(()=>{const d=document.getElementById('shell-overlay-dialog'),s=document.getElementById('message-user-search'),c=document.getElementById('message-user-cancel'),a=document.getElementById('message-users-submit');const rect=d.getBoundingClientRect();const eachVisible=[s,c,a].every(e=>{const r=e.getBoundingClientRect();return r.width>0&&r.height>0&&r.left>=0&&r.right<=innerWidth&&r.top>=0&&r.bottom<=innerHeight;});const noOverflow=rect.left>=0&&rect.right<=innerWidth&&rect.top>=0&&rect.bottom<=innerHeight&&d.scrollWidth<=d.clientWidth+1;const hitTestedControls=[s,c,a].every(e=>{const r=e.getBoundingClientRect();const hit=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);return hit===e||e.contains(hit);});const keyboardFocused=document.activeElement===s;return {ok:eachVisible&&noOverflow&&hitTestedControls&&keyboardFocused&&!a.disabled,eachVisible,noOverflow,hitTestedControls,keyboardFocused,viewport:[innerWidth,innerHeight]};})()"#).await?;
        receipt["preparedRequests"]=json!(3);
        for key in ["compactSingleRowControls","initialAndAbsentState","stagedCancelNoDispatch","requiredAndMaximum","searchSelectionRetained","pendingDuplicateRejected","nextButtonCarriedFullState","stalePickerRetired","noTransport"] {receipt[key]=json!(true);}
        Ok(receipt)
    }.await;
    result.map_err(|error| format!("{stage}: {error}"))
}
fn attempts(state: Signal<ShellState>, count: usize) -> Result<(), String> {
    let shell = state.read();
    let f = shell
        .message_buttons
        .user_fixture
        .as_ref()
        .ok_or("Missing recorder")?;
    if f.attempts.len() != count || !f.prepared_valid || shell.core.transport.is_some() {
        return Err(format!(
            "Expected {count} valid disconnected requests, got {}",
            f.attempts.len()
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
    selected: &[&str],
) -> Result<(), String> {
    let shell = state.read();
    let request = &shell
        .message_buttons
        .user_fixture
        .as_ref()
        .unwrap()
        .attempts[index];
    if request["state"]["choice"]["pick"]
        != json!({"type":"multi_users_select","selected_users":selected})
        || request["state"]["review"].get("other").is_some()
        || request["state"]["review"]["environment"]
            != json!({"type":"static_select","selected_option":{"text":{"type":"plain_text","text":"Production"},"value":"prod"}})
        || request["action"]["type"] != kind
        || (kind == "multi_users_select"
            && (request["action"]["selected_users"] != json!(selected)
                || request["action"]["initial_users"] != json!(["U00000001"])))
    {
        return Err(format!("Wrong multi-user prepared payload: {request}"));
    }
    Ok(())
}
async fn open(desktop: &dioxus::desktop::DesktopContext, action: &str) -> Result<(), String> {
    click(
        desktop,
        &format!(".block-multi-users-select[data-action-id='{action}']"),
    )
    .await?;
    wait_dom(desktop,r#"({ok:!!document.getElementById('message-user-search')&&document.activeElement===document.getElementById('message-user-search')})"#).await?;
    Ok(())
}
async fn query(desktop: &dioxus::desktop::DesktopContext, text: &str) -> Result<(), String> {
    checked(desktop,&format!(r#"(()=>{{const e=document.getElementById('message-user-search');e.value={};e.dispatchEvent(new Event('input',{{bubbles:true}}));return {{ok:true}};}})()"#,serde_json::to_string(text).unwrap())).await?;
    Ok(())
}
async fn idle(desktop: &dioxus::desktop::DesktopContext, label: &str) -> Result<(), String> {
    wait_dom(desktop,&format!(r#"({{ok:document.querySelector('.block-multi-users-select[data-action-id="pick"]')?.textContent.trim()==={}&&!document.getElementById('message-user-search')&&[...document.querySelectorAll('.block-select,.block-button')].every(e=>!e.disabled)}})"#,serde_json::to_string(label).unwrap())).await?;
    Ok(())
}
