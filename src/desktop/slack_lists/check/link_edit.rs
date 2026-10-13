//! Native Link draft, one-shot saves, display mode, and inert unsafe text.
use super::*;
use crate::slack_lists::fixture;

pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let attempts = fixture::attempt_count(&state.peek());
    let snapshot = fixture::add_link_edits(&mut state.write()).ok_or("Link fixture unavailable")?;
    wait_dom(desktop,r#"({ok:!!document.querySelector('#slack-list-view-select option[value="V_EDIT_LINKS"]')})"#).await?;
    entity_edit::choose(desktop, "V_EDIT_LINKS").await?;
    open(desktop).await?;
    fill(desktop, "discard.example.com", "Discard this").await?;
    wait_dom(
        desktop,
        r#"({ok:document.getElementById('slack-list-link-display')?.disabled})"#,
    )
    .await?;
    checked(desktop,r#"(()=>{const input=document.getElementById('slack-list-link-name');input.focus();input.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));return {ok:true};})()"#).await?;
    wait_dom(desktop,r#"({ok:!document.getElementById('slack-list-cell-editor')&&document.activeElement===document.querySelector('tr[data-row-id="R_LIST_000"] [data-column-id="C_EDIT_LINK"]')})"#).await?;
    assert_attempts(state, attempts)?;
    open(desktop).await?;
    checked(desktop,r#"({ok:document.getElementById('slack-list-link-url').value==='https://example.com/original'&&document.getElementById('slack-list-link-name').value==='Original reference'})"#).await?;
    // Hold the actual CPU budget so cancellation occurs during an in-flight Save.
    let permit = crate::slack_lists::link_normalization_slot().await;
    checked(desktop,r#"({ok:!document.getElementById('slack-list-link-display').disabled&&!document.getElementById('slack-list-link-remove').disabled})"#).await?;
    checked(desktop,r#"(()=>{const root=document.getElementById('slack-list-cell-editor');window.__oldLinkEditor=root.dataset.editorGeneration;const save=document.getElementById('slack-list-editor-save');save.click();save.click();document.getElementById('slack-list-link-display')?.click();document.getElementById('slack-list-link-remove')?.click();return {ok:true};})()"#).await?;
    wait_dom(desktop,r#"({ok:document.getElementById('slack-list-editor-save')?.disabled&&document.getElementById('slack-list-link-url')?.disabled})"#).await?;
    assert_attempts(state, attempts)?;
    checked(desktop,r#"(()=>{document.getElementById('slack-list-editor-cancel').click();return {ok:true};})()"#).await?;
    wait_dom(
        desktop,
        r#"({ok:!document.getElementById('slack-list-cell-editor')})"#,
    )
    .await?;
    open(desktop).await?;
    checked(desktop,r#"({ok:document.getElementById('slack-list-cell-editor').dataset.editorGeneration!==window.__oldLinkEditor&&document.getElementById('slack-list-link-url').value==='https://example.com/original'})"#).await?;
    drop(permit);
    // Acquiring after release drains any older queued normalizer before asserting.
    let barrier = crate::slack_lists::link_normalization_slot().await;
    assert_attempts(state, attempts)?;
    drop(barrier);
    // A genuinely changed normalization result would be writable if it leaked
    // into the reopened draft; unlike the race check above, this is not a no-op.
    let permit = crate::slack_lists::link_normalization_slot().await;
    fill(
        desktop,
        "cancelled.example.com/changed",
        "Cancelled changed name",
    )
    .await?;
    checked(desktop,r#"(()=>{const root=document.getElementById('slack-list-cell-editor');window.__oldChangedLinkEditor=root.dataset.editorGeneration;document.getElementById('slack-list-editor-save').click();return {ok:true};})()"#).await?;
    wait_dom(desktop,r#"({ok:document.getElementById('slack-list-editor-save')?.disabled&&document.getElementById('slack-list-link-url')?.disabled})"#).await?;
    assert_attempts(state, attempts)?;
    checked(desktop,r#"(()=>{document.getElementById('slack-list-editor-cancel').click();return {ok:true};})()"#).await?;
    wait_dom(
        desktop,
        r#"({ok:!document.getElementById('slack-list-cell-editor')})"#,
    )
    .await?;
    open(desktop).await?;
    checked(desktop,r#"({ok:document.getElementById('slack-list-cell-editor').dataset.editorGeneration!==window.__oldChangedLinkEditor&&document.getElementById('slack-list-link-url').value==='https://example.com/original'&&document.getElementById('slack-list-link-name').value==='Original reference'})"#).await?;
    drop(permit);
    let barrier = crate::slack_lists::link_normalization_slot().await;
    assert_attempts(state, attempts)?;
    drop(barrier);
    fill(desktop, "example.com/new", "New reference").await?;
    wait_dom(desktop,r#"(()=>{const root=document.getElementById('slack-list-cell-editor'),fields=[...root.querySelectorAll('input')],buttons=['slack-list-editor-save','slack-list-editor-cancel','slack-list-link-remove'].map(id=>document.getElementById(id));const visible=e=>{const r=e.getBoundingClientRect();return r.height>0&&r.width>0&&r.left>=0&&r.right<=innerWidth&&r.top>=0&&r.bottom<=innerHeight;};return {ok:fields.every(visible)&&buttons.every(visible)&&document.getElementById('slack-list-link-display').disabled};})()"#).await?;
    let capture = capture_stage(desktop, "link-editor").await?;
    save(state, desktop, "slack-list-editor-save", attempts + 1).await?;
    canonical(
        state,
        json!([{"originalUrl":"http://example.com/new","displayName":"New reference","displayAsUrl":false,"attachment":{}}]),
    )?;
    open(desktop).await?;
    save(state, desktop, "slack-list-link-display", attempts + 2).await?;
    canonical(
        state,
        json!([{"originalUrl":"http://example.com/new","displayName":"New reference","displayAsUrl":true,"attachment":{}}]),
    )?;
    wait_dom(desktop,r#"({ok:document.querySelector('tr[data-row-id="R_LIST_000"] [data-column-id="C_EDIT_LINK"] a')?.textContent==='http://example.com/new'})"#).await?;
    open(desktop).await?;
    save(state, desktop, "slack-list-link-display", attempts + 3).await?;
    wait_dom(desktop,r#"({ok:document.querySelector('tr[data-row-id="R_LIST_000"] [data-column-id="C_EDIT_LINK"] a')?.textContent==='New reference'})"#).await?;
    open(desktop).await?;
    fill(desktop, "http://example.com/new", "Renamed reference").await?;
    save(state, desktop, "slack-list-editor-save", attempts + 4).await?;
    canonical(
        state,
        json!([{"originalUrl":"http://example.com/new","displayName":"Renamed reference","displayAsUrl":false,"attachment":{}}]),
    )?;
    open(desktop).await?;
    save(state, desktop, "slack-list-link-remove", attempts + 5).await?;
    canonical(state, json!([]))?;
    open(desktop).await?;
    checked(desktop,r#"({ok:document.getElementById('slack-list-link-url').value===''&&document.getElementById('slack-list-link-name').value===''&&!document.getElementById('slack-list-link-remove')})"#).await?;
    fill(desktop, "javascript:alert(1)", "Inert stored value").await?;
    save(state, desktop, "slack-list-editor-save", attempts + 6).await?;
    canonical(
        state,
        json!([{"originalUrl":"javascript:alert(1)","displayName":"Inert stored value","displayAsUrl":false,"attachment":{}}]),
    )?;
    wait_dom(desktop,r#"(()=>{const cell=document.querySelector('tr[data-row-id="R_LIST_000"] [data-column-id="C_EDIT_LINK"]');return {ok:cell.textContent.includes('Inert stored value')&&!cell.querySelector('a')};})()"#).await?;
    if !fixture::prepared_valid(&state.peek()) {
        return Err("Link write lost canonical identity".into());
    }
    entity_edit::choose(desktop, "").await?;
    fixture::restore_link_edits(&mut state.write(), snapshot);
    wait_dom(desktop,r#"({ok:!document.querySelector('#slack-list-view-select option[value="V_EDIT_LINKS"]')&&document.querySelector('.slack-list-grid tbody tr')?.cells.length===11})"#).await?;
    assert_attempts(state, attempts + 6)?;
    Ok(
        json!({"cancelNoWrite":true,"cancelPendingNormalization":true,"uncontrolledDrafts":true,"normalizedSave":true,"rename":true,"displayModes":true,"removeLink":true,"unsafeStorageInert":true,"duplicateLocked":true,"keyboardFocus":true,"boundedLayout":true,"cleanup":true,"captures":{"editor":capture}}),
    )
}
async fn open(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    wait_dom(desktop,r#"({ok:!!document.querySelector('tr[data-row-id="R_LIST_000"] [data-column-id="C_EDIT_LINK"] .slack-list-edit-cell:not(:disabled)')})"#).await?;
    checked(desktop,r#"(()=>{const cell=document.querySelector('tr[data-row-id="R_LIST_000"] [data-column-id="C_EDIT_LINK"]');cell.scrollIntoView({block:'nearest',inline:'nearest'});cell.focus();cell.dispatchEvent(new KeyboardEvent('keydown',{key:'F2',bubbles:true,cancelable:true}));return {ok:true};})()"#).await?;
    wait_dom(desktop,r#"({ok:document.activeElement===document.getElementById('slack-list-link-url')&&!!document.getElementById('slack-list-link-name')&&!document.getElementById('slack-list-editor-save')?.disabled})"#).await?;
    Ok(())
}
async fn fill(
    desktop: &dioxus::desktop::DesktopContext,
    url: &str,
    name: &str,
) -> Result<(), String> {
    checked(desktop,&format!(r#"(()=>{{const url=document.getElementById('slack-list-link-url'),name=document.getElementById('slack-list-link-name');url.value={};url.dispatchEvent(new Event('input',{{bubbles:true}}));name.value={};name.dispatchEvent(new Event('input',{{bubbles:true}}));return {{ok:true}};}})()"#,json!(url),json!(name))).await?;
    // Dirty-state rerender must retain both uncontrolled native input values.
    wait_dom(desktop,&format!(r#"({{ok:document.getElementById('slack-list-link-url')?.value==={}&&document.getElementById('slack-list-link-name')?.value==={}&&(!document.getElementById('slack-list-link-display')||document.getElementById('slack-list-link-display').disabled)}})"#,json!(url),json!(name))).await?;
    Ok(())
}
async fn save(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    button: &str,
    attempts: usize,
) -> Result<(), String> {
    checked(desktop,&format!(r#"(()=>{{const button=document.getElementById({});button.focus();button.click();button.click();return {{ok:true}};}})()"#,json!(button))).await?;
    wait_attempts(state, attempts).await?;
    wait_dom(desktop,r#"({ok:document.getElementById('slack-list-editor-save')?.disabled&&document.getElementById('slack-list-link-url')?.disabled&&document.getElementById('slack-list-link-name')?.disabled&&['slack-list-link-remove','slack-list-link-display'].every(id=>!document.getElementById(id)||document.getElementById(id).disabled)})"#).await?;
    assert_attempts(state, attempts)?;
    fixture::complete_pending(&mut state.write());
    crate::slack_lists::refresh(state);
    wait_dom(
        desktop,
        r#"({ok:!document.getElementById('slack-list-cell-editor')})"#,
    )
    .await?;
    assert_attempts(state, attempts)
}
fn canonical(state: Signal<ShellState>, value: Value) -> Result<(), String> {
    if fixture::link_edit_value(&state.peek()) != Some(value) {
        return Err("Link canonical readback differs from expected value".into());
    }
    Ok(())
}
