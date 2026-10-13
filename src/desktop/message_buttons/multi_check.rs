//! Disconnected mixed-message checkbox and multi-select interaction evidence.
use super::multi_fixture as fixture;
use crate::state::ShellState;
use dioxus::prelude::*;
use serde_json::{Value, json};

pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !fixture::enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let receipt = match run(state, &desktop).await {
            Ok(value) => value,
            Err(error) => json!({"ok":false,"error":error}),
        };
        eprintln!("message-multi-check: {receipt}");
    });
}
async fn run(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let mut stage = "initial";
    let result:Result<Value,String>=async {
        idle(desktop).await?;
        checkbox_values(desktop,&[]).await?;
        checked(desktop,r#"(()=>{const group=document.querySelector('.block-checkbox-group'),inputs=[...group.querySelectorAll('input')];window.__multiInitialInputs=inputs;return {ok:group.getAttribute('aria-label')&&inputs.length===2&&inputs.every(e=>e.labels.length===1)&&new Set(inputs.map(e=>e.id)).size===2&&!group.querySelector('a')};})()"#).await?;
        stage="first button preserves absent versus empty defaults";
        submit(desktop).await?;wait_attempts(state,1).await?;idle(desktop).await?;
        wire(state,0,None,vec![],"button")?;
        stage="checkbox cancel leaves original nodes and state";
        checkbox(desktop,"a").await?;confirmation(desktop).await?;checkbox_values(desktop,&[]).await?;
        click(desktop,"#button-confirmation-cancel").await?;idle(desktop).await?;
        checked(desktop,r#"({ok:window.__multiInitialInputs.every((e,i)=>e===document.querySelectorAll('.block-checkbox-group input')[i])})"#).await?;
        attempts(state,1)?;
        stage="checkbox confirm commits once and locks siblings";
        checkbox(desktop,"a").await?;confirmation(desktop).await?;confirm(desktop).await?;
        wait_attempts(state,2).await?;pending(desktop).await?;
        checked(desktop,r#"(()=>{const input=document.querySelector('.block-checkbox-group input[data-option-value="b"]');input.checked=true;input.dispatchEvent(new Event('change',{bubbles:true}));return {ok:true};})()"#).await?;
        idle(desktop).await?;checkbox_values(desktop,&["a"]).await?;attempts(state,2)?;
        wire(state,1,Some(vec![fixture::option("Alpha","a")]),vec![],"checkboxes")?;
        stage="checkbox retained raw sibling survives changed options and initial";
        fixture::change_checkbox_options(state);
        wait_dom(desktop,r#"({ok:document.querySelector('.block-checkbox-group')?.textContent.includes('Alpha refreshed')})"#).await?;
        checkbox_values(desktop,&["a"]).await?;
        checkbox(desktop,"b").await?;confirmation(desktop).await?;confirm(desktop).await?;
        wait_attempts(state,3).await?;idle(desktop).await?;
        wire(state,2,Some(vec![fixture::option("Alpha","a"),fixture::option("Beta","b")]),vec![],"checkboxes")?;
        stage="checkbox deselection emits explicit empty array";
        for (value,count) in [("a",4),("b",5)] {checkbox(desktop,value).await?;confirmation(desktop).await?;confirm(desktop).await?;wait_attempts(state,count).await?;idle(desktop).await?;}
        wire(state,4,Some(vec![]),vec![],"checkboxes")?;
        stage="saved empty array overrides new defaults after actual remount";
        checked(desktop,r#"(()=>{window.__multiBeforeRemount=document.querySelector('.block-checkbox-group');return {ok:true};})()"#).await?;
        fixture::navigate_away_back(state);
        wait_dom(desktop,r#"({ok:!document.querySelector('.block-checkbox-group')})"#).await?;
        idle(desktop).await?;checkbox_values(desktop,&[]).await?;
        checked(desktop,r#"({ok:!window.__multiBeforeRemount.isConnected&&window.__multiBeforeRemount!==document.querySelector('.block-checkbox-group')})"#).await?;
        attempts(state,5)?;
        stage="multi draft cancel and integrated confirmation";
        open_picker(desktop).await?;picker_values(desktop,&[]).await?;
        pick(desktop,"a").await?;picker_values(desktop,&["a"]).await?;
        click(desktop,"#message-multi-cancel").await?;idle(desktop).await?;attempts(state,5)?;
        open_picker(desktop).await?;picker_values(desktop,&[]).await?;
        stage="over-limit draft cannot dispatch";
        for value in ["a","b","c"] {pick(desktop,value).await?;}
        picker_values(desktop,&["a","b","c"]).await?;
        wait_dom(desktop,r#"({ok:document.getElementById('message-multi-apply')?.disabled&&!!document.querySelector('#message-multi-form [role=alert]')})"#).await?;
        checked(desktop,r#"(()=>{document.getElementById('message-multi-apply').click();return {ok:true};})()"#).await?;attempts(state,5)?;
        pick(desktop,"c").await?;
        wait_dom(desktop,r#"({ok:!document.getElementById('message-multi-apply')?.disabled})"#).await?;
        stage="Apply dispatches once without a second confirmation";
        checked(desktop,r#"(()=>{const apply=document.getElementById('message-multi-apply');apply.click();apply.click();return {ok:true};})()"#).await?;
        wait_attempts(state,6).await?;pending(desktop).await?;
        checked(desktop,r#"({ok:!document.getElementById('message-multi-form')&&!document.getElementById('button-confirmation-form')})"#).await?;
        idle(desktop).await?;
        let ab=vec![fixture::option("Alpha","a"),fixture::option("Beta","b")];
        wire(state,5,Some(vec![]),ab.clone(),"multi_static_select")?;
        stage="button includes both array controls and untouched single controls";
        submit(desktop).await?;wait_attempts(state,7).await?;idle(desktop).await?;
        wire(state,6,Some(vec![]),ab.clone(),"button")?;
        stage="changed multi initial affects mounted display without rewriting state";
        let before=checked(desktop,r#"({ok:true,revision:document.querySelector('.block-multi-select')?.dataset.inputRevision})"#).await?;
        fixture::change_multi_initial(state);
        wait_dom(desktop,&format!(r#"({{ok:document.querySelector('.block-multi-select')?.dataset.inputRevision!=={}}})"#,before["revision"])).await?;idle(desktop).await?;
        open_picker(desktop).await?;picker_values(desktop,&["c"]).await?;
        click(desktop,"#message-multi-cancel").await?;idle(desktop).await?;
        submit(desktop).await?;wait_attempts(state,8).await?;idle(desktop).await?;
        wire(state,7,Some(vec![]),ab.clone(),"button")?;
        stage="navigation draft: open picker before leaving channel";
        open_picker(desktop).await?;pick(desktop,"b").await?;picker_values(desktop,&["b","c"]).await?;
        stage="navigation draft: wait for old controls and picker to unmount";
        checked(desktop,r#"(()=>{window.__multiNavigationGroup=document.querySelector('.block-checkbox-group');window.__multiNavigationPicker=document.getElementById('message-multi-form');return {ok:!!window.__multiNavigationGroup&&!!window.__multiNavigationPicker};})()"#).await?;
        fixture::navigate_away_back(state);
        wait_dom(desktop,r#"({ok:!window.__multiNavigationGroup.isConnected&&!window.__multiNavigationPicker.isConnected&&!document.getElementById('message-multi-form')})"#).await?;
        stage="navigation draft: wait for enabled remounted controls";
        idle(desktop).await?;
        stage="navigation draft: open remounted picker and verify retained state";
        open_picker(desktop).await?;picker_values(desktop,&["a","b"]).await?;
        click(desktop,"#message-multi-cancel").await?;idle(desktop).await?;
        submit(desktop).await?;wait_attempts(state,9).await?;idle(desktop).await?;
        wire(state,8,Some(vec![]),ab,"button")?;
        stage="canonical option change retires a picker without dispatch";
        open_picker(desktop).await?;fixture::change_multi_options(state);
        wait_dom(desktop,r#"({ok:!document.getElementById('message-multi-form')})"#).await?;idle(desktop).await?;attempts(state,9)?;
        stage="retained removed option can be removed locally but Cancel preserves it";
        open_picker(desktop).await?;picker_values(desktop,&["b"]).await?;
        checked(desktop,r#"({ok:!!document.querySelector('.message-multi-selected button[data-option-value="a"]')&&!document.querySelector('#message-multi-form input[data-option-value="a"]')})"#).await?;
        click(desktop,".message-multi-selected button[data-option-value=\"a\"]").await?;
        wait_dom(desktop,r#"({ok:!document.querySelector('.message-multi-selected button[data-option-value="a"]')&&!document.querySelector('#message-multi-form input[data-option-value="a"]')&&!!document.querySelector('.message-multi-selected button[data-option-value="b"]')})"#).await?;
        click(desktop,"#message-multi-cancel").await?;idle(desktop).await?;attempts(state,9)?;
        open_picker(desktop).await?;
        checked(desktop,r#"({ok:!!document.querySelector('.message-multi-selected button[data-option-value="a"]')})"#).await?;
        click(desktop,"#message-multi-cancel").await?;idle(desktop).await?;
        stage="account replacement retires picker and draft";
        open_picker(desktop).await?;fixture::replace_account(state);
        wait_dom(desktop,r#"({ok:!document.getElementById('message-multi-form')})"#).await?;idle(desktop).await?;attempts(state,9)?;
        stage="untouched absent multi Apply preserves undefined selection";
        let before=checked(desktop,r#"({ok:true,revision:document.querySelector('.block-multi-select')?.dataset.inputRevision})"#).await?;
        fixture::reset_without_multi_initial(state);
        wait_dom(desktop,&format!(r#"({{ok:document.querySelector('.block-multi-select')?.dataset.inputRevision!=={}}})"#,before["revision"])).await?;idle(desktop).await?;
        open_picker(desktop).await?;picker_values(desktop,&[]).await?;
        click(desktop,"#message-multi-apply").await?;wait_attempts(state,10).await?;idle(desktop).await?;
        absent_wire(state,9)?;
        stage="large picker scroll, filtering, keyboard focus and cancel";
        stress_picker(state,desktop).await?;
        stage="final accessibility and bounded layout";
        checked(desktop,r#"(()=>{document.querySelector('.toast [aria-label="Dismiss notification"]')?.click();const timeline=document.getElementById('message-timeline');return {ok:timeline.scrollWidth<=timeline.clientWidth+1&&[...timeline.querySelectorAll('.block-actions,.block-section')].every(e=>e.scrollWidth<=e.clientWidth+1),viewport:[innerWidth,innerHeight]};})()"#).await?;
        // Leave the integrated picker visible for a useful normal/narrow capture.
        open_picker(desktop).await?;picker_values(desktop,&[]).await?;
        let receipt=checked(desktop,r#"(()=>{const form=document.getElementById('message-multi-form'),r=form.closest('[role=dialog]').getBoundingClientRect(),apply=document.getElementById('message-multi-apply'),cancel=document.getElementById('message-multi-cancel');const visible=e=>{const b=e.getBoundingClientRect(),hit=document.elementFromPoint(b.left+b.width/2,b.top+b.height/2);return b.width>0&&b.height>0&&b.top>=0&&b.bottom<=innerHeight&&(hit===e||e.contains(hit));};return {ok:r.left>=0&&r.right<=innerWidth&&r.top>=0&&r.bottom<=innerHeight&&visible(apply)&&visible(cancel)&&form.contains(document.activeElement),viewport:[innerWidth,innerHeight]};})()"#).await?;
        Ok(json!({"ok":true,"preparedRequests":10,"largePickerScrollAndFocus":true,"retainedSelectionRemoval":true,"untouchedAbsentApply":true,"absentAndEmptyDefaults":true,"checkboxConfirmation":true,"checkboxRetainedRaw":true,"emptyRemount":true,"multiCancel":true,"maxSelected":true,"integratedConfirmation":true,"pendingDuplicateRejected":true,"rawSiblingState":true,"initialDisplayNotWire":true,"navigationDraftRetired":true,"stalePickerRetired":true,"accountPickerRetired":true,"noTransport":true,"layout":receipt}))
    }.await;
    result.map_err(|error| {
        let shell=state.peek();
        format!("{stage}: {error}; fixture scope team={:?} channel={:?} channel_generation={} account_epoch={} loading={} picker={}",shell.core.active_team,shell.core.active_channel,shell.channel_generation,shell.core.account_epoch,shell.loading,super::inputs::multi::picker(&shell).is_some())
    })
}
async fn stress_picker(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<(), String> {
    let before=checked(desktop,r#"({ok:true,revision:document.querySelector('.block-multi-select')?.dataset.inputRevision})"#).await?;
    let previous = fixture::stress_picker(state).ok_or("Stress fixture unavailable")?;
    wait_dom(
        desktop,
        &format!(
            r#"({{ok:document.querySelector('.block-multi-select')?.dataset.inputRevision!=={}}})"#,
            before["revision"]
        ),
    )
    .await?;
    idle(desktop).await?;
    open_picker(desktop).await?;
    checked(desktop,r#"(()=>{const form=document.getElementById('message-multi-form'),dialog=form.closest('[role=dialog]'),content=form.querySelector('.message-multi-content'),apply=document.getElementById('message-multi-apply'),cancel=document.getElementById('message-multi-cancel');const visible=e=>{const r=e.getBoundingClientRect(),hit=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);return r.width>0&&r.height>0&&r.top>=0&&r.bottom<=innerHeight&&(hit===e||e.contains(hit));};const r=dialog.getBoundingClientRect();return {ok:form.querySelectorAll('input[type=checkbox]').length===80&&content.scrollHeight>content.clientHeight+100&&content.clientHeight>0&&r.left>=0&&r.right<=innerWidth&&r.top>=0&&r.bottom<=innerHeight&&visible(apply)&&visible(cancel),content:[content.clientHeight,content.scrollHeight],dialog:[r.top,r.bottom],footer:[apply.getBoundingClientRect().top,apply.getBoundingClientRect().bottom]};})()"#).await?;
    checked(desktop,r#"(()=>{const input=document.getElementById('message-multi-search');input.focus();input.value='Stress option 79';input.dispatchEvent(new Event('input',{bubbles:true}));return {ok:true};})()"#).await?;
    wait_dom(desktop,r#"(()=>{const form=document.getElementById('message-multi-form'),options=[...form.querySelectorAll('input[type=checkbox]')];return {ok:options.length===1&&options[0].dataset.optionValue==='stress-79'&&document.activeElement===document.getElementById('message-multi-search')};})()"#).await?;
    pick(desktop, "stress-79").await?;
    picker_values(desktop, &["stress-79"]).await?;
    checked(desktop,r#"(()=>{const dialog=document.getElementById('shell-overlay-dialog'),apply=document.getElementById('message-multi-apply'),close=dialog.querySelector('header button');apply.focus();apply.dispatchEvent(new KeyboardEvent('keydown',{key:'Tab',bubbles:true,cancelable:true}));const forward=document.activeElement===close;close.dispatchEvent(new KeyboardEvent('keydown',{key:'Tab',shiftKey:true,bubbles:true,cancelable:true}));return {ok:forward&&document.activeElement===apply};})()"#).await?;
    checked(desktop,r#"(()=>{const input=document.getElementById('message-multi-search');input.focus();input.value='';input.dispatchEvent(new Event('input',{bubbles:true}));return {ok:true};})()"#).await?;
    wait_dom(desktop,r#"({ok:document.querySelectorAll('#message-multi-form input[type=checkbox]').length===80&&document.activeElement===document.getElementById('message-multi-search')})"#).await?;
    picker_values(desktop, &["stress-79"]).await?;
    click(desktop, "#message-multi-cancel").await?;
    idle(desktop).await?;
    attempts(state, 10)?;
    wait_dom(
        desktop,
        r#"({ok:document.activeElement===document.querySelector('.block-multi-select')})"#,
    )
    .await?;
    let before=checked(desktop,r#"({ok:true,revision:document.querySelector('.block-multi-select')?.dataset.inputRevision})"#).await?;
    fixture::restore_picker(state, previous);
    wait_dom(
        desktop,
        &format!(
            r#"({{ok:document.querySelector('.block-multi-select')?.dataset.inputRevision!=={}}})"#,
            before["revision"]
        ),
    )
    .await?;
    idle(desktop).await?;
    attempts(state, 10)?;
    Ok(())
}

fn attempts(state: Signal<ShellState>, expected: usize) -> Result<(), String> {
    let shell = state.peek();
    let fixture = shell
        .message_buttons
        .multi_fixture
        .as_ref()
        .ok_or("Missing multi fixture")?;
    if fixture.attempts.len() != expected
        || !fixture.prepared_valid
        || shell.core.transport.is_some()
    {
        return Err(format!(
            "Expected {expected} prepared requests, got {}; valid={}",
            fixture.attempts.len(),
            fixture.prepared_valid
        ));
    }
    Ok(())
}
async fn wait_attempts(state: Signal<ShellState>, expected: usize) -> Result<(), String> {
    for _ in 0..40 {
        if state
            .peek()
            .message_buttons
            .multi_fixture
            .as_ref()
            .is_some_and(|f| f.attempts.len() >= expected)
        {
            return attempts(state, expected);
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    attempts(state, expected)
}
fn wire(
    state: Signal<ShellState>,
    index: usize,
    checks: Option<Vec<Value>>,
    targets: Vec<Value>,
    kind: &str,
) -> Result<(), String> {
    let shell = state.peek();
    let receipt = shell
        .message_buttons
        .multi_fixture
        .as_ref()
        .and_then(|f| f.attempts.get(index))
        .ok_or("Missing request receipt")?;
    let checks = match checks {
        Some(options) => json!({"type":"checkboxes","selected_options":options}),
        None => json!({"type":"checkboxes"}),
    };
    let expected = json!({"checks":{"flags":checks},"review":{"targets":{"type":"multi_static_select","selected_options":targets},"env":{"type":"static_select","selected_option":fixture::option("Production","prod")},"mode":{"type":"radio_buttons","selected_option":fixture::option("Normal","normal")}}});
    let action = &receipt["action"];
    if receipt["state"] != expected || action["type"] != kind {
        return Err(format!(
            "Request {index} lost full raw state or action type: {}",
            receipt
        ));
    }
    let selected = if kind == "checkboxes" {
        &expected["checks"]["flags"]["selected_options"]
    } else {
        &expected["review"]["targets"]["selected_options"]
    };
    if kind != "button" && action["selected_options"] != *selected {
        return Err(format!("Request {index} action/state selections differ"));
    }
    if kind == "checkboxes" && action.get("initial_options").is_some() {
        return Err("Checkbox action invented initial_options".into());
    }
    if kind == "multi_static_select"
        && (action["initial_options"] != json!([])
            || action["placeholder"] != json!({"type":"plain_text","text":"Choose targets"}))
    {
        return Err("Multi action lost initial_options or placeholder".into());
    }
    Ok(())
}
fn absent_wire(state: Signal<ShellState>, index: usize) -> Result<(), String> {
    let shell = state.peek();
    let receipt = shell
        .message_buttons
        .multi_fixture
        .as_ref()
        .and_then(|f| f.attempts.get(index))
        .ok_or("Missing untouched-absent request")?;
    let expected = json!({"checks":{"flags":{"type":"checkboxes","selected_options":[fixture::option("Beta","b")]}},"review":{"targets":{"type":"multi_static_select"},"env":{"type":"static_select","selected_option":fixture::option("Production","prod")},"mode":{"type":"radio_buttons","selected_option":fixture::option("Normal","normal")}}});
    if receipt["state"] != expected
        || receipt["action"]["type"] != "multi_static_select"
        || receipt["action"].get("selected_options").is_some()
        || receipt["action"].get("initial_options").is_some()
    {
        return Err(format!(
            "Untouched absent multi selection became empty/null: {receipt}"
        ));
    }
    Ok(())
}
async fn click(desktop: &dioxus::desktop::DesktopContext, selector: &str) -> Result<(), String> {
    // Retry only before acting. The first actionable evaluation clicks once and
    // returns success atomically; a missing dialog afterward must fail normally.
    wait_dom(desktop,&format!(r#"(()=>{{const e=document.querySelector({}),r=e?.getBoundingClientRect();const ready=!!e&&e.isConnected&&!e.disabled&&r.width>0&&r.height>0&&!e.closest('[hidden],[inert]');if(!ready)return {{ok:false,selector:{},present:!!e,connected:!!e?.isConnected,disabled:e?.disabled??null,title:e?.title??null,revision:e?.dataset.inputRevision??null,active:{{tag:document.activeElement?.tagName,id:document.activeElement?.id}},checkboxes:document.querySelectorAll('.block-checkbox-group input').length,dialog:document.getElementById('shell-overlay-dialog')?.dataset.dialogGeneration??null}};e.scrollIntoView({{block:'nearest'}});e.focus();e.click();return {{ok:true}};}})()"#,json!(selector),json!(selector))).await?;
    Ok(())
}
async fn checkbox(desktop: &dioxus::desktop::DesktopContext, value: &str) -> Result<(), String> {
    click(
        desktop,
        &format!(".block-checkbox-group input[data-option-value=\"{value}\"]"),
    )
    .await
}
async fn pick(desktop: &dioxus::desktop::DesktopContext, value: &str) -> Result<(), String> {
    click(
        desktop,
        &format!("#message-multi-form input[data-option-value=\"{value}\"]"),
    )
    .await
}
async fn checkbox_values(
    desktop: &dioxus::desktop::DesktopContext,
    values: &[&str],
) -> Result<(), String> {
    values_match(desktop, ".block-checkbox-group", values).await
}
async fn picker_values(
    desktop: &dioxus::desktop::DesktopContext,
    values: &[&str],
) -> Result<(), String> {
    values_match(desktop, "#message-multi-form", values).await
}
async fn values_match(
    desktop: &dioxus::desktop::DesktopContext,
    selector: &str,
    values: &[&str],
) -> Result<(), String> {
    let mut expected = values.to_vec();
    expected.sort();
    wait_dom(desktop,&format!(r#"(()=>{{const parent=document.querySelector({}),actual=[...(parent?.querySelectorAll('input:checked')??[])].map(e=>e.dataset.optionValue).sort();return {{ok:!!parent&&JSON.stringify(actual)===JSON.stringify({}),actual}};}})()"#,json!(selector),json!(expected))).await?;
    Ok(())
}
async fn submit(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    click(desktop, ".block-button").await
}
async fn open_picker(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    click(desktop, ".block-multi-select").await?;
    wait_dom(desktop,r#"(()=>{const form=document.getElementById('message-multi-form');return {ok:!!form&&document.getElementById('message-multi-title')?.textContent==='Apply targets?'&&document.getElementById('message-multi-apply')?.textContent==='Apply targets'&&document.getElementById('message-multi-cancel')?.textContent==='Cancel'&&form.contains(document.activeElement)&&!document.getElementById('button-confirmation-form')};})()"#).await?;
    Ok(())
}
async fn confirmation(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    wait_dom(desktop,r#"({ok:document.getElementById('button-confirmation-title')?.textContent==='Change flags?'&&document.activeElement===document.getElementById('button-confirmation-cancel')})"#).await?;
    Ok(())
}
async fn confirm(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    checked(desktop,r#"(()=>{const submit=document.querySelector('#button-confirmation-form button[type=submit]');submit.click();submit.click();return {ok:true};})()"#).await?;
    Ok(())
}
async fn idle(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    wait_dom(desktop,r#"(()=>{const inputs=[...document.querySelectorAll('.block-checkbox-group input')],multi=document.querySelector('.block-multi-select');return {ok:inputs.length===2&&inputs.every(e=>!e.disabled)&&!!multi&&!multi.disabled&&!document.getElementById('button-confirmation-form')&&!document.getElementById('message-multi-form')};})()"#).await?;
    Ok(())
}
async fn pending(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    wait_dom(desktop,r#"(()=>{const inputs=[...document.querySelectorAll('.block-checkbox-group input')],multi=document.querySelector('.block-multi-select'),single=document.querySelector('.block-static-select');return {ok:inputs.length===2&&inputs.every(e=>e.disabled)&&multi?.disabled&&document.querySelector('.block-button')?.disabled};})()"#).await?;
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
        "Native multi did not reach expected state: {last}; expression: {script}"
    ))
}
async fn checked(desktop: &dioxus::desktop::DesktopContext, script: &str) -> Result<Value, String> {
    let receipt = eval(desktop, script).await?;
    if receipt["ok"] == true {
        Ok(receipt)
    } else {
        Err(format!(
            "Native multi assertion failed: {receipt}; expression: {script}"
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
