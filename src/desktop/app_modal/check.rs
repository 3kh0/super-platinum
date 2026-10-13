//! Disconnected native form interaction check; no OS input or Slack requests.

use dioxus::prelude::*;
use serde_json::{Value, json};
use super_platinum_core::block_actions::form::{InputKey, InputValue, parse_datetime_local};

use crate::state::ShellState;

pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if std::env::var("SUPER_PLATINUM_APP_MODAL_CHECK").as_deref() != Ok("1")
        || !matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("app-modal-form"
                | "app-modal-form-narrow"
                | "app-modal-lifecycle"
                | "app-modal-lifecycle-narrow")
        )
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let receipt = match run(state, &desktop).await {
            Ok(mut receipt) => {
                if std::env::var("SUPER_PLATINUM_FIXTURE")
                    .is_ok_and(|name| name.starts_with("app-modal-lifecycle"))
                {
                    match lifecycle(state, &desktop).await {
                        Ok(lifecycle) => receipt["lifecycle"] = lifecycle,
                        Err(error) => receipt = json!({"ok":false,"error":error}),
                    }
                }
                if receipt["ok"] == true
                    && std::env::var("SUPER_PLATINUM_APP_MODAL_LOWER").as_deref() == Ok("1")
                    && matches!(
                        std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
                        Ok("app-modal-form" | "app-modal-form-narrow")
                    )
                {
                    match checked(&desktop, LOWER_CHECK).await {
                        Ok(lower) => receipt["lower"] = lower,
                        Err(error) => receipt = json!({"ok":false,"error":error}),
                    }
                }
                receipt
            }
            Err(error) => json!({"ok":false,"error":error}),
        };
        eprintln!("app-modal-check: {receipt}");
    });
}

