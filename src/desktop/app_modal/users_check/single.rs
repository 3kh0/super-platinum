//! Single-user mode uses the same modal/search ownership and prepared-request recorder.
use super::*;

pub(super) async fn run(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    wait(desktop,r#"({ok:document.querySelectorAll('.app-modal-users').length===2&&!!document.querySelector('[data-block-id="reviewers"] [data-selected-user="U00000001"]')&&!document.querySelector('[data-block-id="reviewers"] [data-remove-user]')&&!document.querySelector('[data-block-id="optional"] [data-selected-user]')})"#).await?;
    let initial = state
        .peek()
        .app_modal
        .as_ref()
        .unwrap()
        .current_form()
        .unwrap()
        .submission()
        .map_err(|e| e.to_string())?;
    if initial["values"]["reviewers"]["value"]
        != json!({"type":"users_select","selected_user":"U00000001"})
        || initial["values"]["optional"]["value"]
            != json!({"type":"users_select","selected_user":null})
    {
        return Err("Single-user initial/default state differs".into());
    }
    attempts(state, 0)?;
    input(desktop, "note", "Local sibling draft").await?;
    open(desktop, "reviewers").await?;
    wait(desktop,r#"({ok:!!document.querySelector('[data-user-id="U00000001"]')&&!document.querySelector('[data-user-id="U00000006"]')&&!document.querySelector('[data-user-id="U00000007"]')&&!document.querySelector('[data-user-id][role="checkbox"]')})"#).await?;
    eval(desktop,r#"(()=>{window.__oldSingleUser=document.querySelector('[data-user-id="U00000001"]');return {ok:true};})()"#).await?;
    input(desktop, "reviewers", "Blair").await?;
    wait(desktop,r#"({ok:document.querySelectorAll('[data-user-id]').length===1&&!!document.querySelector('[data-user-id="U00000002"]')&&!!document.querySelector('[data-selected-user="U00000001"]')})"#).await?;
    // Keyboard selection must return focus to the field that survives collapse.
    eval(desktop,r#"(()=>{const e=document.querySelector('[data-user-id="U00000002"]');e.focus();e.click();return {ok:true};})()"#).await?;
    collapsed(desktop, "reviewers", "U00000002").await?;
    eval(
        desktop,
        "(()=>{window.__oldSingleUser.click();return {ok:true};})()",
    )
    .await?;
    collapsed(desktop, "reviewers", "U00000002").await?;
    attempts(state, 0)?;

    // Remote defaults cannot overwrite the edited reviewer; the untouched observer refreshes.
    let mut update = fixture::single_view();
    update["blocks"][1]["element"]["initial_user"] = json!("U00000004");
    update["blocks"][2]["element"]["initial_user"] = json!("U00000004");
    super::super::apply_fixture_update(state, update);
    wait(desktop,r#"({ok:!!document.querySelector('[data-block-id="reviewers"] [data-selected-user="U00000002"]')&&!!document.querySelector('[data-block-id="optional"] [data-remove-user="U00000004"]')&&document.querySelector('[data-block-id="note"] input')?.value==='Local sibling draft'})"#).await?;
    eval(
        desktop,
        "(()=>{window.__oldSingleUser.click();return {ok:true};})()",
    )
    .await?;
    open(desktop, "reviewers").await?;
    input(desktop, "reviewers", "Casey").await?;
    wait(desktop,r#"({ok:document.querySelectorAll('[data-user-id]').length===1&&!!document.querySelector('[data-user-id="U00000003"]')})"#).await?;
    eval(desktop,r#"(()=>{const e=document.querySelector('[data-user-id]');e.focus();e.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));return {ok:true};})()"#).await?;
    collapsed(desktop, "reviewers", "U00000002").await?;
    open(desktop, "reviewers").await?;
    eval(desktop,r#"(()=>{const e=document.querySelector('[data-block-id="reviewers"] .app-modal-users-actions button');e.focus();e.click();return {ok:true};})()"#).await?;
    collapsed(desktop, "reviewers", "U00000002").await?;
    let geometry = eval(desktop, LAYOUT).await?;
    if geometry["ok"] != true {
        return Err(format!("Single-user geometry: {geometry}"));
    }
    let selected = capture(desktop, "selected").await?;

    click(
        desktop,
        "[data-block-id='optional'] [data-remove-user='U00000004']",
    )
    .await?;
    collapsed(desktop, "optional", "").await?;
    // Self is a real candidate, never an implicit default. Optional clear restores null.
    open(desktop, "optional").await?;
    eval(desktop,r#"(()=>{const e=document.querySelector('[data-user-id="U00000001"]');e.focus();e.click();return {ok:true};})()"#).await?;
    collapsed(desktop, "optional", "U00000001").await?;
    click(
        desktop,
        "[data-block-id='optional'] [data-remove-user='U00000001']",
    )
    .await?;
    collapsed(desktop, "optional", "").await?;
    let cleared_geometry = eval(
        desktop,
        &LAYOUT.replace(
            "[data-block-id=\"reviewers\"] input",
            "[data-block-id=\"optional\"] input",
        ),
    )
    .await?;
    if cleared_geometry["ok"] != true {
        return Err(format!("Optional clear geometry: {cleared_geometry}"));
    }
    let cleared = capture(desktop, "cleared").await?;
    attempts(state, 0)?;
    eval(desktop,r#"(()=>{const e=document.querySelector('#app-modal-form button[type=submit]');e.click();e.click();return {ok:true};})()"#).await?;
    wait_attempts(state, 1).await?;
    let expected = json!({"values":{
        "note":{"value":{"type":"plain_text_input","value":"Local sibling draft"}},
        "reviewers":{"value":{"type":"users_select","selected_user":"U00000002"}},
        "optional":{"value":{"type":"users_select","selected_user":null}}
    }});
    if state
        .peek()
        .app_modal
        .as_ref()
        .unwrap()
        .users_fixture
        .as_ref()
        .unwrap()
        .states[0]
        != expected
    {
        return Err("Single-user prepared full sibling state differs".into());
    }
    wait(desktop,r#"({ok:document.querySelector('#app-modal-form button[type=submit]')?.disabled&&[...document.querySelectorAll('.app-modal-users input,.app-modal-users button')].every(e=>e.disabled)})"#).await?;
    fixture::complete(state, true);
    wait(desktop,r#"({ok:document.querySelector('#app-modal-form button[type=submit]')?.disabled&&document.querySelector('.app-modal-notice')?.textContent.includes('not be repeated')})"#).await?;
    super::super::submit(state);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    attempts(state, 1)?;
    eval(
        desktop,
        "(()=>{document.querySelector('.app-modal-body').scrollTop=0;return {ok:true};})()",
    )
    .await?;
    let uncertain = capture(desktop, "uncertain").await?;
    let mut receipt = geometry;
    receipt["captures"] = json!({"selected":selected,"cleared":cleared,"uncertain":uncertain});
    receipt["preparedRequests"] = json!(1);
    receipt["clearGeometry"] = cleared_geometry;
    for flag in [
        "initialAndEmptyState",
        "selfAndEligibility",
        "requiredCannotClear",
        "optionalClearNull",
        "singleChoiceCollapsesAndFocuses",
        "inlineEditsNoDispatch",
        "searchRetainsSelection",
        "remoteRetainsDirtyAndRefreshesPristine",
        "staleChoiceRejected",
        "escapeAndDoneRetainModal",
        "fullSiblingState",
        "pendingDuplicateRejected",
        "uncertainNoReplay",
        "noTransport",
    ] {
        receipt[flag] = json!(true);
    }
    Ok(receipt)
}

async fn collapsed(
    desktop: &dioxus::desktop::DesktopContext,
    block: &str,
    selected: &str,
) -> Result<(), String> {
    let selector = serde_json::to_string(&format!("[data-block-id='{block}']")).unwrap();
    let selected = serde_json::to_string(selected).unwrap();
    wait(desktop,&format!("(()=>{{const f=document.querySelector({selector}),i=f?.querySelector('input'),s=f?.querySelector('[data-selected-user]');return {{ok:!!document.getElementById('app-modal-form')&&!document.querySelector('[data-user-id]')&&document.activeElement===i&&(s?.dataset.selectedUser||'')==={selected}}};}})()")).await?;
    Ok(())
}
