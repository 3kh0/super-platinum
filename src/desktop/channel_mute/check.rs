//! Disconnected native checks for the real Channel Details notification action.
use crate::state::ShellState;
use dioxus::prelude::*;
use serde_json::{Value, json};

pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !super::fixture::enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let receipt = match run(state, &desktop).await {
            Ok(value) => value,
            Err(error) => json!({"ok":false,"error":error}),
        };
        eprintln!("channel-mute-check: {receipt}");
    });
}

async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    if state.peek().core.transport.is_some() || state.peek().core.session.is_some() {
        return Err("Connected fixture refused".into());
    }
    action(desktop, "Mute channel", false).await?;
    let initial_layout = layout(desktop).await?;
    let fixed_chrome = wait(desktop, r#"(()=>{
        const dialog=document.getElementById('channel-details-dialog');
        const body=dialog.querySelector('.channel-details-body');
        const header=dialog.querySelector('header'),tabs=dialog.querySelector('[role=tablist]');
        if(!body||!header||!tabs)return {ok:false};
        const h=header.getBoundingClientRect().top,t=tabs.getBoundingClientRect().top;
        const overflow=body.scrollHeight>body.clientHeight+2;
        body.scrollTop=body.scrollHeight;
        const scrolled=body.scrollTop>0;
        const stable=Math.abs(header.getBoundingClientRect().top-h)<1&&Math.abs(tabs.getBoundingClientRect().top-t)<1;
        body.scrollTop=0;
        return {ok:stable&&(!overflow||scrolled),stable,overflow,scrolled};
    })()"#).await?;
    act(
        desktop,
        "const b=document.getElementById('channel-mute-action');b.focus();b.click();b.click()",
    )
    .await?;
    attempts(state, 1).await?;
    action(desktop, "Mute channel", true).await?;
    if super::presentation(&state.peek(), "C2").is_none_or(|view| view.muted) {
        return Err("Pending mute optimistically changed canonical preferences".into());
    }
    // The dialog may close while the account-owned request remains pending.
    act(desktop,"document.getElementById('channel-mute-action').dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}))").await?;
    wait(desktop,"({ok:!document.getElementById('channel-details-dialog')&&document.activeElement?.id==='channel-details-trigger'})").await?;
    act(
        desktop,
        "document.getElementById('channel-details-trigger').click()",
    )
    .await?;
    action(desktop, "Mute channel", true).await?;
    if super::fixture::attempts(&state.peek()) != 1 {
        return Err("Reopen replayed mute".into());
    }
    super::fixture::complete(&mut state.write(), false);
    action(desktop, "Unmute channel", false).await?;
    if !state.peek().core.workspaces["T1"].sidebar.is_muted("C2") {
        return Err("Mute did not update sidebar notification state".into());
    }
    act(
        desktop,
        "document.getElementById('channel-mute-action').click()",
    )
    .await?;
    attempts(state, 2).await?;
    action(desktop, "Unmute channel", true).await?;
    super::fixture::complete(&mut state.write(), false);
    action(desktop, "Mute channel", false).await?;
    if state.peek().core.workspaces["T1"].sidebar.is_muted("C2") {
        return Err("Unmute left sidebar notification state muted".into());
    }

    act(
        desktop,
        "document.getElementById('channel-mute-action').click()",
    )
    .await?;
    attempts(state, 3).await?;
    super::fixture::complete(&mut state.write(), true);
    wait(desktop,"({ok:document.getElementById('channel-mute-action')?.disabled===true&&document.getElementById('channel-mute-check')?.disabled===false})").await?;
    act(desktop,"document.getElementById('channel-mute-check').click();document.getElementById('channel-mute-action').click()").await?;
    crate::slack_links_check::wait_state(state, |shell| super::fixture::reads(shell) == 1).await?;
    super::fixture::complete_read(&mut state.write());
    action(desktop, "Unmute channel", false).await?;
    if super::fixture::attempts(&state.peek()) != 3 {
        return Err("Status check replayed a preference write".into());
    }

    super::fixture::realtime(&mut state.write(), false);
    action(desktop, "Mute channel", false).await?;
    super::fixture::realtime(&mut state.write(), true);
    action(desktop, "Unmute channel", false).await?;
    // Choosing a duration is only a local draft. Cancel must not send or mute.
    super::fixture::realtime(&mut state.write(), false);
    action(desktop, "Mute channel", false).await?;
    choose_duration(desktop, "tomorrow").await?;
    let duration_layout = layout(desktop).await?;
    act(
        desktop,
        "document.getElementById('channel-mute-temporary-cancel').click()",
    )
    .await?;
    wait(desktop,"({ok:!document.getElementById('channel-mute-duration')&&document.getElementById('channel-mute-temporary')?.disabled===false&&document.activeElement?.id==='channel-mute-temporary'})").await?;
    if super::fixture::attempts(&state.peek()) != 3
        || super::presentation(&state.peek(), "C2").is_none_or(|view| view.muted)
    {
        return Err("Duration selection/Cancel dispatched or changed canonical mute".into());
    }
    // Stock's sole-expiry write replaces a permanent mute server-side.
    super::fixture::realtime(&mut state.write(), true);
    action(desktop, "Unmute channel", false).await?;
    choose_duration(desktop, "hour").await?;
    act(
        desktop,
        "const b=document.getElementById('channel-mute-temporary-save');b.click();b.click()",
    )
    .await?;
    attempts(state, 4).await?;
    wait(desktop,"({ok:document.getElementById('channel-mute-temporary')?.disabled===true&&document.getElementById('channel-mute-action')?.disabled===true})").await?;
    super::fixture::complete(&mut state.write(), true);
    wait(
        desktop,
        "({ok:document.getElementById('channel-mute-check')?.disabled===false})",
    )
    .await?;
    act(
        desktop,
        "document.getElementById('channel-mute-check').click()",
    )
    .await?;
    crate::slack_links_check::wait_state(state, |shell| super::fixture::reads(shell) == 2).await?;
    super::fixture::complete_read(&mut state.write());
    action(desktop, "Unmute channel", false).await?;
    if !super::fixture::temporary_matches(&state.peek(), 3, 3600)
        || super::fixture::attempts(&state.peek()) != 4
    {
        return Err(
            "One-hour mute did not retain exact expiry/sibling permanent state, or replayed".into(),
        );
    }
    choose_duration(desktop, "tomorrow").await?;
    act(
        desktop,
        "document.getElementById('channel-mute-temporary-save').click()",
    )
    .await?;
    attempts(state, 5).await?;
    super::fixture::complete(&mut state.write(), false);
    action(desktop, "Unmute channel", false).await?;
    if !super::fixture::temporary_matches(&state.peek(), 4, 21 * 3600) {
        return Err("Until-tomorrow mute did not use the captured 09:00 local default".into());
    }
    choose_duration(desktop, "hour").await?;
    let final_layout = layout(desktop).await?;
    let shell = state.peek();
    if !super::fixture::preserved(&shell) || !super::fixture::prepared_valid(&shell) {
        return Err("Mute request or sibling preference preservation failed".into());
    }
    Ok(
        json!({"ok":true,"fixedChrome":fixed_chrome,"mute":true,"unmute":true,"duplicateBlocked":true,
        "pendingAcrossReopen":true,"uncertainNoReplay":true,"readOnlyCheck":true,
        "preferenceProjection":true,"siblingPreferencesPreserved":true,"preparedRequestExact":true,
        "focusReturned":true,"noTransport":true,"temporaryHour":true,"temporaryTomorrow":true,
        "durationCancelNoWrite":true,"temporaryUncertainNoReplay":true,"temporaryExactExpiry":true,"permanentToTemporary":true,"durationLayout":duration_layout,"attempts":super::fixture::attempts(&shell),
        "reads":super::fixture::reads(&shell),"layout":initial_layout,"finalLayout":final_layout}),
    )
}
async fn choose_duration(
    desktop: &dioxus::desktop::DesktopContext,
    value: &str,
) -> Result<(), String> {
    wait(
        desktop,
        "({ok:document.getElementById('channel-mute-temporary')?.disabled===false})",
    )
    .await?;
    act(
        desktop,
        "document.getElementById('channel-mute-temporary').click()",
    )
    .await?;
    wait(
        desktop,
        "({ok:document.getElementById('channel-mute-duration')?.disabled===false})",
    )
    .await?;
    let value = serde_json::to_string(value).unwrap();
    act(desktop,&format!("const s=document.getElementById('channel-mute-duration');s.focus();s.value={value};s.dispatchEvent(new Event('change',{{bubbles:true}}))")).await?;
    let label = if value == "\"tomorrow\"" {
        "Mute until tomorrow"
    } else {
        "Mute for 1 hour"
    };
    let label = serde_json::to_string(label).unwrap();
    wait(desktop,&format!("(()=>{{const b=document.getElementById('channel-mute-temporary-save');return {{ok:b?.disabled===false&&b.textContent.trim()==={label}}}}})()")).await?;
    Ok(())
}
async fn attempts(state: Signal<ShellState>, count: usize) -> Result<(), String> {
    crate::slack_links_check::wait_state(state, |shell| super::fixture::attempts(shell) == count)
        .await
}
async fn wait(desktop: &dioxus::desktop::DesktopContext, script: &str) -> Result<Value, String> {
    match crate::slack_links_check::wait_dom(desktop, script).await {
        Ok(value) => Ok(value),
        Err(error) => {
            let detail=crate::slack_links_check::wait_dom(desktop,r#"(()=>{const b=document.getElementById('channel-mute-action');return {ok:true,label:b?.textContent,disabled:b?.disabled,status:document.getElementById('channel-mute-status')?.textContent,active:document.activeElement?.id,dialog:!!document.getElementById('channel-details-dialog')};})()"#).await;
            Err(format!("{error}: {script}; {detail:?}"))
        }
    }
}
async fn act(desktop: &dioxus::desktop::DesktopContext, script: &str) -> Result<(), String> {
    wait(
        desktop,
        &format!("(()=>{{{script};return {{ok:true}};}})()"),
    )
    .await
    .map(|_| ())
}
async fn action(
    desktop: &dioxus::desktop::DesktopContext,
    label: &str,
    disabled: bool,
) -> Result<(), String> {
    let label = serde_json::to_string(label).unwrap();
    wait(desktop,&format!("(()=>{{const b=document.getElementById('channel-mute-action');return {{ok:b?.textContent.trim()==={label}&&b.disabled==={disabled}}};}})()")).await.map(|_|())
}
async fn layout(desktop: &dioxus::desktop::DesktopContext) -> Result<Value, String> {
    wait(desktop,r#"(()=>{const d=document.getElementById('channel-details-dialog'),b=document.getElementById('channel-mute-action'),s=document.getElementById('channel-mute-status');if(!d||!b||!s)return {ok:false};const r=d.getBoundingClientRect(),a=b.getBoundingClientRect(),t=s.getBoundingClientRect(),within=v=>v.left>=r.left&&v.right<=r.right&&v.top>=r.top&&v.bottom<=r.bottom;const controls=[...d.querySelectorAll('.channel-mute-actions button,.channel-mute-actions select')].map(e=>e.getBoundingClientRect());return {ok:controls.every(v=>v.height===32&&within(v))&&r.left>=0&&r.top>=0&&r.right<=innerWidth&&r.bottom<=innerHeight&&a.height===32&&within(a)&&within(t)&&b.querySelector('svg')!==null,viewport:[innerWidth,innerHeight],dialog:[r.left,r.top,r.right,r.bottom],action:[a.left,a.top,a.right,a.bottom],status:[t.left,t.top,t.right,t.bottom]};})()"#).await
}
