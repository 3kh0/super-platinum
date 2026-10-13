//! Shared native picker gestures and held-write checks.
use super::*;
use crate::slack_lists::fixture;
pub(super) async fn choose(
    desktop: &dioxus::desktop::DesktopContext,
    id: &str,
) -> Result<(), String> {
    let before=checked(desktop,&format!(r#"(()=>{{const picker=document.getElementById('slack-list-view-select'),dialog=picker.closest('.slack-list-dialog'),old=dialog.dataset.layoutGeneration;picker.focus();picker.value={};picker.dispatchEvent(new Event('change',{{bubbles:true}}));return {{ok:true,old}};}})()"#,json!(id))).await?;
    wait_dom(desktop,&format!(r#"({{ok:document.getElementById('slack-list-view-select')?.value==={}&&document.querySelector('.slack-list-dialog')?.dataset.layoutGeneration!=={}}})"#,json!(id),before["old"])).await?;
    Ok(())
}
pub(super) async fn edit(
    domain: &str,
    desktop: &dioxus::desktop::DesktopContext,
    column: &str,
) -> Result<(), String> {
    checked_entity(desktop,domain,&format!(r#"(()=>{{const cell=document.querySelector('tr[data-row-id="R_LIST_000"] [data-column-id="{column}"]');cell.scrollIntoView({{block:'nearest',inline:'nearest'}});cell.focus();cell.dispatchEvent(new KeyboardEvent('keydown',{{key:'F2',bubbles:true,cancelable:true}}));return {{ok:true}};}})()"#)).await?;
    wait_entity(desktop,domain,r#"({ok:!!document.getElementById('slack-list-user-query')&&document.activeElement===document.getElementById('slack-list-user-query')&&!document.getElementById('slack-list-editor-save').disabled})"#).await?;
    Ok(())
}
pub(super) async fn query(
    domain: &str,
    desktop: &dioxus::desktop::DesktopContext,
    query: &str,
) -> Result<(), String> {
    checked_entity(desktop,domain,&format!(r#"(()=>{{const input=document.getElementById('slack-list-user-query');input.focus();input.value={};input.dispatchEvent(new Event('input',{{bubbles:true}}));return {{ok:true}};}})()"#,json!(query))).await?;
    Ok(())
}
pub(super) async fn choice(
    domain: &str,
    desktop: &dioxus::desktop::DesktopContext,
    id: &str,
) -> Result<(), String> {
    checked_entity(desktop,domain,&format!(r#"(()=>{{const input=document.querySelector('input[name="slack-list-user-choice"][value="{id}"]');input.focus();input.click();return {{ok:true}};}})()"#)).await?;
    wait_entity(
        desktop,
        domain,
        &format!(r#"({{ok:!!document.querySelector('[data-selected-user="{id}"]')}})"#),
    )
    .await?;
    Ok(())
}
pub(super) async fn remove(
    domain: &str,
    desktop: &dioxus::desktop::DesktopContext,
    id: &str,
) -> Result<(), String> {
    checked_entity(desktop,domain,&format!(r#"(()=>{{document.querySelector('[data-selected-user="{id}"] button').click();return {{ok:true}};}})()"#)).await?;
    wait_entity(
        desktop,
        domain,
        &format!(r#"({{ok:!document.querySelector('[data-selected-user="{id}"]')}})"#),
    )
    .await?;
    Ok(())
}
pub(super) async fn save(
    domain: &str,
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    attempts: usize,
) -> Result<(), String> {
    checked_entity(desktop,domain,r#"(()=>{const save=document.getElementById('slack-list-editor-save');save.focus();save.click();save.click();return {ok:true};})()"#).await?;
    wait_attempts(state, attempts).await?;
    wait_entity(desktop,domain,r#"({ok:document.getElementById('slack-list-editor-save')?.disabled&&document.getElementById('slack-list-user-query')?.disabled&&[...document.querySelectorAll('.slack-list-entity-picker fieldset')].every(e=>e.disabled)})"#).await?;
    assert_attempts(state, attempts)?;
    fixture::complete_pending(&mut state.write());
    crate::slack_lists::refresh(state);
    wait_entity(
        desktop,
        domain,
        r#"({ok:!document.getElementById('slack-list-cell-editor')})"#,
    )
    .await?;
    assert_attempts(state, attempts)
}

async fn checked_entity(
    desktop: &dioxus::desktop::DesktopContext,
    domain: &str,
    source: &str,
) -> Result<Value, String> {
    checked(desktop, &entity_script(source, domain)).await
}
async fn wait_entity(
    desktop: &dioxus::desktop::DesktopContext,
    domain: &str,
    source: &str,
) -> Result<Value, String> {
    wait_dom(desktop, &entity_script(source, domain)).await
}
fn entity_script(source: &str, domain: &str) -> String {
    source
        .replace("slack-list-user-", &format!("slack-list-{domain}-"))
        .replace("data-selected-user", &format!("data-selected-{domain}"))
}

/// Deliver an older input event, then let native typing advance before its
/// cached-result render arrives. This deterministically exercises the desktop
/// IPC gap without sleeps or changing the editor's mutation boundary.
pub(super) async fn typing_survives_results(
    domain: &str,
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<(), String> {
    let attempts = fixture::attempt_count(&state.peek());
    checked_entity(desktop,domain,r#"(()=>{const input=document.getElementById('slack-list-user-query');input.focus();input.value='zz-no-results';input.dispatchEvent(new Event('input',{bubbles:true}));input.value='zz-no-results-fast';input.setSelectionRange(input.value.length,input.value.length);window.__listEntityTyping=input;return {ok:true};})()"#).await?;
    // The empty candidate view proves that the older query's Rust render has
    // arrived. Merely testing the manually assigned DOM value would not do so.
    wait_entity(desktop,domain,r#"({ok:document.querySelectorAll('input[name="slack-list-user-choice"]').length===0&&document.querySelector('.slack-list-entity-picker')?.textContent.includes('No matching')})"#).await?;
    let query = crate::slack_lists::presentation(&state.peek())
        .and_then(|view| view.editor)
        .map(|editor| editor.directory.query);
    if query.as_deref() != Some("zz-no-results") {
        return Err("Older entity query did not reach its result render".into());
    }
    checked_entity(desktop,domain,r#"(()=>{const input=document.getElementById('slack-list-user-query');return {ok:input===window.__listEntityTyping&&input.value==='zz-no-results-fast'&&input.selectionStart===input.value.length&&input.selectionEnd===input.value.length,value:input.value,start:input.selectionStart,end:input.selectionEnd};})()"#).await?;
    checked_entity(desktop,domain,r#"(()=>{const input=document.getElementById('slack-list-user-query');input.dispatchEvent(new Event('input',{bubbles:true}));return {ok:true};})()"#).await?;
    // Publish the newer input and restore the empty-query candidate view for
    // the unchanged selection/cancel/write checks that follow.
    for _ in 0..60 {
        let query = crate::slack_lists::presentation(&state.peek())
            .and_then(|view| view.editor)
            .map(|editor| editor.directory.query);
        if query.as_deref() == Some("zz-no-results-fast") {
            query_reset(domain, desktop).await?;
            return assert_attempts(state, attempts);
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    Err("Newer entity input did not reach the scoped query".into())
}
async fn query_reset(
    domain: &str,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<(), String> {
    query(domain, desktop, "").await?;
    wait_entity(desktop,domain,r#"({ok:document.getElementById('slack-list-user-query')?.value===''&&document.querySelectorAll('input[name="slack-list-user-choice"]').length===3})"#).await?;
    Ok(())
}
