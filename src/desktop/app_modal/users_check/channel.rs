//! Uses the real inline modal and shared discovery policy without a transport.
use super::super::channel_fixture as channels;
use super::*;

pub(super) async fn run(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    wait(desktop,r#"({ok:document.querySelectorAll('.app-modal-users').length===2&&!!document.querySelector('[data-block-id="reviewers"] [data-selected-user="C00000006"]')&&!document.querySelector('[data-block-id="reviewers"] [data-remove-user]')&&!document.querySelector('[data-block-id="optional"] [data-selected-user]')})"#).await?;
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
        != json!({"type":"channels_select","selected_channel":"C00000006","response_url_enabled":true})
        || initial["values"]["optional"]["value"]
            != json!({"type":"channels_select","selected_channel":null,"response_url_enabled":true})
    {
        return Err("Initial channel, empty optional state, or response flag differs".into());
    }
    attempts(state, 0)?;
    open(desktop, "reviewers").await?;
    wait(desktop,r#"({ok:document.querySelectorAll('[data-conversation-id]').length===3&&!!document.querySelector('[data-conversation-id="C00000001"]')&&!!document.querySelector('[data-conversation-id="C00000002"]')&&!!document.querySelector('[data-conversation-id="C00000006"]')&&!document.querySelector('[data-conversation-id="G00000003"]')&&!document.querySelector('[data-conversation-id="G00000004"]')&&!document.querySelector('[data-conversation-id="D00000008"]')&&!document.querySelector('[data-conversation-id="C00000005"]')&&!document.querySelector('[data-conversation-id^="U"]')})"#).await?;
    let origin = capture(desktop, "initial").await?;
    input(desktop, "note", "Local sibling draft").await?;
    eval(desktop,r#"(()=>{window.__oldModalConversation=document.querySelector('[data-conversation-id="C00000001"]');return {ok:true};})()"#).await?;
    input(desktop, "reviewers", "design").await?;
    wait(desktop,r#"({ok:document.querySelectorAll('[data-conversation-id]').length===1&&!!document.querySelector('[data-conversation-id="C00000002"]')&&!!document.querySelector('[data-selected-user="C00000006"]')})"#).await?;
    choose(desktop, "C00000002").await?;
    collapsed(desktop, "reviewers", "C00000002").await?;
    eval(
        desktop,
        "(()=>{window.__oldModalConversation.click();return {ok:true};})()",
    )
    .await?;
    collapsed(desktop, "reviewers", "C00000002").await?;
    attempts(state, 0)?;
    // New remote defaults refresh the untouched optional field, preserving the edited field.
    let mut update = channels::view();
    update["blocks"][2]["element"]["initial_channel"] = json!("C00000002");
    update["blocks"][1]["element"]["initial_channel"] = json!("C00000001");
    super::super::apply_fixture_update(state, update);
    wait(desktop,r#"({ok:!!document.querySelector('[data-block-id="reviewers"] [data-selected-user="C00000002"]')&&!!document.querySelector('[data-block-id="optional"] [data-remove-user="C00000002"]')&&document.querySelector('[data-block-id="note"] input')?.value==='Local sibling draft'})"#).await?;
    open(desktop, "optional").await?;
    wait(desktop,r#"({ok:document.querySelectorAll('[data-conversation-id]').length===3&&!!document.querySelector('[data-conversation-id="C00000001"]')&&!!document.querySelector('[data-conversation-id="C00000002"]')})"#).await?;
    eval(desktop,r#"(()=>{const e=document.querySelector('[data-conversation-id]');e.focus();e.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));return {ok:true};})()"#).await?;
    collapsed(desktop, "optional", "C00000002").await?;
    open(desktop, "reviewers").await?;
    eval(desktop,r#"(()=>{const e=document.querySelector('[data-block-id="reviewers"] .app-modal-users-actions button');e.focus();e.click();return {ok:true};})()"#).await?;
    collapsed(desktop, "reviewers", "C00000002").await?;
    let geometry = eval(desktop, LAYOUT).await?;
    if geometry["ok"] != true {
        return Err(format!("Channel modal geometry: {geometry}"));
    }
    let selected = capture(desktop, "selected").await?;
    click(
        desktop,
        "[data-block-id='optional'] [data-remove-user='C00000002']",
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
        return Err(format!("Channel clear geometry: {cleared_geometry}"));
    }
    let cleared = capture(desktop, "cleared").await?;
    attempts(state, 0)?;
    eval(desktop,r#"(()=>{const e=document.querySelector('#app-modal-form button[type=submit]');e.click();e.click();return {ok:true};})()"#).await?;
    wait_attempts(state, 1).await?;
    let expected = json!({"values":{
        "note":{"value":{"type":"plain_text_input","value":"Local sibling draft"}},
        "reviewers":{"value":{"type":"channels_select","selected_channel":"C00000002","response_url_enabled":true}},
        "optional":{"value":{"type":"channels_select","selected_channel":null,"response_url_enabled":true}}
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
        return Err("Channel full sibling state differs".into());
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
    receipt["captures"] =
        json!({"initial":origin,"selected":selected,"cleared":cleared,"uncertain":uncertain});
    receipt["preparedRequests"] = json!(1);
    receipt["clearGeometry"] = cleared_geometry;
    for flag in [
        "initialChannelWithoutOriginFallback",
        "responseFlagPreserved",
        "publicChannelsOnly",
        "excludesPrivateDmMpimArchived",
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
async fn choose(d: &dioxus::desktop::DesktopContext, id: &str) -> Result<(), String> {
    let q = serde_json::to_string(&format!("[data-conversation-id='{id}']")).unwrap();
    let result=eval(d,&format!("(()=>{{const e=document.querySelector({q});if(!e||e.disabled)return {{ok:false}};e.focus();e.click();return {{ok:true}};}})()")).await?;
    if result["ok"] == true {
        Ok(())
    } else {
        Err(format!("Missing conversation choice {id}"))
    }
}
async fn collapsed(
    d: &dioxus::desktop::DesktopContext,
    block: &str,
    selected: &str,
) -> Result<(), String> {
    let q = serde_json::to_string(&format!("[data-block-id='{block}']")).unwrap();
    let selected = serde_json::to_string(selected).unwrap();
    wait(d,&format!("(()=>{{const f=document.querySelector({q}),i=f?.querySelector('input'),s=f?.querySelector('[data-selected-user]');return {{ok:!!document.getElementById('app-modal-form')&&!document.querySelector('[data-conversation-id]')&&document.activeElement===i&&(s?.dataset.selectedUser||'')==={selected}}};}})()")).await?;
    Ok(())
}
