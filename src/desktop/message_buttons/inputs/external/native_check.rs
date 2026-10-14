//! Synthetic native ordinary multi-select evidence; no session, transport or app callback.
use super::*;
use crate::message_buttons::{external_fixture as recorder, multi_external_fixture as fixture};
use fixture::{capture, checked, click, wait_dom};
use serde_json::{Value, json};
use std::time::Duration;

pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !fixture::ordinary() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = tokio::time::timeout(Duration::from_secs(150), run(state, &desktop)).await;
        let receipt = match result {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => json!({"ok":false,"error":e}),
            Err(_) => json!({"ok":false,"error":"Multi external check timed out"}),
        };
        eprintln!("multi-external-select-check: {receipt}");
    });
}
async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let binding = fixture::wait_runner(state).await?;
    let mut captures = serde_json::Map::new();
    idle(d, "1 selected").await?;
    checked(d,"(()=>{const e=document.querySelector('.block-external-select'),r=e.getBoundingClientRect();return {ok:Math.abs(r.height-32)<=1};})()").await?;
    pristine(state)?;
    captures.insert("initial".into(), capture(d, "initial").await?);
    open(d).await?;
    query(d, "ab").await?;
    wait_dom(d,"({ok:document.querySelector('[data-external-search]')?.textContent.includes('at least 3')})").await?;
    tokio::time::sleep(Duration::from_millis(650)).await;
    if queries(state) != 0 {
        return Err("Below-minimum search dispatched".into());
    }
    captures.insert("short-query".into(), capture(d, "short-query").await?);
    state
        .write()
        .message_buttons
        .external_fixture
        .as_mut()
        .unwrap()
        .hold_suggestions = true;
    query(d, "rel").await?;
    tokio::time::sleep(Duration::from_millis(200)).await;
    if queries(state) != 0 {
        return Err("Search dispatched before trailing debounce".into());
    }
    wait_dom(d,"({ok:document.querySelector('[data-external-search]')?.textContent.includes('Loading options')})").await?;
    captures.insert("loading".into(), capture(d, "loading").await?);
    checked(d,"({ok:document.querySelector('[data-external-search]')?.textContent.includes('Loading options')&&document.querySelectorAll('[data-option-index]').length===0})").await?;
    state
        .write()
        .message_buttons
        .external_fixture
        .as_mut()
        .unwrap()
        .hold_suggestions = false;
    results(d).await?;
    let layout = wait_dom(d, LAYOUT).await?;
    captures.insert("groups".into(), capture(d, "groups").await?);
    click(d, "[data-option-index='0']").await?;
    selected(d, &["Installed release", "Release Alpha"]).await?;
    preview(d, "2 selected").await?;
    pristine(state)?;
    query(d, "empty").await?;
    wait_dom(d,"({ok:document.querySelector('[data-external-search]')?.textContent.includes('No matching options')})").await?;
    selected(d, &["Installed release", "Release Alpha"]).await?;
    pristine(state)?;
    captures.insert("staged".into(), capture(d, "staged").await?);
    click(d, "#external-cancel").await?;
    idle(d, "1 selected").await?;
    pristine(state)?;
    captures.insert("canceled".into(), capture(d, "canceled").await?);
    open(d).await?;
    query(d, "rel").await?;
    results(d).await?;
    click(d, "[data-option-index='1']").await?;
    click(d, "[data-option-index='0']").await?;
    selected(d, &["Installed release", "Release Beta", "Release Alpha"]).await?;
    wait_dom(d,"({ok:document.querySelector('#external-apply')?.disabled&&document.body.textContent.includes('at most 2')})").await?;
    query(d, "empty").await?;
    wait_dom(d,"({ok:document.querySelector('[data-external-search]')?.textContent.includes('No matching options')})").await?;
    selected(d, &["Installed release", "Release Beta", "Release Alpha"]).await?;
    pristine(state)?;
    captures.insert("over-max".into(), capture(d, "over-max").await?);
    click(d, "[aria-label='Remove Installed release']").await?;
    selected(d, &["Release Beta", "Release Alpha"]).await?;
    click(d, "#external-apply").await?;
    confirmation(d).await?;
    pristine(state)?;
    captures.insert("confirmation".into(), capture(d, "confirmation").await?);
    click(d, "#external-cancel").await?;
    selected(d, &["Release Beta", "Release Alpha"]).await?;
    pristine(state)?;
    captures.insert(
        "confirmation-canceled".into(),
        capture(d, "confirmation-canceled").await?,
    );
    click(d, "#external-apply").await?;
    confirmation(d).await?;
    double_apply(d).await?;
    wait_attempts(state, 1).await?;
    wire(
        state,
        0,
        &[("Release Beta", "beta"), ("Release Alpha", "alpha,desktop")],
        true,
    )?;
    recorder::complete(state, false);
    idle(d, "2 selected").await?;
    captures.insert("selected".into(), capture(d, "selected").await?);
    open(d).await?;
    selected(d, &["Release Beta", "Release Alpha"]).await?;
    click(d, "[aria-label='Remove Release Beta']").await?;
    click(d, "[aria-label='Remove Release Alpha']").await?;
    selected(d, &[]).await?;
    // Empty preview is local until the two explicit Apply steps finish.
    if committed(state)?["choice"]["pick"]["selected_options"]
        .as_array()
        .map(Vec::len)
        != Some(2)
    {
        return Err("Staged clear committed early".into());
    }
    click(d, "#external-apply").await?;
    confirmation(d).await?;
    double_apply(d).await?;
    wait_attempts(state, 2).await?;
    wire(state, 1, &[], true)?;
    recorder::complete(state, false);
    idle(d, "Find release").await?;
    captures.insert("cleared".into(), capture(d, "cleared").await?);

    open(d).await?;
    selected(d, &[]).await?;
    let old = owner(&state.read()).ok_or("Missing stale-owner ticket")?;
    let count = queries(state);
    query(d, "slow").await?;
    wait_queries(state, count + 1).await?;
    let stale_revision = old
        .query(&state.read())
        .ok_or("Missing pending stale query")?
        .revision;
    fixture::replace_initial(state)?;
    wait_dom(d, "({ok:!document.getElementById('external-apply')})").await?;
    tokio::time::sleep(Duration::from_millis(1200)).await;
    let Owner::Message(old_ticket) = old.clone() else {
        return Err("Wrong stale owner kind".into());
    };
    if old_ticket.current(&state.read()) {
        return Err("Canonical replacement retained old picker authority".into());
    }
    if shared::complete(
        &mut state.write(),
        &old,
        stale_revision,
        Ok(super_platinum_core::block_actions::external::ExternalSuggestions::Options(vec![])),
    ) {
        return Err("Stale result applied".into());
    }
    multiple::apply(state, old.id());
    attempts(state, 2)?;
    idle(d, "Find release").await?;
    captures.insert("stale-owner".into(), capture(d, "stale-owner").await?);

    open(d).await?;
    selected(d, &[]).await?;
    query(d, "error").await?;
    wait_dom(d,"({ok:document.querySelector('[data-external-search]')?.textContent.includes('wait')&&!!document.querySelector('[data-external-retry]')})").await?;
    let count = queries(state);
    query(d, "edited").await?;
    click(d, "#external-cancel").await?;
    idle(d, "Find release").await?;
    open(d).await?;
    query(d, "edited").await?;
    wait_dom(d,"({ok:!!document.querySelector('[data-external-retry]')&&document.querySelector('[data-external-retry]').disabled})").await?;
    if queries(state) != count {
        return Err("New visit bypassed Retry-After".into());
    }
    captures.insert("error".into(), capture(d, "error").await?);
    wait_dom(d,"({ok:!!document.querySelector('[data-external-retry]')&&!document.querySelector('[data-external-retry]').disabled})").await?;
    tokio::time::sleep(Duration::from_millis(650)).await;
    if queries(state) != count {
        return Err("Cooldown automatically replayed search".into());
    }
    click(d, "[data-external-retry]").await?;
    results(d).await?;
    if queries(state) != count + 1 {
        return Err("Explicit retry did not issue exactly once".into());
    }
    click(d, "#external-cancel").await?;
    idle(d, "Find release").await?;
    click(d, ".block-button").await?;
    wait_attempts(state, 3).await?;
    wire(state, 2, &[], false)?;
    recorder::complete(state, true);
    wait_dom(
        d,
        "({ok:[...document.querySelectorAll('.block-select,.block-button')].every(e=>e.disabled)})",
    )
    .await?;
    checked(d,"(()=>{document.querySelector('.block-button')?.click();document.querySelector('.block-external-select')?.click();return {ok:true};})()").await?;
    tokio::time::sleep(Duration::from_millis(650)).await;
    attempts(state, 3)?;
    let uncertain_layout = wait_dom(d, UNCERTAIN_LAYOUT).await?;
    captures.insert("uncertain".into(), capture(d, "uncertain").await?);
    let shell = state.read();
    let record = shell.message_buttons.external_fixture.as_ref().unwrap();
    if shell.core.transport.is_some() || shell.core.session.is_some() || !record.valid {
        return Err("Disconnected request contract failed".into());
    }
    Ok(
        json!({"ok":true,"fixture":binding["fixture"],"nonce":binding["nonce"],"pid":binding["pid"],"preparedRequests":3,"queries":record.queries,"attempts":record.attempts,
        "noTransport":true,"noSession":true,"syntheticInput":true,"exactRawWire":true,"orderedSelections":true,"explicitEmptyArray":true,"fullSiblingState":true,"minimumAndDebounce":true,"groupOrder":true,"selectionAcrossQueries":true,"stagedPreviewSeparateWire":true,"cancelRollback":true,"confirmationCancelStaged":true,"savedEmptyPriority":true,"overMaxCorrection":true,"staleOwnerRejected":true,"cooldownAcrossVisits":true,"cooldownExpiryWithoutReplay":true,"uncertainNoReplay":true,
        "uncertainControlsClear":uncertain_layout["ok"],"uncertainLayout":uncertain_layout,
        "compactSearch":layout["compactSearch"],"eachVisible":layout["eachVisible"],"hitTestedControls":layout["hitTestedControls"],"noOverflow":layout["noOverflow"],"viewport":layout["viewport"],"captures":captures}),
    )
}
async fn preview(d: &dioxus::desktop::DesktopContext, label: &str) -> Result<(), String> {
    wait_dom(
        d,
        &format!(
            "({{ok:document.querySelector('.block-external-select')?.textContent.trim()==={}}})",
            json!(label)
        ),
    )
    .await?;
    Ok(())
}
async fn idle(d: &dioxus::desktop::DesktopContext, label: &str) -> Result<(), String> {
    wait_dom(d,"({ok:!!document.querySelector('.block-external-select')&&!document.querySelector('.block-external-select').disabled&&!document.getElementById('external-apply')})").await?;
    preview(d, label).await
}
async fn open(d: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    click(d, ".block-external-select").await?;
    wait_dom(
        d,
        "({ok:!!document.querySelector('[data-external-search] input[type=search]')})",
    )
    .await?;
    Ok(())
}
async fn query(d: &dioxus::desktop::DesktopContext, text: &str) -> Result<(), String> {
    checked(d,&format!("(()=>{{const e=document.querySelector('[data-external-search] input[type=search]');e.focus();Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,{});e.dispatchEvent(new Event('input',{{bubbles:true}}));return {{ok:true}};}})()",json!(text))).await?;
    Ok(())
}
async fn results(d: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    wait_dom(d,"({ok:document.querySelectorAll('[data-option-index]').length===2&&[...document.querySelectorAll('.message-multi-group')].map(e=>e.textContent).join('|')==='Stable|Preview'})").await?;
    Ok(())
}
async fn selected(d: &dioxus::desktop::DesktopContext, labels: &[&str]) -> Result<(), String> {
    wait_dom(d,&format!("(()=>{{const actual=[...document.querySelectorAll('[data-external-search] .message-multi-selected-option > span')].map(e=>e.textContent);return {{ok:!!document.querySelector('[data-external-search]')&&JSON.stringify(actual)===JSON.stringify({}),actual}};}})()",json!(labels))).await?;
    Ok(())
}
async fn confirmation(d: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    wait_dom(d,"({ok:!document.querySelector('[data-external-search]')&&document.getElementById('external-title')?.textContent==='Change release?'})").await?;
    Ok(())
}
async fn double_apply(d: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    checked(d,"(()=>{const e=document.getElementById('external-apply');if(!e||e.disabled)return {ok:false};e.click();e.click();return {ok:true};})()").await?;
    Ok(())
}
fn queries(state: Signal<ShellState>) -> usize {
    state
        .read()
        .message_buttons
        .external_fixture
        .as_ref()
        .unwrap()
        .queries
        .len()
}
fn attempts(state: Signal<ShellState>, expected: usize) -> Result<(), String> {
    let actual = state
        .read()
        .message_buttons
        .external_fixture
        .as_ref()
        .unwrap()
        .attempts
        .len();
    if actual == expected {
        Ok(())
    } else {
        Err(format!("Expected {expected} actions, found {actual}"))
    }
}
async fn wait_queries(state: Signal<ShellState>, expected: usize) -> Result<(), String> {
    for _ in 0..100 {
        if queries(state) == expected {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    Err("Suggestion did not dispatch".into())
}
async fn wait_attempts(state: Signal<ShellState>, expected: usize) -> Result<(), String> {
    for _ in 0..100 {
        if attempts(state, expected).is_ok() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    attempts(state, expected)
}
fn committed(state: Signal<ShellState>) -> Result<Value, String> {
    let shell = state.read();
    let entry = shell
        .message_buttons
        .inputs
        .values()
        .find(|e| {
            e.model
                .fields
                .keys()
                .any(|k| k.block_id == "choice" && k.action_id == "pick")
        })
        .ok_or("Missing message input cache")?;
    Ok(entry
        .model
        .values()
        .map_err(|_| "Invalid message values")?
        .unwrap_or(Value::Null))
}
fn pristine(state: Signal<ShellState>) -> Result<(), String> {
    attempts(state, 0)?;
    if !committed(state)?.is_null() {
        return Err("Staged preview initialized shared wire state".into());
    }
    Ok(())
}
fn wire(
    state: Signal<ShellState>,
    index: usize,
    selected: &[(&str, &str)],
    action: bool,
) -> Result<(), String> {
    let shell = state.read();
    let attempt = &shell
        .message_buttons
        .external_fixture
        .as_ref()
        .unwrap()
        .attempts[index];
    let expected = json!(
        selected
            .iter()
            .map(|(text, value)| recorder::option(text, value))
            .collect::<Vec<_>>()
    );
    if attempt["state"]["choice"]["pick"]["selected_options"] != expected
        || attempt["state"]["review"]["environment"]["selected_option"]["value"] != "prod"
    {
        return Err("Ordered raw selection or sibling wire differed".into());
    }
    if action
        && (attempt["action"]["type"] != "multi_external_select"
            || attempt["action"]["selected_options"] != expected)
    {
        return Err("Action lost ordered raw options".into());
    }
    if attempt["fields"]["cursor"] != Value::Null {
        return Err("Invented pagination".into());
    }
    Ok(())
}
const LAYOUT: &str = r#"(()=>{const d=document.getElementById('shell-overlay-dialog'),s=document.querySelector('[data-external-search] input[type=search]'),c=document.getElementById('external-cancel'),a=document.getElementById('external-apply');const r=d.getBoundingClientRect(),sr=s.getBoundingClientRect();const compactSearch=Math.abs(sr.height-32)<=1;const es=[s,c,a,...document.querySelectorAll('[data-option-index]')];const eachVisible=es.every(e=>{const x=e.getBoundingClientRect();return x.width>0&&x.height>0&&x.left>=0&&x.right<=innerWidth&&x.top>=0&&x.bottom<=innerHeight});const hitTestedControls=es.every(e=>{const x=e.getBoundingClientRect(),h=document.elementFromPoint(x.left+x.width/2,x.top+x.height/2);return h===e||e.contains(h)});const noOverflow=r.left>=0&&r.right<=innerWidth&&r.top>=0&&r.bottom<=innerHeight&&d.scrollWidth<=d.clientWidth+1;return {ok:compactSearch&&eachVisible&&hitTestedControls&&noOverflow,compactSearch,eachVisible,hitTestedControls,noOverflow,viewport:[innerWidth,innerHeight]};})()"#;
const UNCERTAIN_LAYOUT: &str = r#"(()=>{
    const toast=document.getElementById('status-toast'),dialog=document.querySelector('#shell-overlay-dialog[aria-modal="true"]');
    const buttons=[document.getElementById('external-cancel'),document.getElementById('external-apply')];
    if(!toast||!dialog||buttons.some(e=>!e))return {ok:false,reason:'Missing uncertainty toast or picker footer'};
    const t=toast.getBoundingClientRect(),d=dialog.getBoundingClientRect();
    const visible=r=>r.width>0&&r.height>0&&r.left>=0&&r.right<=innerWidth&&r.top>=0&&r.bottom<=innerHeight;
    const toastVisible=toast.dataset.positioned==='true'&&getComputedStyle(toast).visibility==='visible'&&visible(t);
    const nonoverlap=t.right<=d.left||t.left>=d.right||t.bottom<=d.top||t.top>=d.bottom;
    const footerClear=buttons.every(e=>{const r=e.getBoundingClientRect(),hit=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);return !e.disabled&&visible(r)&&(hit===e||e.contains(hit));});
    const noOverflow=visible(d)&&dialog.scrollWidth<=dialog.clientWidth+1;
    const rect=r=>({left:r.left,top:r.top,right:r.right,bottom:r.bottom});
    return {ok:toastVisible&&nonoverlap&&footerClear&&noOverflow,toastVisible,nonoverlap,footerClear,noOverflow,toast:rect(t),dialog:rect(d),viewport:[innerWidth,innerHeight]};
})()"#;
