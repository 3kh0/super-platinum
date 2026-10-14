//! Signed, disconnected native static-select interaction proof. No live requests.
use crate::state::ShellState;
use dioxus::prelude::*;
use serde_json::{Value, json};

pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if std::env::var("SUPER_PLATINUM_MESSAGE_SELECT_CHECK").as_deref() != Ok("1")
        || !matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("message-static-select" | "message-static-select-narrow")
        )
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let receipt = match run(state, &desktop).await {
            Ok(receipt) => receipt,
            Err(error) => json!({"ok":false,"error":error}),
        };
        eprintln!("message-select-check: {receipt}");
    });
}

async fn run(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    settled(desktop, "Release A", "Production").await?;
    checked(
        desktop,
        r#"(()=>{const selects=[...document.querySelectorAll('.block-select')];
        window.__messageSelectInitial=selects;
        return {ok:selects.length===2&&document.querySelectorAll('.block-button').length===1&&
        [...selects[0].querySelectorAll('optgroup')].map(g=>g.label).join('|')==='Stable|Preview'&&
        selects.every(s=>s.getAttribute('aria-label')&&s.querySelector('option[value=""]'))};})()"#,
    )
    .await?;
    assert_attempts(state, 0)?;

    choose(desktop, "Choose release", "Release B").await?;
    confirmation(desktop).await?;
    assert_attempts(state, 0)?;
    checked(desktop,r#"({ok:document.querySelector('.block-select[aria-label="Choose release"]').selectedOptions[0].textContent==='Release A'})"#).await?;
    checked(desktop,r#"(()=>{document.getElementById('button-confirmation-cancel').click();return {ok:true};})()"#).await?;
    settled(desktop, "Release A", "Production").await?;
    checked(desktop,r#"({ok:window.__messageSelectInitial.every((s,i)=>s===document.querySelectorAll('.block-select')[i])})"#).await?;
    assert_attempts(state, 0)?;

    choose(desktop, "Choose release", "Release B").await?;
    confirmation(desktop).await?;
    checked(desktop,r#"(()=>{const submit=document.querySelector('#button-confirmation-form button[type="submit"]');submit.click();submit.click();return {ok:true};})()"#).await?;
    wait_dom(desktop,r#"({ok:[...document.querySelectorAll('.block-select')].length===2&&[...document.querySelectorAll('.block-select')].every(s=>s.disabled&&s.getAttribute('aria-busy')==='true')})"#).await?;
    assert_attempts(state, 1)?;
    // A synthetic change can reach the event handler even while the native menu
    // is disabled; the controller must reject it while the previous write waits.
    choose(desktop, "Choose environment", "Test").await?;
    settled(desktop, "Release B", "Production").await?;
    assert_attempts(state, 1)?;
    assert_wire_state(state, 0, "b", "prod", "static_select")?;

    choose(desktop, "Choose environment", "Test").await?;
    wait_attempts(state, 2).await?;
    settled(desktop, "Release B", "Test").await?;
    assert_wire_state(state, 1, "b", "test", "static_select")?;
    checked(desktop,r#"(()=>{const button=[...document.querySelectorAll('.block-button')].find(b=>b.textContent==='Submit choices');button.click();button.click();return {ok:true};})()"#).await?;
    wait_attempts(state, 3).await?;
    settled(desktop, "Release B", "Test").await?;
    assert_wire_state(state, 2, "b", "test", "button")?;

    checked(desktop,r#"(()=>{window.__messageSelectBeforeNavigation=[...document.querySelectorAll('.block-select')];return {ok:true};})()"#).await?;
    super::select_fixture::navigate_away_back(state);
    wait_dom(
        desktop,
        r#"({ok:document.querySelectorAll('.block-select').length===0})"#,
    )
    .await?;
    settled(desktop, "Release B", "Test").await?;
    checked(desktop,r#"({ok:window.__messageSelectBeforeNavigation.every(s=>!s.isConnected)&&[...document.querySelectorAll('.block-select')].every(s=>!window.__messageSelectBeforeNavigation.includes(s))})"#).await?;
    assert_attempts(state, 3)?;

    choose(desktop, "Choose release", "Release A").await?;
    confirmation(desktop).await?;
    super::select_fixture::change_options(state);
    settled(desktop, "Release B", "Test").await?;
    checked(desktop,r#"({ok:![...document.querySelector('.block-select[aria-label="Choose release"]').options].some(o=>o.textContent==='Release A')})"#).await?;
    assert_attempts(state, 3)?;
    // The stale-action feedback is covered above. Dismiss it before checking
    // control reachability so a narrow-window toast cannot hide a passing rect.
    checked(
        desktop,
        r#"(()=>{document.querySelector('.toast button')?.click();return {ok:true};})()"#,
    )
    .await?;
    wait_dom(desktop, r#"({ok:!document.querySelector('.toast')})"#).await?;
    let mut receipt = checked(desktop, LAYOUT).await?;
    receipt["preparedRequests"] = json!(3);
    receipt["cancelPreservedSelectionAndNode"] = json!(true);
    receipt["pendingDuplicateRejected"] = json!(true);
    receipt["nextButtonCarriedFullState"] = json!(true);
    receipt["navigationRemountRetainedSelections"] = json!(true);
    receipt["optionsEditCancelledConfirmation"] = json!(true);
    receipt["noTransport"] = json!(true);
    Ok(receipt)
}

fn assert_attempts(state: Signal<ShellState>, count: usize) -> Result<(), String> {
    let shell = state.read();
    let fixture = shell
        .message_buttons
        .select_fixture
        .as_ref()
        .ok_or("Missing select fixture")?;
    if fixture.attempts.len() != count || !fixture.prepared_valid || shell.core.transport.is_some()
    {
        return Err(format!(
            "Unexpected synthetic request count (expected {count}) or transport"
        ));
    }
    Ok(())
}
async fn wait_attempts(state: Signal<ShellState>, count: usize) -> Result<(), String> {
    for _ in 0..60 {
        if assert_attempts(state, count).is_ok() {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Err(format!("Synthetic request count did not reach {count}"))
}
fn assert_wire_state(
    state: Signal<ShellState>,
    index: usize,
    release: &str,
    environment: &str,
    kind: &str,
) -> Result<(), String> {
    let shell = state.read();
    let fixture = shell
        .message_buttons
        .select_fixture
        .as_ref()
        .ok_or("Missing select fixture")?;
    let receipt = fixture
        .attempts
        .get(index)
        .ok_or("Missing synthetic request")?;
    let release_option = json!({"text":{"type":"plain_text","text":if release == "b" {"Release B"} else {"Release A"},"emoji":true},"value":release});
    let environment_option = json!({"text":{"type":"plain_text","text":if environment == "test" {"Test"} else {"Production"},"emoji":true},"value":environment});
    let (block, action) = if index == 0 {
        ("choice", "pick")
    } else if kind == "static_select" {
        ("review", "environment")
    } else {
        ("review", "submit")
    };
    if receipt["state"]["choice"]["pick"]
        != json!({"type":"static_select","selected_option":release_option})
        || receipt["state"]["review"]["environment"]
            != json!({"type":"static_select","selected_option":environment_option})
        || receipt["action"]["type"] != kind
        || receipt["action"]["block_id"] != block
        || receipt["action"]["action_id"] != action
        || receipt["prepared_valid"] != true
        || (kind == "static_select"
            && receipt["action"]["selected_option"]
                != receipt["state"][block][action]["selected_option"])
    {
        return Err(format!(
            "Synthetic request {index} lost raw full state or action identity"
        ));
    }
    Ok(())
}
async fn choose(
    desktop: &dioxus::desktop::DesktopContext,
    label: &str,
    option: &str,
) -> Result<(), String> {
    let label = serde_json::to_string(label).unwrap();
    let option = serde_json::to_string(option).unwrap();
    checked(desktop,&format!(r#"(()=>{{const selects=[...document.querySelectorAll('.block-select')].filter(s=>s.getAttribute('aria-label')==={label});
        if(selects.length!==1)return {{ok:false,selectCount:selects.length}};const s=selects[0];const matches=[...s.options].filter(o=>o.textContent==={option});
        if(matches.length!==1)return {{ok:false,optionCount:matches.length}};s.focus({{preventScroll:true}});s.value=matches[0].value;
        s.dispatchEvent(new Event('change',{{bubbles:true}}));return {{ok:true}};}})()"#)).await?;
    Ok(())
}
async fn confirmation(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    wait_dom(desktop,r#"({ok:document.getElementById('button-confirmation-title')?.textContent==='Change release?'&&document.activeElement===document.getElementById('button-confirmation-cancel')})"#).await?;
    Ok(())
}
async fn settled(
    desktop: &dioxus::desktop::DesktopContext,
    release: &str,
    environment: &str,
) -> Result<(), String> {
    let release = serde_json::to_string(release).unwrap();
    let environment = serde_json::to_string(environment).unwrap();
    wait_dom(desktop,&format!(r#"(()=>{{const a=document.querySelector('.block-select[aria-label="Choose release"]'),b=document.querySelector('.block-select[aria-label="Choose environment"]');
        const button=[...document.querySelectorAll('.block-button')].find(b=>b.textContent==='Submit choices');return {{ok:!document.getElementById('button-confirmation-form')&&a&&!a.disabled&&b&&!b.disabled&&button&&!button.disabled&&
        a.selectedOptions[0]?.textContent==={release}&&b.selectedOptions[0]?.textContent==={environment}}};}})()"#)).await?;
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
        "Native select did not reach expected state: {last}"
    ))
}
async fn checked(desktop: &dioxus::desktop::DesktopContext, script: &str) -> Result<Value, String> {
    let receipt = eval(desktop, script).await?;
    if receipt["ok"] == true {
        Ok(receipt)
    } else {
        Err(format!("Native select assertion failed: {receipt}"))
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
const LAYOUT: &str = r#"(()=>{const timeline=document.getElementById('message-timeline');timeline.scrollTop=timeline.scrollHeight;
    const controls=[...timeline.querySelectorAll('.block-select,.block-button')],bounds=timeline.getBoundingClientRect();
    const eachVisible=controls.every(c=>{const r=c.getBoundingClientRect(),hit=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);return r.width>0&&r.height>0&&r.left>=bounds.left-1&&r.right<=bounds.right+1&&r.top>=bounds.top-1&&r.bottom<=bounds.bottom+1&&(hit===c||c.contains(hit));});
    const noOverflow=timeline.scrollWidth<=timeline.clientWidth+1&&[...timeline.querySelectorAll('.block-actions,.block-section')].every(e=>e.scrollWidth<=e.clientWidth+1);
    controls[0]?.focus({preventScroll:true});return {ok:controls.length===3&&eachVisible&&noOverflow,eachVisible,noOverflow,
    viewport:[innerWidth,innerHeight],timeline:{width:timeline.clientWidth,scrollWidth:timeline.scrollWidth,height:timeline.clientHeight}};
})()"#;
