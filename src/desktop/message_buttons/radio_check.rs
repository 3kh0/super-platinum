//! Signed, disconnected native radio-group interaction proof. No live requests.
use crate::state::ShellState;
use dioxus::prelude::*;
use serde_json::{Value, json};

pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if std::env::var("SUPER_PLATINUM_MESSAGE_RADIO_CHECK").as_deref() != Ok("1")
        || !matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("message-radio-buttons" | "message-radio-buttons-narrow")
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
        eprintln!("message-radio-check: {receipt}");
    });
}

async fn run(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let mut stage = "initial state";
    let result: Result<Value, String> = async {
    settled(desktop, "Release A", "Production").await?;
    stage = "initial accessibility";
    checked(desktop, ACCESSIBILITY).await?;
    assert_attempts(state, 0)?;

    stage = "open release confirmation";
    choose(desktop, "Choose release", "Release B").await?;
    confirmation(desktop).await?;
    assert_attempts(state, 0)?;
    stage = "uncommitted choice remains checked during confirmation";
    checked(desktop,r#"(()=>{const checked=document.querySelector('.block-radio-group[data-block-id="choice"] input:checked');const actual=checked?document.getElementById(checked.getAttribute('aria-labelledby'))?.textContent:null;return {ok:actual==='Release A',checkedLabel:actual};})()"#).await?;
    stage = "cancel restores choice without remount";
    checked(desktop,r#"(()=>{document.getElementById('button-confirmation-cancel').click();return {ok:true};})()"#).await?;
    settled(desktop, "Release A", "Production").await?;
    checked(desktop,r#"({ok:window.__messageRadioInitial.every((s,i)=>s===document.querySelectorAll('.block-radio-group')[i])&&window.__messageRadioInitialInputs.every((s,i)=>s===document.querySelectorAll('.block-radio-group input')[i])})"#).await?;
    assert_attempts(state, 0)?;

    stage = "open release confirmation";
    choose(desktop, "Choose release", "Release B").await?;
    confirmation(desktop).await?;
    stage = "confirm once and pending duplicate lock";
    checked(desktop,r#"(()=>{const submit=document.querySelector('#button-confirmation-form button[type="submit"]');submit.click();submit.click();return {ok:true};})()"#).await?;
    wait_dom(desktop,r#"({ok:[...document.querySelectorAll('.block-radio-group')].length===2&&[...document.querySelectorAll('.block-radio-group')].every(s=>[...s.querySelectorAll('input')].every(r=>r.disabled)&&s.getAttribute('aria-busy')==='true')})"#).await?;
    assert_attempts(state, 1)?;
    // A synthetic change can reach the event handler even while the native menu
    // is disabled; the controller must reject it while the previous write waits.
    checked(desktop,r#"(()=>{const group=document.querySelector('.block-radio-group[data-block-id="review"]');const radio=[...group.querySelectorAll('input')].find(r=>document.getElementById(r.getAttribute('aria-labelledby')).textContent==='Test');radio.checked=true;radio.dispatchEvent(new Event('change',{bubbles:true}));return {ok:true};})()"#).await?;
    settled(desktop, "Release B", "Production").await?;
    assert_attempts(state, 1)?;
    assert_wire_state(state, 0, "b", "prod", "radio_buttons")?;

    stage = "environment change carries full state";
    choose(desktop, "Choose environment", "Test").await?;
    wait_attempts(state, 2).await?;
    settled(desktop, "Release B", "Test").await?;
    assert_wire_state(state, 1, "b", "test", "radio_buttons")?;
    stage = "button carries both committed choices";
    checked(desktop,r#"(()=>{const button=[...document.querySelectorAll('.block-button')].find(b=>b.textContent==='Submit choices');button.click();button.click();return {ok:true};})()"#).await?;
    wait_attempts(state, 3).await?;
    settled(desktop, "Release B", "Test").await?;
    assert_wire_state(state, 2, "b", "test", "button")?;

    stage = "navigation remount retention";
    checked(desktop,r#"(()=>{window.__messageRadioBeforeNavigation=[...document.querySelectorAll('.block-radio-group')];return {ok:true};})()"#).await?;
    super::radio_fixture::navigate_away_back(state);
    wait_dom(
        desktop,
        r#"({ok:document.querySelectorAll('.block-radio-group').length===0})"#,
    )
    .await?;
    settled(desktop, "Release B", "Test").await?;
    checked(desktop,r#"({ok:window.__messageRadioBeforeNavigation.every(s=>!s.isConnected)&&[...document.querySelectorAll('.block-radio-group')].every(s=>!window.__messageRadioBeforeNavigation.includes(s))})"#).await?;
    assert_attempts(state, 3)?;

    stage = "removed selected option has no checked radio";
    super::radio_fixture::hide_selected(state);
    wait_dom(desktop,r#"(()=>{const group=document.querySelector('.block-radio-group[data-block-id="choice"]');const inputs=group?[...group.querySelectorAll('input')]:[];
        return {ok:inputs.length===1&&inputs.every(r=>!r.checked&&!r.disabled)&&group.querySelector('.block-radio-label')?.textContent==='Release A'&&!document.getElementById('button-confirmation-form')};})()"#).await?;
    assert_attempts(state, 3)?;
    stage = "restored option retains choice despite changed initial";
    super::radio_fixture::restore_selected(state);
    settled(desktop, "Release B", "Test").await?;
    assert_attempts(state, 3)?;

    stage = "option edit cancels pending confirmation";
    choose(desktop, "Choose release", "Release A").await?;
    confirmation(desktop).await?;
    super::radio_fixture::change_options(state);
    settled(desktop, "Release B", "Test").await?;
    checked(desktop,r#"({ok:![...document.querySelectorAll('.block-radio-group[data-block-id="choice"] .block-radio-label')].some(o=>o.textContent==='Release A')})"#).await?;
    assert_attempts(state, 3)?;
    stage = "dismiss stale-action notice before final reachability";
    checked(desktop,r#"(()=>{const notice=document.querySelector('.toast');const close=notice?.querySelector('[aria-label="Dismiss notification"]');if(!notice?.textContent.includes('changed')||!close)return {ok:false};close.click();return {ok:true};})()"#).await?;
    wait_dom(desktop,r#"({ok:!document.querySelector('.toast')})"#).await?;

    stage = "empty-valued choice commits in the existing mount";
    super::radio_fixture::add_empty_choice(state);
    settled(desktop, "Release B", "Test").await?;
    checked(desktop,r#"(()=>{window.__messageRadioEmptyMount=document.querySelector('#message-timeline .block-radio-group[data-block-id="choice"]');return {ok:true};})()"#).await?;
    choose(desktop, "Choose release", "Empty release").await?;
    confirmation(desktop).await?;
    checked(desktop,r#"(()=>{document.querySelector('#button-confirmation-form button[type="submit"]').click();return {ok:true};})()"#).await?;
    wait_attempts(state, 4).await?;
    settled(desktop, "Empty release", "Test").await?;
    checked(desktop,r#"({ok:window.__messageRadioEmptyMount===document.querySelector('#message-timeline .block-radio-group[data-block-id="choice"]')&&window.__messageRadioEmptyMount.isConnected})"#).await?;
    assert_empty_wire_state(state, 3, "radio_buttons")?;

    stage = "simultaneous channel and thread copies have independent display";
    super::radio_fixture::open_parent_copy(state);
    wait_dom(desktop,r#"(()=>{const channel=document.querySelector('#message-timeline .block-radio-group[data-block-id="choice"]'),thread=document.querySelector('#thread-body .block-radio-group[data-block-id="choice"]');
        const selected=g=>{const r=g?.querySelector('input:checked');return r?document.getElementById(r.getAttribute('aria-labelledby'))?.textContent:null;};
        const a=channel?.querySelector('input:checked'),b=thread?.querySelector('input:checked');
        return {ok:channel===window.__messageRadioEmptyMount&&selected(channel)==='Empty release'&&selected(thread)==='Release B'&&a.name!==b.name&&a.id!==b.id&&!a.disabled&&!b.disabled,channelChoice:selected(channel),threadChoice:selected(thread)};})()"#).await?;
    assert_attempts(state, 4)?;
    checked(desktop,r#"(()=>{document.querySelector('button.thread-close').click();return {ok:true};})()"#).await?;
    wait_dom(desktop,r#"({ok:!document.getElementById('thread-body')})"#).await?;
    settled(desktop, "Empty release", "Test").await?;

    stage = "real remount applies empty saved-value initial fallback";
    super::radio_fixture::navigate_away_back(state);
    wait_dom(desktop,r#"({ok:document.querySelectorAll('.block-radio-group').length===0})"#).await?;
    settled(desktop, "Release B", "Test").await?;
    checked(desktop,r#"({ok:!window.__messageRadioEmptyMount.isConnected&&window.__messageRadioEmptyMount!==document.querySelector('#message-timeline .block-radio-group[data-block-id="choice"]')})"#).await?;
    assert_attempts(state, 4)?;
    stage = "remount display fallback does not rewrite shared wire selection";
    checked(desktop,r#"(()=>{[...document.querySelectorAll('.block-button')].find(b=>b.textContent==='Submit choices').click();return {ok:true};})()"#).await?;
    wait_attempts(state, 5).await?;
    settled(desktop, "Release B", "Test").await?;
    assert_empty_wire_state(state, 4, "button")?;
    assert_attempts(state, 5)?;
    stage = "restore final canonical options and layout";
    super::radio_fixture::change_options(state);
    settled(desktop, "Release B", "Test").await?;
    wait_dom(desktop,r#"({ok:document.querySelectorAll('.block-radio-group[data-block-id="choice"] input').length===1})"#).await?;
    assert_attempts(state, 5)?;
    stage = "final layout and reachability";
    let mut receipt = checked(desktop, LAYOUT).await?;
    receipt["preparedRequests"] = json!(5);
    receipt["cancelPreservedSelectionAndNode"] = json!(true);
    receipt["pendingDuplicateRejected"] = json!(true);
    receipt["nextButtonCarriedFullState"] = json!(true);
    receipt["navigationRemountRetainedSelections"] = json!(true);
    receipt["optionsEditCancelledConfirmation"] = json!(true);
    receipt["removedOptionRetainedSelection"] = json!(true);
    receipt["changedInitialDidNotResetSelection"] = json!(true);
    receipt["emptyChoiceRetainedByExistingMount"] = json!(true);
    receipt["simultaneousCopiesHaveIndependentDisplay"] = json!(true);
    receipt["emptySavedValueRemountUsedInitial"] = json!(true);
    receipt["remountPreservedEmptyWireSelection"] = json!(true);
    receipt["noTransport"] = json!(true);
    Ok(receipt)
    }.await;
    result.map_err(|error| format!("{stage}: {error}"))
}

fn assert_attempts(state: Signal<ShellState>, count: usize) -> Result<(), String> {
    let shell = state.read();
    let fixture = shell
        .message_buttons
        .radio_fixture
        .as_ref()
        .ok_or("Missing radio fixture")?;
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
        .radio_fixture
        .as_ref()
        .ok_or("Missing radio fixture")?;
    let receipt = fixture
        .attempts
        .get(index)
        .ok_or("Missing synthetic request")?;
    let release_option = json!({"text":{"type":"plain_text","text":if release == "b" {"Release B"} else {"Release A"},"emoji":true},"description":{"type":"plain_text","text":if release == "b" {"Preview release"} else {"Stable release"}},"value":release});
    let environment_option = json!({"text":{"type":"plain_text","text":if environment == "test" {"Test"} else {"Production"},"emoji":true},"value":environment});
    let (block, action) = if index == 0 {
        ("choice", "pick")
    } else if kind == "radio_buttons" {
        ("review", "environment")
    } else {
        ("review", "submit")
    };
    if receipt["state"]["choice"]["pick"]
        != json!({"type":"radio_buttons","selected_option":release_option})
        || receipt["state"]["review"]["environment"]
            != json!({"type":"radio_buttons","selected_option":environment_option})
        || receipt["action"]["type"] != kind
        || receipt["action"]["block_id"] != block
        || receipt["action"]["action_id"] != action
        || receipt["prepared_valid"] != true
        || (kind == "radio_buttons"
            && receipt["action"]["selected_option"]
                != receipt["state"][block][action]["selected_option"])
    {
        return Err(format!(
            "Synthetic request {index} lost raw full state or action identity"
        ));
    }
    Ok(())
}

fn assert_empty_wire_state(
    state: Signal<ShellState>,
    index: usize,
    kind: &str,
) -> Result<(), String> {
    let shell = state.read();
    let fixture = shell
        .message_buttons
        .radio_fixture
        .as_ref()
        .ok_or("Missing radio fixture")?;
    let receipt = fixture
        .attempts
        .get(index)
        .ok_or("Missing empty-value request")?;
    let empty =
        json!({"text":{"type":"plain_text","text":"Empty release","emoji":true},"value":""});
    let environment =
        json!({"text":{"type":"plain_text","text":"Test","emoji":true},"value":"test"});
    let (block, action) = if kind == "radio_buttons" {
        ("choice", "pick")
    } else {
        ("review", "submit")
    };
    if receipt["state"]["choice"]["pick"] != json!({"type":"radio_buttons","selected_option":empty})
        || receipt["state"]["review"]["environment"]
            != json!({"type":"radio_buttons","selected_option":environment})
        || receipt["action"]["type"] != kind
        || receipt["action"]["block_id"] != block
        || receipt["action"]["action_id"] != action
        || receipt["prepared_valid"] != true
        || (kind == "radio_buttons" && receipt["action"]["selected_option"] != empty)
    {
        return Err(format!(
            "Synthetic request {index} lost exact empty choice or other container state"
        ));
    }
    Ok(())
}
async fn choose(
    desktop: &dioxus::desktop::DesktopContext,
    group: &str,
    option: &str,
) -> Result<(), String> {
    let group = serde_json::to_string(group).unwrap();
    let option = serde_json::to_string(option).unwrap();
    checked(desktop,&format!(r#"(()=>{{const groups=[...document.querySelectorAll('.block-radio-group')].filter(g=>g.getAttribute('aria-label')==={group});
        if(groups.length!==1)return {{ok:false,groupCount:groups.length}};const radios=[...groups[0].querySelectorAll('input[type="radio"]')].filter(r=>document.getElementById(r.getAttribute('aria-labelledby'))?.textContent==={option});
        if(radios.length!==1||radios[0].disabled)return {{ok:false,radioCount:radios.length}};radios[0].focus({{preventScroll:true}});radios[0].click();return {{ok:true}};}})()"#)).await?;
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
    wait_dom(desktop,&format!(r#"(()=>{{const a=document.querySelector('.block-radio-group[data-block-id="choice"]'),b=document.querySelector('.block-radio-group[data-block-id="review"]');
        const selected=g=>{{const radios=[...g.querySelectorAll('input[type="radio"]')];const chosen=radios.filter(r=>r.checked);return radios.every(r=>!r.disabled)&&chosen.length===1?document.getElementById(chosen[0].getAttribute('aria-labelledby'))?.textContent:null;}};
        const button=[...document.querySelectorAll('.block-button')].find(b=>b.textContent==='Submit choices');return {{ok:!document.getElementById('button-confirmation-form')&&a&&b&&button&&!button.disabled&&selected(a)==={release}&&selected(b)==={environment}}};}})()"#)).await?;
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
        "Native radio did not reach expected state: {last}; expression: {script}"
    ))
}
async fn checked(desktop: &dioxus::desktop::DesktopContext, script: &str) -> Result<Value, String> {
    let receipt = eval(desktop, script).await?;
    if receipt["ok"] == true {
        Ok(receipt)
    } else {
        Err(format!(
            "Native radio assertion failed: {receipt}; expression: {script}"
        ))
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
    const controls=[...timeline.querySelectorAll('.block-radio-group,.block-button')],bounds=timeline.getBoundingClientRect();
    const eachVisible=controls.every(c=>{const r=c.getBoundingClientRect();return r.width>0&&r.height>0&&r.left>=bounds.left-1&&r.right<=bounds.right+1&&r.top>=bounds.top-1&&r.bottom<=bounds.bottom+1;});
    const noOverflow=timeline.scrollWidth<=timeline.clientWidth+1&&[...timeline.querySelectorAll('.block-actions,.block-section')].every(e=>e.scrollWidth<=e.clientWidth+1);
    const hitTargets=[...timeline.querySelectorAll('.block-radio-group input,.block-button')];
    const eachReachable=hitTargets.every(c=>{const r=c.getBoundingClientRect();const hit=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);return hit===c||c.contains(hit);});
    controls[0]?.querySelector('input')?.focus({preventScroll:true});return {ok:controls.length===3&&eachVisible&&noOverflow&&eachReachable,eachVisible,noOverflow,eachReachable,
    viewport:[innerWidth,innerHeight],timeline:{width:timeline.clientWidth,scrollWidth:timeline.scrollWidth,height:timeline.clientHeight}};
})()"#;

const ACCESSIBILITY: &str = r#"(()=>{const groups=[...document.querySelectorAll('.block-radio-group')];window.__messageRadioInitial=groups;
    const inputs=[...document.querySelectorAll('.block-radio-group input[type="radio"]')];window.__messageRadioInitialInputs=inputs;
    const groupNames=groups.map(g=>new Set([...g.querySelectorAll('input')].map(r=>r.name)));
    const releaseInputs=[...(groups[0]?.querySelectorAll('input')??[])];
    const descriptions=[...(groups[0]?.querySelectorAll('.block-radio-description')??[])].map(e=>e.textContent);
    const checks={counts:groups.length===2&&inputs.length===4&&document.querySelectorAll('.block-button').length===1,
        groupLabels:groups.every(g=>g.getAttribute('role')==='radiogroup'&&g.getAttribute('aria-label')),
        distinctNames:groupNames.length===2&&groupNames.every(names=>names.size===1)&&[...groupNames[0]][0]!==[...groupNames[1]][0],
        uniqueIds:new Set(inputs.map(r=>r.id)).size===inputs.length,
        optionLabels:inputs.every(r=>document.getElementById(r.getAttribute('aria-labelledby'))&&r.labels.length===1),
        descriptions:descriptions.join('|')==='Stable release|Preview release',
        descriptionLinks:releaseInputs.every(r=>document.getElementById(r.getAttribute('aria-describedby'))),
        inertLabels:!groups.some(g=>g.querySelector('a,button'))};
    return {ok:Object.values(checks).every(Boolean),checks,groupCount:groups.length,inputCount:inputs.length,
        names:groupNames.map(names=>[...names]),descriptions};})()"#;
