//! Ordinary people edits through the same native editor and guarded cell write.
use super::entity_edit::{choice, choose, edit, query, remove, save, typing_survives_results};
use super::*;
use crate::slack_lists::fixture;

pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let attempts = fixture::attempt_count(&state.peek());
    let snapshot = fixture::add_user_edits(&mut state.write())
        .ok_or("People checks require a disconnected fixture")?;
    wait_dom(desktop,r#"({ok:!!document.querySelector('#slack-list-view-select option[value="V_EDIT_USERS"]')})"#).await?;
    choose(desktop, "V_EDIT_USERS").await?;
    wait_dom(desktop,r#"({ok:!!document.querySelector('tr[data-row-id="R_LIST_000"] [data-column-id="C_EDIT_USER"] .slack-list-edit-cell')&&!document.querySelector('tr[data-row-id="R_LIST_000"] [data-column-id="C_EDIT_TASK_USER"] .slack-list-edit-cell')})"#).await?;
    edit("user", desktop, "C_EDIT_USER").await?;
    typing_survives_results("user", state, desktop).await?;
    wait_dom(desktop,r#"(()=>{const selected=[...document.querySelectorAll('[data-selected-user]')].map(e=>e.dataset.selectedUser),choices=[...document.querySelectorAll('input[name="slack-list-user-choice"]')].map(e=>e.value);return {ok:JSON.stringify(selected)==='["U00000001"]'&&JSON.stringify(choices)==='["U00000001","U00000002","W00000003"]'&&document.querySelector('input[name="slack-list-user-choice"]')?.type==='radio',selected,choices};})()"#).await?;
    query("user", desktop, "Bob").await?;
    wait_dom(desktop,r#"({ok:document.querySelectorAll('input[name="slack-list-user-choice"]').length===1&&document.querySelector('input[name="slack-list-user-choice"]')?.value==='U00000002'})"#).await?;
    choice("user", desktop, "U00000002").await?;
    checked(desktop,r#"(()=>{document.getElementById('slack-list-editor-cancel').click();return {ok:true};})()"#).await?;
    wait_dom(desktop,r#"({ok:!document.getElementById('slack-list-cell-editor')&&document.activeElement===document.querySelector('tr[data-row-id="R_LIST_000"] [data-column-id="C_EDIT_USER"]')})"#).await?;
    assert_attempts(state, attempts)?;
    canonical(state, "edit_user", json!(["U00000001"]))?;

    edit("user", desktop, "C_EDIT_USER").await?;
    choice("user", desktop, "U00000002").await?;
    save("user", state, desktop, attempts + 1).await?;
    canonical(state, "edit_user", json!(["U00000002"]))?;
    edit("user", desktop, "C_EDIT_USER").await?;
    remove("user", desktop, "U00000002").await?;
    save("user", state, desktop, attempts + 2).await?;
    canonical(state, "edit_user", json!([]))?;

    edit("user", desktop, "C_EDIT_USERS").await?;
    wait_dom(desktop,r#"({ok:document.querySelector('[data-selected-user="U99999999"]')?.textContent.includes('U99999999')&&document.querySelector('input[name="slack-list-user-choice"]')?.type==='checkbox'})"#).await?;
    remove("user", desktop, "U99999999").await?;
    choice("user", desktop, "W00000003").await?;
    query("user", desktop, "Whitney").await?;
    wait_dom(desktop,r#"(()=>{const input=document.getElementById('slack-list-user-query'),save=document.getElementById('slack-list-editor-save');const visible=e=>{const r=e.getBoundingClientRect();return r.width>0&&r.height>0&&r.left>=0&&r.right<=innerWidth&&r.top>=0&&r.bottom<=innerHeight;};return {ok:visible(input)&&visible(save)&&document.querySelectorAll('[data-selected-user]').length===2};})()"#).await?;
    let editor = capture_stage(desktop, "user-editor").await?;
    save("user", state, desktop, attempts + 3).await?;
    canonical(state, "edit_users", json!(["U00000001", "W00000003"]))?;
    if !fixture::prepared_valid(&state.peek()) {
        return Err("People writes lost canonical request identity".into());
    }
    choose(desktop, "").await?;
    fixture::restore_user_edits(&mut state.write(), snapshot);
    wait_dom(desktop,r#"({ok:!document.querySelector('#slack-list-view-select option[value="V_EDIT_USERS"]')&&document.querySelector('.slack-list-grid tbody tr[data-row-id]')?.cells.length===11})"#).await?;
    assert_attempts(state, attempts + 3)?;
    Ok(
        json!({"candidateFilters":true,"guestAndExternal":true,"typingAndCancelNoWrite":true,"inputSurvivesResultRender":true,"singleReplace":true,"clear":true,"uncachedRemoval":true,"multiSelection":true,"taskAssigneeReadOnly":true,"keyboardEntryAndFocus":true,"duplicateLocked":true,"canonicalWrites":true,"cleanup":true,"captures":{"editor":editor}}),
    )
}
fn canonical(state: Signal<ShellState>, key: &str, value: Value) -> Result<(), String> {
    if fixture::user_edit_value(&state.peek(), key) != Some(value) {
        return Err(format!("People readback mismatch for {key}"));
    }
    Ok(())
}
