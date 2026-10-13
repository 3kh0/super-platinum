//! Native DOM interactions for exact disconnected message-button fixtures.
use crate::state::ShellState;
use dioxus::prelude::*;
use serde_json::{Value, json};
use super_platinum_core::block_actions::form::{InputKey, InputValue};

pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !super::fixture::enabled() && !super::fixture::metadata_enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let result = if super::fixture::metadata_enabled() {
            run_metadata(state, &desktop).await
        } else {
            run(state, &desktop).await
        };
        let receipt = match result {
            Ok(receipt) => receipt,
            Err(error) => json!({"ok":false,"error":error}),
        };
        eprintln!("message-buttons-check: {receipt}");
    });
}

async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    wait_dom(
        desktop,
        r#"({ok:document.querySelectorAll('.block-button').length===5})"#,
    )
    .await?;
    checked(
        desktop,
        r#"(()=>{const buttons=[...document.querySelectorAll('.block-button')];return {ok:
        buttons.filter(b=>b.textContent==='Open').length===2&&
        buttons.filter(b=>b.textContent.startsWith('Ambiguous')).length===2&&
        buttons.filter(b=>b.textContent.startsWith('Ambiguous')).every(b=>b.disabled)&&
        buttons.filter(b=>!b.textContent.startsWith('Ambiguous')).every(b=>!b.disabled)&&
        !document.body.textContent.includes('Hidden recipient')};})()"#,
    )
    .await?;
    click_label(desktop, "Open release A").await?;
    wait_confirmation(desktop).await?;
    checked(desktop, CONFIRMATION_CHECK).await?;
    checked(desktop,r#"(()=>{document.getElementById('button-confirmation-cancel').click();return {ok:true};})()"#).await?;
    wait_dom(
        desktop,
        r#"({ok:!document.getElementById('button-confirmation-form')})"#,
    )
    .await?;
    assert_attempts(state, 0, 0)?;
    click_label(desktop, "Open release A").await?;
    wait_confirmation(desktop).await?;
    // A repeated native click is still one action: confirmation is consumed once.
    checked(desktop,r#"(()=>{const button=document.querySelector('#button-confirmation-form button[type=submit]');button.click();button.click();return {ok:true};})()"#).await?;
    wait_dom(
        desktop,
        r#"({ok:document.querySelectorAll('#app-modal-form .app-modal-field').length===9})"#,
    )
    .await?;
    assert_attempts(state, 1, 1)?;
    set_name(state, desktop, "Native release").await?;
    click_form_submit(desktop).await?;
    wait_dom(desktop,r#"({ok:document.querySelector('[data-block-id="name"] .app-modal-error')?.textContent==='Choose another release name.'})"#).await?;
    set_name(state, desktop, "Approved release").await?;
    click_form_submit(desktop).await?;
    wait_dom(
        desktop,
        r#"({ok:document.querySelector('#app-modal-title')?.textContent==='Review release'})"#,
    )
    .await?;
    click_form_submit(desktop).await?;
    wait_dom(
        desktop,
        r#"({ok:!document.getElementById('app-modal-form')})"#,
    )
    .await?;
    assert_attempts(state, 1, 1)?;

    click_label(desktop, "Review changed action").await?;
    wait_confirmation(desktop).await?;
    super::fixture::edit_confirmed_source(state);
    wait_dom(
        desktop,
        r#"({ok:!document.getElementById('button-confirmation-form')})"#,
    )
    .await?;
    assert_attempts(state, 1, 1)?;

    click_label(desktop, "Open release B").await?;
    wait_dom(desktop,r#"(()=>{const b=document.querySelector('.block-button[aria-label="Open release B"]');return {ok:b?.disabled&&b.getAttribute('aria-busy')==='true'};})()"#).await?;
    click_label(desktop, "Open release B").await?;
    wait_dom(desktop,r#"(()=>{const b=document.querySelector('.block-button[aria-label="Open release B"]');return {ok:b?.disabled&&b.title.includes('not confirmed')&&b.getAttribute('aria-busy')==='false'};})()"#).await?;
    click_label(desktop, "Open release B").await?;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_attempts(state, 2, 1)?;
    {
        let shell = state.read();
        let fixture = shell
            .message_buttons
            .fixture
            .as_ref()
            .ok_or("Missing fixture state")?;
        if !fixture.prepared_valid
            || !fixture.wrong_token_ignored
            || fixture.attempts
                != vec![
                    ("release-a".into(), "open".into(), "release-a".into()),
                    ("release-b".into(), "open".into(), "release-b".into()),
                ]
            || shell.core.transport.is_some()
        {
            return Err(
                "Synthetic requests did not preserve exact action identity and correlation".into(),
            );
        }
    }
    state.write().toast = None;
    click_label(desktop, "Open release A").await?;
    wait_confirmation(desktop).await?;
    let mut receipt = checked(desktop, CONFIRMATION_CHECK).await?;
    assert_attempts(state, 2, 1)?;
    receipt["cancelSentNothing"] = json!(true);
    receipt["confirmedActionCount"] = json!(1);
    receipt["repeatedClickSentOnce"] = json!(true);
    receipt["modalOpenedSubmittedPushedCleared"] = json!(true);
    receipt["editedSourceCancelled"] = json!(true);
    receipt["pendingAndUncertainLocked"] = json!(true);
    receipt["sameLabelDistinctBlocks"] = json!(true);
    receipt["hiddenRecipientAbsent"] = json!(true);
    receipt["ambiguousIdentityDisabled"] = json!(true);
    receipt["wrongTokenIgnored"] = json!(true);
    receipt["preparedRequests"] = json!(2);
    receipt["recordedUrlIntents"] = json!(1);
    receipt["noTransport"] = json!(true);
    Ok(receipt)
}

fn assert_attempts(state: Signal<ShellState>, attempts: usize, urls: usize) -> Result<(), String> {
    let shell = state.read();
    let fixture = shell
        .message_buttons
        .fixture
        .as_ref()
        .ok_or("Missing fixture state")?;
    if fixture.attempts.len() != attempts || fixture.url_intents != urls {
        return Err("Unexpected synthetic action or URL intent count".into());
    }
    Ok(())
}
async fn click_label(desktop: &dioxus::desktop::DesktopContext, label: &str) -> Result<(), String> {
    let label = serde_json::to_string(label).unwrap();
    checked(desktop,&format!("(()=>{{const buttons=[...document.querySelectorAll('.block-button')].filter(b=>b.getAttribute('aria-label')==={label});if(buttons.length!==1)return {{ok:false,targetCount:buttons.length}};buttons[0].click();return {{ok:true}};}})()")).await?;
    Ok(())
}
async fn wait_confirmation(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    wait_dom(desktop,r#"({ok:!!document.getElementById('button-confirmation-form')&&document.activeElement===document.getElementById('button-confirmation-cancel')})"#).await?;
    Ok(())
}
async fn set_name(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    value: &str,
) -> Result<(), String> {
    let text = serde_json::to_string(value).unwrap();
    checked(desktop,&format!("(()=>{{const input=document.querySelector('[data-block-id=\"name\"] input');input.value={text};input.dispatchEvent(new Event('input',{{bubbles:true}}));return {{ok:true}};}})()")).await?;
    let key = InputKey {
        block_id: "name".into(),
        action_id: "value".into(),
    };
    for _ in 0..60 {
        if state
            .read()
            .app_modal
            .as_ref()
            .and_then(|session| session.current_form())
            .and_then(|form| form.fields.get(&key))
            .is_some_and(|field| field.value == InputValue::Text(value.into()))
        {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Err("Native form edit did not reach typed state".into())
}
async fn click_form_submit(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    checked(desktop,r#"(()=>{document.querySelector('#app-modal-form button[type=submit]').click();return {ok:true};})()"#).await?;
    Ok(())
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
    Err("Native message button did not reach expected state".into())
}
async fn checked(desktop: &dioxus::desktop::DesktopContext, script: &str) -> Result<Value, String> {
    let receipt = eval(desktop, script).await?;
    if receipt["ok"] == true {
        Ok(receipt)
    } else {
        Err(format!("Native message button assertion failed: {receipt}"))
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

const CONFIRMATION_CHECK: &str = r#"(()=>{
    const dialog=document.getElementById('shell-overlay-dialog'),form=document.getElementById('button-confirmation-form');
    const body=document.getElementById('button-confirmation-body'),cancel=document.getElementById('button-confirmation-cancel');
    const controls=[...dialog.querySelectorAll('button,[tabindex]')].filter(e=>!e.disabled&&e.tabIndex>=0&&e.getClientRects().length);
    const first=controls[0],last=controls.at(-1);last.focus();last.dispatchEvent(new KeyboardEvent('keydown',{key:'Tab',bubbles:true,cancelable:true}));
    const forwardTrap=document.activeElement===first;
    first.dispatchEvent(new KeyboardEvent('keydown',{key:'Tab',shiftKey:true,bubbles:true,cancelable:true}));
    const reverseTrap=document.activeElement===last;cancel.focus({preventScroll:true});
    const d=dialog.getBoundingClientRect(),footer=form.querySelector('footer').getBoundingClientRect();
    const footerVisible=footer.bottom<=innerHeight+1&&footer.right<=innerWidth+1&&footer.top>=d.top;
    const noOverflow=dialog.scrollWidth<=dialog.clientWidth+1&&body.scrollWidth<=body.clientWidth+1;
    const formatted=!!body.querySelector('.bold')&&body.textContent.includes('release A');
    return {ok:forwardTrap&&reverseTrap&&footerVisible&&noOverflow&&formatted,forwardTrap,reverseTrap,footerVisible,noOverflow,formatted,
        viewport:[innerWidth,innerHeight],dialog:{width:d.width,height:d.height}};
})()"#;

async fn run_metadata(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let initial=wait_dom(desktop,r#"(()=>{const buttons=[...document.querySelectorAll('.block-button')];
        const loading=buttons.filter(b=>b.title==='Loading app information…').length;
        return {ok:buttons.length===11&&buttons.every(b=>b.disabled)&&loading===8,buttons:buttons.length,loading};})()"#).await?;
    {
        let shell = state.read();
        let fixture = shell
            .message_buttons
            .fixture
            .as_ref()
            .ok_or("Missing metadata fixture")?;
        if fixture.lookup_started.len() != 8
            || fixture.lookup_in_flight != 8
            || !fixture.lookup_completed.is_empty()
            || !fixture.attempts.is_empty()
            || fixture.url_intents != 0
            || shell.core.transport.is_some()
        {
            return Err("Initial metadata loading did not respect the eight-request bound".into());
        }
    }
    super::fixture::release_metadata(state);
    wait_dom(desktop,r#"(()=>{const buttons=[...document.querySelectorAll('.block-button')];
        const fetched=buttons.filter(b=>b.getAttribute('aria-label')?.startsWith('Lookup bot '));
        const incomplete=buttons.find(b=>b.getAttribute('aria-label')==='Incomplete inline profile');
        return {ok:fetched.length===10&&fetched.every(b=>!b.disabled)&&incomplete?.disabled&&
            !incomplete.title.includes('Loading')&&buttons.every(b=>b.title!=='Loading app information…')};})()"#).await?;
    {
        let shell = state.read();
        let fixture = shell
            .message_buttons
            .fixture
            .as_ref()
            .ok_or("Missing metadata fixture")?;
        let expected: Vec<String> = (1..=10).map(|index| format!("BLOOKUP{index:03}")).collect();
        let mut started = fixture.lookup_started.clone();
        started.sort();
        let mut completed = fixture.lookup_completed.clone();
        completed.sort();
        if started != expected
            || completed != expected
            || fixture.lookup_max_in_flight != 8
            || fixture.lookup_in_flight != 0
            || fixture.lookup_failed
            || !fixture.attempts.is_empty()
            || fixture.url_intents != 0
            || shell.core.transport.is_some()
            || shell.overlay.is_some()
        {
            return Err(
                "Metadata queue failed to drain exactly once without action dispatch".into(),
            );
        }
        let workspace = &shell.core.workspaces["T1"];
        for index in 1..=10 {
            let bot = format!("BLOOKUP{index:03}");
            let app = format!("ALOOKUP{index:03}");
            let profile = workspace
                .bots
                .get(&bot, Some(&app))
                .ok_or("Missing completed bot profile")?;
            if profile.extra.get("app_id").and_then(Value::as_str) != Some(app.as_str())
                || profile.extra.get("team_id").and_then(Value::as_str) != Some("TVALID001")
            {
                return Err("Completed metadata lost exact app routing".into());
            }
        }
        if workspace
            .bots
            .get("BLOOKUP999", Some("ALOOKUP999"))
            .is_some()
        {
            return Err("Incomplete inline profile incorrectly fetched fallback metadata".into());
        }
    }
    let mut receipt = checked(desktop, METADATA_LAYOUT).await?;
    receipt["initialDisabled"] = json!(true);
    receipt["initialLoading"] = initial["loading"].clone();
    receipt["lookupStarted"] = json!(10);
    receipt["lookupCompleted"] = json!(10);
    receipt["maxInFlight"] = json!(8);
    receipt["queueDrained"] = json!(true);
    receipt["incompleteInlineNotFetched"] = json!(true);
    receipt["preparedRequests"] = json!(0);
    receipt["recordedUrlIntents"] = json!(0);
    receipt["noTransport"] = json!(true);
    Ok(receipt)
}

const METADATA_LAYOUT: &str = r#"(()=>{
    const timeline=document.getElementById('message-timeline');
    const buttons=[...timeline.querySelectorAll('.block-button')];
    const bounds=timeline.getBoundingClientRect();
    const individuallyVisible=buttons.every(button=>{
        timeline.scrollTop+=button.getBoundingClientRect().top-bounds.top-16;
        const r=button.getBoundingClientRect();
        return r.top>=bounds.top-1&&r.bottom<=bounds.bottom+1&&r.left>=bounds.left-1&&r.right<=bounds.right+1;
    });
    timeline.scrollTop=timeline.scrollHeight;
    const incomplete=buttons.find(b=>b.getAttribute('aria-label')==='Incomplete inline profile');
    const r=incomplete.getBoundingClientRect();
    const incompleteVisible=r.top>=bounds.top-1&&r.bottom<=bounds.bottom+1;
    const noOverflow=timeline.scrollWidth<=timeline.clientWidth+1;
    return {ok:individuallyVisible&&incompleteVisible&&noOverflow,individuallyVisible,incompleteVisible,noOverflow,
        viewport:[innerWidth,innerHeight],timeline:{width:timeline.clientWidth,scrollWidth:timeline.scrollWidth,height:timeline.clientHeight}};
})()"#;
