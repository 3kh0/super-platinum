//! Ordinary channel references edits through the same native editor and guarded cell write.
use super::entity_edit::{choice, choose, edit, query, remove, save, typing_survives_results};
use super::*;
use crate::slack_lists::fixture;

pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let attempts = fixture::attempt_count(&state.peek());
    let snapshot = fixture::add_channel_edits(&mut state.write())
        .ok_or("Channel checks require a disconnected fixture")?;
    wait_dom(desktop,r#"({ok:!!document.querySelector('#slack-list-view-select option[value="V_EDIT_CHANNELS"]')})"#).await?;
    choose(desktop, "V_EDIT_CHANNELS").await?;
    wait_dom(desktop,r#"({ok:!!document.querySelector('tr[data-row-id="R_LIST_000"] [data-column-id="C_EDIT_CHANNEL"] .slack-list-edit-cell')})"#).await?;
    let pre_hover = editor_hover::hover(state, desktop, "channel").await?;
    edit("channel", desktop, "C_EDIT_CHANNEL").await?;
    typing_survives_results("channel", state, desktop).await?;
    let blocked_editor = editor_hover::blocked(state, desktop, "channel").await?;
    wait_dom(desktop,r#"(()=>{const selected=[...document.querySelectorAll('[data-selected-channel]')].map(e=>e.dataset.selectedChannel),choices=[...document.querySelectorAll('input[name="slack-list-channel-choice"]')].map(e=>e.value);return {ok:JSON.stringify(selected)==='["C00000001"]'&&JSON.stringify(choices)==='["C00000001","G00000002","C00000003"]'&&document.querySelector('input[name="slack-list-channel-choice"]')?.type==='radio',selected,choices};})()"#).await?;
    query("channel", desktop, "beta").await?;
    wait_dom(desktop,r#"({ok:document.querySelectorAll('input[name="slack-list-channel-choice"]').length===1&&document.querySelector('input[name="slack-list-channel-choice"]')?.value==='G00000002'})"#).await?;
    choice("channel", desktop, "G00000002").await?;
    checked(desktop,r#"(()=>{document.getElementById('slack-list-editor-cancel').click();return {ok:true};})()"#).await?;
    wait_dom(desktop,r#"({ok:!document.getElementById('slack-list-cell-editor')&&document.activeElement===document.querySelector('tr[data-row-id="R_LIST_000"] [data-column-id="C_EDIT_CHANNEL"]')})"#).await?;
    assert_attempts(state, attempts)?;
    canonical(state, "edit_channel", json!(["C00000001"]))?;
    let after_close = editor_hover::hover(state, desktop, "channel").await?;
    editor_hover::dismiss(desktop, "channel").await?;

    edit("channel", desktop, "C_EDIT_CHANNEL").await?;
    choice("channel", desktop, "G00000002").await?;
    save("channel", state, desktop, attempts + 1).await?;
    canonical(state, "edit_channel", json!(["G00000002"]))?;
    edit("channel", desktop, "C_EDIT_CHANNEL").await?;
    remove("channel", desktop, "G00000002").await?;
    save("channel", state, desktop, attempts + 2).await?;
    canonical(state, "edit_channel", json!([]))?;

    edit("channel", desktop, "C_EDIT_CHANNELS").await?;
    wait_dom(desktop,r#"({ok:document.querySelector('[data-selected-channel="Z99999999"]')?.textContent.includes('Z99999999')&&document.querySelector('input[name="slack-list-channel-choice"]')?.type==='checkbox'})"#).await?;
    remove("channel", desktop, "Z99999999").await?;
    choice("channel", desktop, "C00000003").await?;
    query("channel", desktop, "gamma").await?;
    wait_dom(desktop,r#"(()=>{const input=document.getElementById('slack-list-channel-query'),save=document.getElementById('slack-list-editor-save');const visible=e=>{const r=e.getBoundingClientRect();return r.width>0&&r.height>0&&r.left>=0&&r.right<=innerWidth&&r.top>=0&&r.bottom<=innerHeight;};return {ok:visible(input)&&visible(save)&&document.querySelectorAll('[data-selected-channel]').length===2};})()"#).await?;
    let capture_before = editor_hover::capture_before(desktop, "channel").await?;
    let editor = capture_stage(desktop, "channel-editor").await?;
    let hover_capture = editor_hover::capture_after(desktop, "channel", &capture_before).await?;
    save("channel", state, desktop, attempts + 3).await?;
    canonical(state, "edit_channels", json!(["C00000001", "C00000003"]))?;
    if !fixture::prepared_valid(&state.peek()) {
        return Err("Channel writes lost canonical request identity".into());
    }
    choose(desktop, "").await?;
    fixture::restore_channel_edits(&mut state.write(), snapshot);
    wait_dom(desktop,r#"({ok:!document.querySelector('#slack-list-view-select option[value="V_EDIT_CHANNELS"]')&&document.querySelector('.slack-list-grid tbody tr[data-row-id]')?.cells.length===11})"#).await?;
    assert_attempts(state, attempts + 3)?;
    Ok(
        json!({"candidateFilters":true,"privateAndExternal":true,"typingAndCancelNoWrite":true,"inputSurvivesResultRender":true,"singleReplace":true,"clear":true,"uncachedRemoval":true,"multiSelection":true,"keyboardEntryAndFocus":true,"duplicateLocked":true,"canonicalWrites":true,"cleanup":true,"hover":{"preHover":pre_hover,"blockedEditor":blocked_editor,"afterClose":after_close,"capture":hover_capture},"captures":{"editor":editor}}),
    )
}
fn canonical(state: Signal<ShellState>, key: &str, value: Value) -> Result<(), String> {
    if fixture::channel_edit_value(&state.peek(), key) != Some(value) {
        return Err(format!("Channel readback mismatch for {key}"));
    }
    Ok(())
}
