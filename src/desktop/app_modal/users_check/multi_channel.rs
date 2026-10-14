//! Native multi-channel modal stages, using production field events and submission.
use super::*;
use crate::app_modal::multi_channel_fixture as channels_fixture;
pub(super) async fn run(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    wait(
        desktop,
        r#"({ok:document.querySelectorAll('.app-modal-users').length===2})"#,
    )
    .await?;
    if state.peek().core.transport.is_some() {
        return Err("Expected disconnected fixture".into());
    }
    let initial = state
        .peek()
        .app_modal
        .as_ref()
        .unwrap()
        .current_form()
        .unwrap()
        .submission()
        .map_err(|e| e.to_string())?;
    if initial["values"]["reviewers"]["value"]["selected_channels"] != json!(["C00000006"])
        || initial["values"]["optional"]["value"]["selected_channels"] != json!([])
    {
        return Err("Initial/empty modal state mismatch".into());
    }
    input(desktop, "note", "Local sibling draft").await?;
    open(desktop, "reviewers").await?;
    wait(desktop,r#"({ok:!!document.querySelector('[data-conversation-id="C00000006"]')&&!document.querySelector('[data-conversation-id="C00000005"]')&&!document.querySelector('[data-conversation-id="G00000003"]')})"#).await?;
    click(desktop, "[data-remove-user='C00000006']").await?;
    click(desktop, "#app-modal-form button[type=submit]").await?;
    wait(desktop,r#"({ok:document.querySelector('[data-block-id="reviewers"] .app-modal-error')?.textContent.includes('required')})"#).await?;
    attempts(state, 0)?;
    for id in ["C00000006", "C00000002", "C00000001"] {
        click(desktop, &format!("[data-conversation-id='{id}']")).await?;
    }
    click(desktop, "#app-modal-form button[type=submit]").await?;
    wait(desktop,r#"({ok:document.querySelectorAll('[data-remove-user]').length===3&&document.querySelector('[data-block-id="reviewers"] .app-modal-error')?.textContent.trim().length>0})"#).await?;
    attempts(state, 0)?;
    let invalid = capture(desktop, "maximum").await?;
    eval(desktop,r#"(()=>{document.querySelector('[data-block-id="reviewers"] .app-modal-error').scrollIntoView({block:'center'});return {ok:true};})()"#).await?;
    let validation_geometry = wait(desktop, VALIDATION_LAYOUT).await?;
    let validation = capture(desktop, "validation").await?;
    eval(desktop,r#"(()=>{document.querySelector('[data-block-id="reviewers"] input').scrollIntoView({block:'center'});return {ok:true};})()"#).await?;
    click(desktop, "[data-remove-user='C00000001']").await?;
    input(desktop, "reviewers", "design").await?;
    wait(desktop,r#"({ok:document.querySelectorAll('[data-conversation-id]').length===1&&document.querySelector('[data-conversation-id="C00000002"]')?.getAttribute('aria-checked')==='true'&&document.querySelectorAll('[data-remove-user]').length===2})"#).await?;
    eval(desktop,r#"(()=>{window.__oldModalUser=document.querySelector('[data-conversation-id="C00000002"]');return {ok:true};})()"#).await?;
    let mut update = channels_fixture::view();
    update["blocks"][1]["element"]["initial_channels"] = json!(["C00000001"]);
    update["blocks"][2]["element"]["initial_channels"] = json!(["C00000001"]);
    super::super::apply_fixture_update(state, update);
    wait(desktop,r#"({ok:!document.querySelector('[data-conversation-id]')&&document.querySelector('[data-block-id="optional"] [data-remove-user="C00000001"]')&&document.querySelector('[data-block-id="note"] input')?.value==='Local sibling draft'})"#).await?;
    eval(
        desktop,
        "(()=>{window.__oldModalUser.click();return {ok:true};})()",
    )
    .await?;
    click(
        desktop,
        "[data-block-id='optional'] [data-remove-user='C00000001']",
    )
    .await?;
    let cleared = capture(desktop, "cleared").await?;
    open(desktop, "reviewers").await?;
    wait(desktop,r#"({ok:document.querySelectorAll('[data-remove-user]').length===2&&document.querySelector('[data-conversation-id="C00000002"]')?.getAttribute('aria-checked')==='true'})"#).await?;
    let geometry = eval(desktop, LAYOUT).await?;
    if geometry["ok"] != true {
        return Err(format!("Inline selector geometry: {geometry}"));
    }
    let selected = capture(desktop, "selected").await?;
    // Collapse only the inline choices with Escape; do not close the app's view.
    wait(
        desktop,
        r#"({ok:!!document.querySelector('[data-conversation-id]')?.closest('[data-escape-local]')})"#,
    )
    .await?;
    eval(desktop,r#"(()=>{const e=document.querySelector('[data-conversation-id]');e.focus();e.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));return {ok:true};})()"#).await?;
    wait(desktop,r#"({ok:!!document.getElementById('app-modal-form')&&!document.querySelector('[data-conversation-id]')&&!document.querySelector('.app-modal-users[data-escape-local]')&&document.activeElement===document.querySelector('[data-block-id="reviewers"] input')})"#).await?;
    open(desktop, "reviewers").await?;
    eval(desktop,r#"(()=>{const e=document.querySelector('[data-block-id="reviewers"] .app-modal-users-actions button');e.focus();e.click();return {ok:true};})()"#).await?;
    wait(desktop,r#"({ok:!document.querySelector('[data-conversation-id]')&&document.activeElement===document.querySelector('[data-block-id="reviewers"] input')})"#).await?;
    eval(desktop,r#"(()=>{const b=document.querySelector('#app-modal-form button[type=submit]');b.click();b.click();return {ok:true};})()"#).await?;
    wait_attempts(state, 1).await?;
    wire(state, 0, &["C00000006", "C00000002"])?;
    wait(desktop,r#"({ok:document.querySelector('#app-modal-form button[type=submit]')?.disabled&&[...document.querySelectorAll('.app-modal-users input,.app-modal-users button')].every(e=>e.disabled)})"#).await?;
    fixture::complete(state, false);
    wait(desktop,r#"({ok:document.querySelector('[data-block-id="reviewers"] .app-modal-error')?.textContent==='Choose another reviewer.'&&!document.querySelector('#app-modal-form button[type=submit]').disabled})"#).await?;
    click(desktop, "[data-remove-user='C00000002']").await?;
    click(desktop, "#app-modal-form button[type=submit]").await?;
    wait_attempts(state, 2).await?;
    wire(state, 1, &["C00000006"])?;
    fixture::complete(state, true);
    wait(desktop,r#"({ok:document.querySelector('#app-modal-form button[type=submit]')?.disabled&&document.querySelector('.app-modal-notice')?.textContent.includes('not be repeated')})"#).await?;
    super::super::submit(state);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    attempts(state, 2)?;
    eval(
        desktop,
        "(()=>{document.querySelector('.app-modal-body').scrollTop=0;return {ok:true};})()",
    )
    .await?;
    let uncertain = capture(desktop, "uncertain").await?;
    let mut receipt = geometry;
    receipt["captures"] = json!({"maximum":invalid,"validation":validation,"selected":selected,"cleared":cleared,"uncertain":uncertain});
    receipt["validationErrorVisible"] = validation_geometry["ok"].clone();
    receipt["validationGeometry"] = validation_geometry;
    receipt["preparedRequests"] = json!(2);
    for flag in [
        "initialAndEmptyState",
        "publicChannelsOnly",
        "requiredAndMaximum",
        "inlineEditsNoDispatch",
        "searchRetainsSelection",
        "remoteRetainsDirtyAndRefreshesPristine",
        "staleChoiceRejected",
        "escapeRetainsModal",
        "fullSiblingState",
        "pendingDuplicateRejected",
        "serverFieldErrorRetained",
        "uncertainNoReplay",
        "noTransport",
    ] {
        receipt[flag] = json!(true);
    }
    Ok(receipt)
}
fn wire(state: Signal<ShellState>, index: usize, users: &[&str]) -> Result<(), String> {
    let shell = state.peek();
    let f = shell
        .app_modal
        .as_ref()
        .unwrap()
        .users_fixture
        .as_ref()
        .unwrap();
    let state = &f.states[index];
    if state
        == &json!({"values":{"note":{"value":{"type":"plain_text_input","value":"Local sibling draft"}},
        "reviewers":{"value":{"type":"multi_channels_select","selected_channels":users}},"optional":{"value":{"type":"multi_channels_select","selected_channels":[]}}}})
    {
        Ok(())
    } else {
        Err("Prepared full state differs from current modal fields".into())
    }
}
