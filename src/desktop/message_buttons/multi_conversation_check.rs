//! Disconnected native staged multi-conversation selection and exact prepared-wire check.
use super::{
    multi_conversation_fixture as fixture,
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
        eprintln!("message-multi-conversation-check: {receipt}");
    });
}

async fn run(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let mut stage = "initial controls";
    let result:Result<Value,String>=async {
        idle(desktop,"1 selected").await?;
        let initial_controls = wait_dom(desktop,CONTROL_LAYOUT).await?;
        let initial = capture(desktop,"initial").await?;
        let generation={let shell=state.read();let message=&shell.core.workspaces["T1"].messages["C2"].messages[0];
            if shell.core.active_channel.as_deref()!=Some("C2") || message.blocks[1]["accessory"]["default_to_current_conversation"]!=true || message.blocks[1]["accessory"]["initial_conversations"]!=json!(["C00000001"]) || message.blocks[2]["elements"][0]["default_to_current_conversation"]!=true {return Err("Ordinary true flag fixture lost its distinct initial/source context".into());}
            shell.channel_generation};
        recorder::navigate_away_back(state);
        for _ in 0..80 {if state.read().channel_generation>=generation+2 && state.read().core.active_channel.as_deref()==Some("C2"){break;}tokio::time::sleep(std::time::Duration::from_millis(50)).await;}
        if state.read().channel_generation<generation+2{return Err("Navigation did not complete".into());}
        idle(desktop,"1 selected").await?;
        click(desktop,".block-button").await?; wait_attempts(state,1).await?;
        wire(state,0,"button",&["C00000001"])?;
        recorder::complete(state);idle(desktop,"1 selected").await?;
        stage="required and maximum staged selection";
        open(desktop,"pick").await?;
        checked(desktop,r#"(()=>{const ids=[...document.querySelectorAll('#shell-overlay-dialog [data-conversation-id]')].map(e=>e.dataset.conversationId);return {ok:['C00000001','U00000002','G00000002','G00000003','C00000004'].every(id=>ids.includes(id))&&['C00000005','D00000006','U00000009'].every(id=>!ids.includes(id)),ids};})()"#).await?;
        click(desktop,"[data-remove-user='C00000001']").await?;
        wait_dom(desktop,r#"({ok:document.getElementById('message-users-submit')?.disabled&&document.querySelector('.message-multi-status')?.textContent.includes('at least one')})"#).await?;
        for id in ["C00000001","U00000002","G00000002"] {click(desktop,&format!("[data-conversation-id='{id}']")).await?;}
        wait_dom(desktop,r#"({ok:document.getElementById('message-users-submit')?.disabled&&document.querySelector('.message-multi-status')?.textContent.includes('at most 2')})"#).await?;
        let maximum = capture(desktop,"maximum").await?;
        attempts(state,1)?;click(desktop,"#message-user-cancel").await?;idle(desktop,"1 selected").await?;
        stage="search retains draft and cancel does not dispatch";
        open(desktop,"pick").await?;query(desktop,"Blair").await?;
        wait_dom(desktop,r#"({ok:document.querySelectorAll('[data-conversation-id]').length>=1&&!!document.querySelector('[data-conversation-id="U00000002"]')})"#).await?;
        click(desktop,"[data-conversation-id='U00000002']").await?;query(desktop,"private-project").await?;
        wait_dom(desktop,r#"({ok:!!document.querySelector('[data-conversation-id="G00000002"]')&&document.querySelectorAll('[data-remove-user]').length===2&&!document.getElementById('message-users-submit').disabled})"#).await?;
        click(desktop,"#message-user-cancel").await?;idle(desktop,"1 selected").await?;attempts(state,1)?;
        stage="Apply confirmation cancel keeps saved state";
        open(desktop,"pick").await?;click(desktop,"[data-conversation-id='U00000002']").await?;
        click(desktop,"#message-users-submit").await?;
        wait_dom(desktop,r#"({ok:!!document.getElementById('button-confirmation-form')})"#).await?;
        click(desktop,"#button-confirmation-cancel").await?;idle(desktop,"1 selected").await?;attempts(state,1)?;
        stage="apply commits once and locks siblings";
        open(desktop,"pick").await?;click(desktop,"[data-conversation-id='U00000002']").await?;
        let selected = capture(desktop,"selected").await?;
        click(desktop,"#message-users-submit").await?;
        wait_dom(desktop,r#"({ok:!!document.getElementById('button-confirmation-form')})"#).await?;
        checked(desktop,r#"(()=>{const d=document.getElementById('shell-overlay-dialog'),es=[document.getElementById('button-confirmation-cancel'),document.querySelector('#button-confirmation-form button[type=submit]')];const r=d.getBoundingClientRect();return {ok:r.left>=0&&r.right<=innerWidth&&r.top>=0&&r.bottom<=innerHeight&&es.every(e=>{const r=e.getBoundingClientRect(),h=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);return r.width>0&&r.height>0&&(h===e||e.contains(h));})};})()"#).await?;
        let confirmation=capture(desktop,"confirmation").await?;
        checked(desktop,r#"(()=>{const e=document.querySelector('#button-confirmation-form button[type=submit]');e.click();e.click();return {ok:true};})()"#).await?;
        wait_attempts(state,2).await?;
        wait_dom(desktop,r#"({ok:!document.getElementById('message-user-search')&&[...document.querySelectorAll('.block-select,.block-button')].every(e=>e.disabled)})"#).await?;
        wire(state,1,"multi_conversations_select",&["C00000001","U00000002"])?;
        recorder::complete(state);idle(desktop,"2 selected").await?;
        let committed_controls = wait_dom(desktop,CONTROL_LAYOUT).await?;
        stage="optional empty clear is one committed ordered array";
        open(desktop,"other").await?;
        checked(desktop,r#"(()=>{const ids=[...document.querySelectorAll('#shell-overlay-dialog [data-conversation-id]')].map(e=>e.dataset.conversationId);return {ok:ids.includes('U00000002')&&ids.includes('G00000002')&&!ids.includes('C00000004')&&!ids.includes('U00000005')&&!ids.includes('USLACKBOT'),ids};})()"#).await?;
        click(desktop,"[data-conversation-id='U00000002']").await?;
        click(desktop,"[data-remove-user='U00000002']").await?;
        wait_dom(desktop,r#"({ok:document.querySelectorAll('[data-remove-user]').length===0&&!document.getElementById('message-users-submit').disabled})"#).await?;
        let cleared = capture(desktop,"cleared").await?;
        click(desktop,"#message-users-submit").await?;wait_attempts(state,3).await?;
        let clear_wire = state.read().message_buttons.user_fixture.as_ref().unwrap().attempts[2].clone();
        if clear_wire["action"]["selected_conversations"] != json!([]) || clear_wire["state"]["review"]["other"] != json!({"type":"multi_conversations_select","selected_conversations":[]}) { return Err("Explicit clear array missing".into()); }
        recorder::complete(state);idle(desktop,"2 selected").await?;
        stage="normal or narrow geometry";open(desktop,"other").await?;
        let mut receipt=checked(desktop,r#"(()=>{const d=document.getElementById('shell-overlay-dialog'),s=document.getElementById('message-user-search'),c=document.getElementById('message-user-cancel'),a=document.getElementById('message-users-submit');const rect=d.getBoundingClientRect();const eachVisible=[s,c,a].every(e=>{const r=e.getBoundingClientRect();return r.width>0&&r.height>0&&r.left>=0&&r.right<=innerWidth&&r.top>=0&&r.bottom<=innerHeight;});const noOverflow=rect.left>=0&&rect.right<=innerWidth&&rect.top>=0&&rect.bottom<=innerHeight&&d.scrollWidth<=d.clientWidth+1;const hitTestedControls=[s,c,a].every(e=>{const r=e.getBoundingClientRect();const hit=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);return hit===e||e.contains(hit);});const keyboardFocused=document.activeElement===s;return {ok:eachVisible&&noOverflow&&hitTestedControls&&keyboardFocused&&!a.disabled,eachVisible,noOverflow,hitTestedControls,keyboardFocused,viewport:[innerWidth,innerHeight]};})()"#).await?;
        click(desktop,"#message-user-cancel").await?;
        click(desktop,".block-button").await?;wait_attempts(state,4).await?;
        wire(state,3,"button",&["C00000001","U00000002"])?;
        recorder::complete_unknown(state);
        wait_dom(desktop,r#"({ok:[...document.querySelectorAll('.block-select,.block-button')].every(e=>e.disabled)})"#).await?;
        checked(desktop,r#"(()=>{document.querySelector('.block-button').click();return {ok:true};})()"#).await?;
        attempts(state,4)?;
        let uncertain = capture(desktop,"uncertain").await?;
        receipt["captures"]=json!({"initial":initial,"maximum":maximum,"selected":selected,"confirmation":confirmation,"cleared":cleared,"uncertain":uncertain});
        receipt["preparedRequests"]=json!(4);
        receipt["compactSingleRowControls"]=json!(true);
        receipt["ordinaryFlagUsesInitial"]=json!(true);
        receipt["navigationDoesNotSynthesizeDefault"]=json!(true);
        receipt["initialControls"]=initial_controls;
        receipt["committedControls"]=committed_controls;
        for key in ["allConversationKinds","filterEligibility","memberIdForwarded","confirmationHitTested","initialAndAbsentState","stagedCancelNoDispatch","requiredAndMaximum","searchSelectionRetained","pendingDuplicateRejected","nextButtonCarriedFullState","confirmationCancelNoCommit","optionalClearArray","uncertainNoReplay","noTransport"] {receipt[key]=json!(true);}
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
        != json!({"type":"multi_conversations_select","selected_conversations":selected})
        || (index < 2 && request["state"]["review"].get("other").is_some())
        || (index >= 2
            && request["state"]["review"]["other"]
                != json!({"type":"multi_conversations_select","selected_conversations":[]}))
        || request["state"]["review"]["environment"]
            != json!({"type":"static_select","selected_option":{"text":{"type":"plain_text","text":"Production"},"value":"prod"}})
        || request["action"].get("response_url_enabled").is_some()
        || request["action"]["type"] != kind
        || (kind == "multi_conversations_select"
            && (request["action"]["selected_conversations"] != json!(selected)
                || request["action"]["initial_conversations"] != json!(["C00000001"])))
    {
        return Err(format!(
            "Wrong multi-conversation prepared payload: {request}"
        ));
    }
    Ok(())
}
async fn open(desktop: &dioxus::desktop::DesktopContext, action: &str) -> Result<(), String> {
    click(
        desktop,
        &format!(".block-multi-conversations-select[data-action-id='{action}']"),
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
    wait_dom(desktop,&format!(r#"({{ok:document.querySelector('.block-multi-conversations-select[data-action-id="pick"]')?.textContent.trim()==={}&&!document.getElementById('message-user-search')&&[...document.querySelectorAll('.block-select,.block-button')].every(e=>!e.disabled)}})"#,serde_json::to_string(label).unwrap())).await?;
    Ok(())
}

async fn capture(
    d: &dioxus::desktop::DesktopContext,
    stage: &str,
) -> Result<Option<String>, String> {
    let Some(dir) = std::env::var_os("SUPER_PLATINUM_MULTI_CONVERSATION_CAPTURE_DIR") else {
        return Ok(None);
    };
    checked(d,"(()=>{window.__channelsPaint=false;requestAnimationFrame(()=>requestAnimationFrame(()=>window.__channelsPaint=true));return {ok:true};})()").await?;
    wait_dom(d, "({ok:window.__channelsPaint===true})").await?;
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap();
    let path = std::path::PathBuf::from(dir).join(format!("{name}-{stage}.png"));
    let (path, _, _) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), d).await?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

const CONTROL_LAYOUT: &str = r#"(()=>{const controls=[...document.querySelectorAll('.block-multi-conversations-select')].map(e=>{const r=e.getBoundingClientRect(),label=e.querySelector('span')?.getBoundingClientRect(),icon=e.querySelector('svg')?.getBoundingClientRect();return {height:r.height,labelHeight:label?.height,iconHeight:icon?.height,sameRow:!!label&&!!icon&&Math.abs((label.top+label.bottom)-(icon.top+icon.bottom))<=2,labelBeforeIcon:!!label&&!!icon&&label.right<=icon.left+1};});return {ok:controls.length===2&&controls.every(e=>Math.abs(e.height-32)<=1&&e.labelHeight<=32&&e.iconHeight<=32&&e.sameRow&&e.labelBeforeIcon),controls};})()"#;