async fn run(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    wait_dom(
        desktop,
        r#"({ok:document.querySelectorAll('#app-modal-form .app-modal-field').length===9})"#,
    )
    .await?;
    eval(desktop,r#"(()=>{document.querySelector('#app-modal-form button[type=submit]').click();return {ok:true};})()"#).await?;
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    {
        let shell = state.read();
        let session = shell
            .app_modal
            .as_ref()
            .ok_or("Form closed during validation")?;
        if session.fixture_attempts != 0
            || session
                .current_form()
                .and_then(|f| f.fields.get(&key("name")))
                .is_none_or(|f| f.error.is_none())
        {
            return Err("Required field did not stop submission".into());
        }
    }
    checked(desktop,r#"(()=>{const field=document.querySelector('[data-block-id="name"] input');
        window.__appModalName=field;field.focus();field.value='Local release draft';
        field.dispatchEvent(new Event('input',{bubbles:true}));field.setSelectionRange(5,5);return {ok:true};})()"#).await?;
    wait_state(state, |form| {
        form.fields
            .get(&key("name"))
            .is_some_and(|f| f.dirty && f.value == InputValue::Text("Local release draft".into()))
    })
    .await?;
    super::fixture::remote_update(state);
    wait_dom(desktop,r#"(()=>{const name=document.querySelector('[data-block-id="name"] input');
        const note=document.querySelector('[data-block-id="note"] textarea');return {ok:note?.value==='Remote note',
        retained:name===window.__appModalName&&name.value==='Local release draft'&&name.selectionStart===5&&name.selectionEnd===5};})()"#).await?;
    let retention=checked(desktop,r#"(()=>{const name=document.querySelector('[data-block-id="name"] input');
        return {ok:name===window.__appModalName&&name.value==='Local release draft'&&name.selectionStart===5&&name.selectionEnd===5};})()"#).await?;
    checked(desktop, FILL_FIELDS).await?;
    wait_state(state, |form| {
        let value = |id: &str| form.fields.get(&key(id)).map(|f| &f.value);
        value("static") == Some(&InputValue::Selected(vec!["".into()]))
            && value("multi") == Some(&InputValue::Selected(vec!["web,mobile".into(), "".into()]))
            && value("checks")
                == Some(&InputValue::Selected(vec![
                    "docs".into(),
                    "announce".into(),
                ]))
            && value("radio") == Some(&InputValue::Selected(vec!["fast".into()]))
            && value("date") == Some(&InputValue::Date(Some("2026-10-20".into())))
            && value("time") == Some(&InputValue::Time(Some("14:45".into())))
            && value("datetime")
                == Some(&InputValue::DateTime(
                    parse_datetime_local("2026-10-20T10:15").unwrap(),
                ))
    })
    .await?;
    checked(desktop,r#"(()=>{document.querySelector('#app-modal-form button[type=submit]').click();return {ok:true};})()"#).await?;
    wait_dom(desktop,r#"({ok:document.querySelector('[data-block-id="name"] .app-modal-error')?.textContent==='Choose another release name.'})"#).await?;
    let mut receipt = checked(desktop, FINAL_CHECK).await?;
    let shell = state.read();
    let session = shell
        .app_modal
        .as_ref()
        .ok_or("Form closed after rejection")?;
    if session.fixture_attempts != 1
        || session
            .current_form()
            .and_then(|f| f.fields.get(&key("name")))
            .is_none_or(|f| f.value != InputValue::Text("Local release draft".into()))
    {
        return Err("Rejected submission did not retain local field state".into());
    }
    receipt["typingRetainedAcrossUpdate"] = retention["ok"].clone();
    receipt["coreChoicesPreserved"] = json!(true);
    receipt["requiredBlockedSubmission"] = json!(true);
    receipt["fixtureAttempts"] = json!(session.fixture_attempts);
    Ok(receipt)
}

async fn lifecycle(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    // A removed dirty option must return selected when it is reintroduced.
    super::fixture::change_options(state, true);
    wait_dom(
        desktop,
        r#"({ok:document.querySelector('[data-block-id="multi"] select')?.options.length===2})"#,
    )
    .await?;
    super::fixture::change_options(state, false);
    wait_dom(desktop, r#"(()=>{const field=document.querySelector('[data-block-id="multi"] select');return {ok:field?.options.length===3&&[...field.selectedOptions].map(o=>o.value).join('|')==='option-1|option-2'};})()"#).await?;
    checked(desktop, r#"(()=>{const field=document.querySelector('[data-block-id="name"] input');field.value='Final release draft';field.dispatchEvent(new Event('input',{bubbles:true}));return {ok:true};})()"#).await?;
    wait_state(state, |form| {
        form.fields.get(&key("name")).is_some_and(|field| {
            field.value == InputValue::Text("Final release draft".into()) && field.error.is_none()
        })
    })
    .await?;
    click_submit(desktop).await?;
    wait_review(desktop).await?;
    wait_state(state, |form| form.view_id == "V_FORM_REVIEW").await?;
    checked(desktop,r#"(()=>{const back=[...document.querySelectorAll('#app-modal-form footer button')].find(b=>b.textContent==='Back');back.click();return {ok:true};})()"#).await?;
    wait_dom(desktop,r#"({ok:document.querySelector('[data-block-id="name"] input')?.value==='Final release draft'})"#).await?;
    wait_state(state, |form| {
        form.view_id == "V_FORM_FIXTURE"
            && form.fields.get(&key("multi")).is_some_and(|field| {
                field.value == InputValue::Selected(vec!["web,mobile".into(), "".into()])
            })
    })
    .await?;
    click_submit(desktop).await?;
    wait_review(desktop).await?;
    click_submit(desktop).await?;
    wait_closed(state, desktop).await?;

    super::fixture::install(&mut state.write());
    wait_dom(
        desktop,
        r#"({ok:document.querySelectorAll('#app-modal-form .app-modal-field').length===9})"#,
    )
    .await?;
    checked(desktop,r#"(()=>{const field=document.querySelector('[data-block-id="name"] input');field.focus();field.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',code:'Escape',bubbles:true,cancelable:true}));return {ok:true};})()"#).await?;
    wait_closed(state, desktop).await?;

    super::fixture::install(&mut state.write());
    wait_dom(
        desktop,
        r#"({ok:document.querySelectorAll('#app-modal-form .app-modal-field').length===9})"#,
    )
    .await?;
    checked(desktop,r#"(()=>{const field=document.querySelector('[data-block-id="name"] input');field.value='Retained pending draft';field.dispatchEvent(new Event('input',{bubbles:true}));return {ok:true};})()"#).await?;
    wait_state(state, |form| {
        form.fields
            .get(&key("name"))
            .is_some_and(|field| field.value == InputValue::Text("Retained pending draft".into()))
    })
    .await?;
    super::fixture::set_status(state, true, false);
    wait_dom(desktop, LOCKED_CHECK).await?;
    checked(desktop,r#"({ok:[...document.querySelectorAll('#app-modal-form [role=status]')].some(e=>e.textContent==='Submitting…')})"#).await?;
    // DOM-dispatched edits still cannot mutate typed state while locked.
    checked(desktop,r#"(()=>{const field=document.querySelector('[data-block-id="name"] input');field.value='Blocked edit';field.dispatchEvent(new Event('input',{bubbles:true}));field.value='Retained pending draft';document.querySelector('#app-modal-form button[type=submit]').click();return {ok:true};})()"#).await?;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert_locked_state(state)?;
    super::fixture::set_status(state, false, true);
    wait_dom(desktop,r#"({ok:[...document.querySelectorAll('#app-modal-form [role=status]')].some(e=>e.textContent.includes('not been confirmed'))})"#).await?;
    let mut receipt = checked(desktop, LOCKED_CHECK).await?;
    assert_locked_state(state)?;
    receipt["optionReintroductionRetained"] = json!(true);
    receipt["pushBackClear"] = json!(true);
    receipt["escapeClosed"] = json!(true);
    receipt["pendingLocked"] = json!(true);
    receipt["uncertainLocked"] = json!(true);
    Ok(receipt)
}

fn assert_locked_state(state: Signal<ShellState>) -> Result<(), String> {
    let shell = state.read();
    let session = shell.app_modal.as_ref().ok_or("Locked form disappeared")?;
    if session.fixture_attempts != 0
        || session
            .current_form()
            .and_then(|form| form.fields.get(&key("name")))
            .is_none_or(|field| field.value != InputValue::Text("Retained pending draft".into()))
    {
        return Err("Locked form accepted an edit or submission".into());
    }
    Ok(())
}

async fn click_submit(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    checked(desktop,r#"(()=>{document.querySelector('#app-modal-form button[type=submit]').click();return {ok:true};})()"#).await?;
    Ok(())
}

async fn wait_review(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    wait_dom(desktop,r#"({ok:document.querySelector('#app-modal-title')?.textContent==='Review release'&&document.querySelector('#app-modal-form button[type=submit]')?.textContent==='Save release'&&document.getElementById('shell-overlay-dialog')?.contains(document.activeElement)})"#).await?;
    Ok(())
}

async fn wait_closed(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<(), String> {
    wait_dom(
        desktop,
        r#"({ok:!document.getElementById('app-modal-form')})"#,
    )
    .await?;
    if state
        .read()
        .app_modal
        .as_ref()
        .is_some_and(|session| !session.modal.stack().is_empty())
    {
        return Err("Closed form retained an active view".into());
    }
    Ok(())
}

const LOCKED_CHECK: &str = r#"(()=>{
    const dialog=document.getElementById('shell-overlay-dialog'),form=document.getElementById('app-modal-form');
    const body=form.querySelector('.app-modal-body');body.scrollTop=0;
    const controls=[...form.querySelectorAll('input,textarea,select')];
    const locked=controls.length>0&&controls.every(el=>el.disabled)&&form.querySelector('button[type=submit]').disabled;
    const close=dialog.querySelector('button[aria-label="Close app form"]');close.focus({preventScroll:true});
    const closeReachable=!close.disabled&&document.activeElement===close;
    const retained=form.querySelector('[data-block-id="name"] input').value==='Retained pending draft';
    const footer=form.querySelector('footer').getBoundingClientRect(),d=dialog.getBoundingClientRect();
    const noOverflow=dialog.scrollWidth<=dialog.clientWidth+1&&body.scrollWidth<=body.clientWidth+1;
    const footerVisible=footer.top>=d.top&&footer.bottom<=innerHeight+1&&footer.right<=innerWidth+1;
    return {ok:locked&&closeReachable&&retained&&noOverflow&&footerVisible,locked,closeReachable,retained,noOverflow,footerVisible,viewport:[innerWidth,innerHeight]};
})()"#;

fn key(block: &str) -> InputKey {
    InputKey {
        block_id: block.into(),
        action_id: "value".into(),
    }
}

async fn wait_state(
    state: Signal<ShellState>,
    ready: impl Fn(&super_platinum_core::block_actions::form::ModalForm) -> bool,
) -> Result<(), String> {
    for _ in 0..60 {
        if state
            .read()
            .app_modal
            .as_ref()
            .and_then(|s| s.current_form())
            .is_some_and(&ready)
        {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Err("Native field changes did not reach typed form state".into())
}

async fn wait_dom(
    desktop: &dioxus::desktop::DesktopContext,
    script: &str,
) -> Result<Value, String> {
    for _ in 0..60 {
        let receipt = eval(desktop, script).await?;
        if receipt["ok"] == true {
            return Ok(receipt);
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Err("Native form did not reach expected state".into())
}

async fn checked(desktop: &dioxus::desktop::DesktopContext, script: &str) -> Result<Value, String> {
    let receipt = eval(desktop, script).await?;
    if receipt["ok"] == true {
        Ok(receipt)
    } else {
        Err(format!("Native form assertion failed: {receipt}"))
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

const FILL_FIELDS: &str = r#"(()=>{
    const get=(id,selector)=>document.querySelector(`[data-block-id="${id}"] ${selector}`);
    const change=(field)=>field.dispatchEvent(new Event('change',{bubbles:true}));
    const single=get('static','select');single.value='option-2';change(single);
    const multi=get('multi','select');for(const option of multi.options)option.selected=['option-1','option-2'].includes(option.value);change(multi);
    for(const field of document.querySelectorAll('[data-block-id="checks"] input'))field.checked=true;
    change(get('checks','input'));
    const radio=get('radio','input[value="option-1"]');radio.checked=true;change(radio);
    for(const [id,value] of [['date','2026-10-20'],['time','14:45'],['datetime','2026-10-20T10:15']]){
        const field=get(id,'input');field.value=value;field.dispatchEvent(new Event('input',{bubbles:true}));
    }
    return {ok:true};
})()"#;

const FINAL_CHECK: &str = r#"(()=>{
    const dialog=document.getElementById('shell-overlay-dialog');
    const form=document.getElementById('app-modal-form');
    const body=form.querySelector('.app-modal-body');body.scrollTop=0;
    const name=form.querySelector('[data-block-id="name"] input');name.focus({preventScroll:true});
    const fields=[...dialog.querySelectorAll('button,input,textarea,select,[tabindex]')].filter(el=>!el.disabled&&el.tabIndex>=0&&el.getClientRects().length);
    const first=fields[0],last=fields.at(-1);last.focus();
    last.dispatchEvent(new KeyboardEvent('keydown',{key:'Tab',bubbles:true,cancelable:true}));
    const forwardTrap=document.activeElement===first;
    first.dispatchEvent(new KeyboardEvent('keydown',{key:'Tab',shiftKey:true,bubbles:true,cancelable:true}));
    const reverseTrap=document.activeElement===last;
    name.focus({preventScroll:true});body.scrollTop=0;
    const d=dialog.getBoundingClientRect(),footer=form.querySelector('footer').getBoundingClientRect();
    const controls=[...form.querySelectorAll('input,textarea,select')];
    const noOverflow=dialog.scrollWidth<=dialog.clientWidth+1&&body.scrollWidth<=body.clientWidth+1;
    const footerVisible=footer.top>=d.top&&footer.bottom<=innerHeight+1&&footer.right<=innerWidth+1;
    const retained=name.value==='Local release draft'&&form.querySelector('[data-block-id="note"] textarea').value==='Remote note';
    const error=form.querySelector('[data-block-id="name"] .app-modal-error');
    const errorLinked=name.getAttribute('aria-invalid')==='true'&&name.getAttribute('aria-describedby').split(' ').includes(error.id);
    const editable=controls.every(el=>!el.disabled);
    return {ok:forwardTrap&&reverseTrap&&noOverflow&&footerVisible&&retained&&errorLinked&&editable,
        forwardTrap,reverseTrap,noOverflow,footerVisible,retained,errorLinked,editable,
        fields:form.querySelectorAll('.app-modal-field').length,viewport:[innerWidth,innerHeight],
        dialog:{width:d.width,height:d.height},body:{width:body.clientWidth,scrollWidth:body.scrollWidth,height:body.clientHeight},
        footer:{top:footer.top,bottom:footer.bottom}};
})()"#;

const LOWER_CHECK: &str = r#"(()=>{
    const dialog=document.getElementById('shell-overlay-dialog'),form=document.getElementById('app-modal-form');
    const body=form.querySelector('.app-modal-body'),bounds=body.getBoundingClientRect();
    const inside=field=>{const r=field.getBoundingClientRect();return r.top>=bounds.top-1&&r.bottom<=bounds.bottom+1&&r.left>=bounds.left-1&&r.right<=bounds.right+1;};
    const fields=['date','time','datetime'].map(id=>form.querySelector(`[data-block-id="${id}"] input`));
    const individuallyVisible=fields.map(field=>{
        body.scrollTop+=field.getBoundingClientRect().top-bounds.top-8;
        field.focus({preventScroll:true});
        return inside(field)&&document.activeElement===field;
    });
    const valuesRetained=fields[0].value==='2026-10-20'&&fields[1].value==='14:45'&&fields[2].value==='2026-10-20T10:15';
    body.scrollTop=body.scrollHeight;
    fields[2].focus({preventScroll:true});
    const finalVisible=fields.map(inside);
    const atBottom=Math.abs(body.scrollHeight-body.clientHeight-body.scrollTop)<=1;
    const footer=form.querySelector('footer').getBoundingClientRect(),d=dialog.getBoundingClientRect();
    const footerVisible=footer.top>=d.top&&footer.bottom<=innerHeight+1&&footer.right<=innerWidth+1;
    const noOverflow=dialog.scrollWidth<=dialog.clientWidth+1&&body.scrollWidth<=body.clientWidth+1;
    return {ok:individuallyVisible.every(Boolean)&&valuesRetained&&finalVisible[2]&&atBottom&&footerVisible&&noOverflow,
        dateInspected:individuallyVisible[0],timeInspected:individuallyVisible[1],datetimeInspected:individuallyVisible[2],
        finalVisible:{date:finalVisible[0],time:finalVisible[1],datetime:finalVisible[2]},
        valuesRetained,atBottom,footerVisible,noOverflow,viewport:[innerWidth,innerHeight]};
})()"#;
