//! Native editor checks share the disconnected backend and existing window.
use super::*;
use crate::slack_lists::fixture;

pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let mut expected = fixture::attempt_count(&state.peek());
    wait_dom(desktop,r#"({ok:document.querySelector('[data-row-id="R_LIST_005"] [data-column-id="C_NUMBER"] .slack-list-cell-text')?.textContent==='13'})"#).await?;
    open(desktop, "R_LIST_005", "C_NUMBER").await?;
    checked(
        desktop,
        r#"({ok:document.getElementById('slack-list-number-draft')?.value==='12.5'})"#,
    )
    .await?;
    cancel(desktop).await?;
    open(desktop, "R_LIST_020", "C_NAME").await?;
    checked(desktop,r#"(()=>{
      const owner=document.createElement('div');owner.dataset.editorGeneration='new';owner.hidden=true;document.body.append(owner);
      const root=document.createElement('div');root.id='list-editor-mount-race-check';owner.append(root);
      const blocks=text=>[{type:'rich_text',elements:[{type:'rich_text_section',elements:[{type:'text',text}]}]}];
      const api=window.superPlatinumRichEditor;api.mount(root.id,blocks('old'),{},false,'old');
      const rejected=root.childNodes.length===0;api.mount(root.id,blocks('new'),{},false,'new');
      const current=JSON.stringify(api.read(root.id))===JSON.stringify(blocks('new'));owner.remove();
      return {ok:rejected&&current,rejected,current};
    })()"#).await?;
    rich_text(desktop, "Canceled name").await?;
    checked(desktop,r#"(()=>{document.getElementById('slack-list-rich-draft').dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));return {ok:true};})()"#).await?;
    wait_dom(desktop,r#"({ok:!document.getElementById('slack-list-cell-editor')&&!!document.querySelector('.slack-list-dialog')&&document.activeElement===document.querySelector('[data-row-id="R_LIST_020"] [data-column-id="C_NAME"]')})"#).await?;
    assert_attempts(state, expected)?;
    open(desktop, "R_LIST_020", "C_NAME").await?;
    checked(desktop,r#"({ok:!document.getElementById('slack-list-rich-draft').textContent.includes('Canceled name')})"#).await?;
    rich_text(desktop, "Named existing item").await?;
    save(state, desktop, &mut expected).await?;
    wait_dom(desktop,r#"({ok:document.querySelector('[data-row-id="R_LIST_020"] [data-column-id="C_NAME"]').textContent.includes('Named existing item')})"#).await?;
    assert_rich_readback(state, "R_LIST_020", "NAME")?;

    open(desktop, "R_LIST_000", "C_RICH").await?;
    let rich_editor_capture = capture_stage(desktop, "editor-rich").await?;
    checked(desktop,r#"(()=>{
      const root=document.getElementById('slack-list-rich-draft');window.__listRichOriginal=window.superPlatinumRichEditor.read(root.id);
      const bold=root.querySelector('strong');if(!bold)return {ok:false,reason:'missing bold draft'};
      bold.firstChild.textContent+='!';root.dispatchEvent(new Event('input',{bubbles:true}));
      const actual=window.superPlatinumRichEditor.read(root.id),expected=JSON.parse(JSON.stringify(window.__listRichOriginal));
      expected[0].elements[0].elements[0].text+='!';
      return {ok:JSON.stringify(actual)===JSON.stringify(expected),preservedLink:actual[0].elements[0].elements.at(-1).url==='https://example.com/spec'};
    })()"#).await?;
    save(state, desktop, &mut expected).await?;
    wait_dom(desktop,r#"(()=>{const cell=document.querySelector('[data-row-id="R_LIST_000"] [data-column-id="C_RICH"]');return {ok:cell.querySelector('.bold')?.textContent==='Ready to ship!'&&cell.querySelector('.italic')?.textContent.trim()==='review'&&cell.querySelector('a')?.getAttribute('href')==='https://example.com/spec'};})()"#).await?;
    assert_rich_readback(state, "R_LIST_000", "notes")?;

    open(desktop, "R_LIST_000", "C_NUMBER").await?;
    number(desktop, "not a number").await?;
    click_save(desktop).await?;
    wait_dom(desktop,r#"({ok:document.querySelector('#slack-list-cell-editor [role=alert]')?.textContent.includes('valid number')})"#).await?;
    assert_attempts(state, expected)?;
    number(desktop, "0.125").await?;
    save(state, desktop, &mut expected).await?;
    wait_dom(desktop,r#"({ok:document.querySelector('[data-row-id="R_LIST_000"] [data-column-id="C_NUMBER"] .slack-list-cell-text')?.textContent==='0'})"#).await?;
    open(desktop, "R_LIST_000", "C_NUMBER").await?;
    checked(
        desktop,
        r#"({ok:document.getElementById('slack-list-number-draft')?.value==='0.125'})"#,
    )
    .await?;
    cancel(desktop).await?;

    open(desktop, "R_LIST_000", "C_CURRENCY").await?;
    number(desktop, "0").await?;
    save(state, desktop, &mut expected).await?;
    open(desktop, "R_LIST_000", "C_CURRENCY").await?;
    checked(
        desktop,
        r#"({ok:document.getElementById('slack-list-number-draft').value==='0'})"#,
    )
    .await?;
    number(desktop, "").await?;
    save(state, desktop, &mut expected).await?;
    open(desktop, "R_LIST_000", "C_CURRENCY").await?;
    checked(
        desktop,
        r#"({ok:document.getElementById('slack-list-number-draft').value===''})"#,
    )
    .await?;
    cancel(desktop).await?;

    open(desktop, "R_LIST_000", "C_SELECT").await?;
    checked(desktop,r#"(()=>{const choice=document.querySelector('#slack-list-cell-editor input[value="OPT_DONE"]');choice.click();return {ok:choice.checked};})()"#).await?;
    save(state, desktop, &mut expected).await?;
    wait_dom(desktop,r#"({ok:document.querySelector('[data-row-id="R_LIST_000"] [data-column-id="C_SELECT"] .slack-list-option')?.textContent==='Done'})"#).await?;
    open(desktop, "R_LIST_000", "C_MULTI").await?;
    checked(desktop,r#"(()=>{document.querySelector('#slack-list-cell-editor input[value="OPT_DESIGN"]').click();document.querySelector('#slack-list-cell-editor input[value="OPT_ENGINEERING"]').click();return {ok:true};})()"#).await?;
    wait_dom(
        desktop,
        r#"({ok:document.querySelectorAll('#slack-list-cell-editor input:checked').length===2})"#,
    )
    .await?;
    let select_editor_capture = capture_stage(desktop, "editor-select").await?;
    save(state, desktop, &mut expected).await?;
    wait_dom(desktop,r#"(()=>{const labels=[...document.querySelectorAll('[data-row-id="R_LIST_000"] [data-column-id="C_MULTI"] .slack-list-option')].map(e=>e.textContent);return {ok:JSON.stringify(labels)===JSON.stringify(['Design','Engineering'])};})()"#).await?;

    open(desktop, "R_LIST_000", "C_NUMBER").await?;
    number(desktop, "777").await?;
    fixture::refresh(&mut state.write());
    wait_dom(desktop,r#"({ok:document.getElementById('slack-list-editor-save')?.disabled&&document.getElementById('slack-list-number-draft')?.value==='777'&&!document.getElementById('slack-list-editor-cancel')?.disabled})"#).await?;
    cancel(desktop).await?;
    assert_attempts(state, expected)?;

    open(desktop, "R_LIST_000", "C_NUMBER").await?;
    number(desktop, "5").await?;
    fixture::set_outcome_uncertain(&mut state.write());
    click_save(desktop).await?;
    expected += 1;
    wait_attempts(state, expected).await?;
    fixture::complete_pending(&mut state.write());
    wait_dom(desktop,r#"({ok:document.getElementById('slack-list-editor-save')?.disabled&&!!document.getElementById('slack-list-editor-refresh')&&!document.getElementById('slack-list-editor-refresh').disabled})"#).await?;
    checked(desktop,r#"(()=>{document.getElementById('slack-list-editor-refresh').click();return {ok:true};})()"#).await?;
    wait_dom(desktop,r#"({ok:!document.getElementById('slack-list-cell-editor')&&document.querySelector('[data-row-id="R_LIST_000"] [data-column-id="C_NUMBER"] .slack-list-cell-text')?.textContent==='5'})"#).await?;
    assert_attempts(state, expected)?;
    if !fixture::prepared_valid(&state.peek()) {
        return Err("Typed cell request shape or retry policy was invalid".into());
    }
    fixture::set_number_precision(&mut state.write(), Some(2));
    wait_dom(desktop,r#"({ok:document.querySelector('[data-row-id="R_LIST_005"] [data-column-id="C_NUMBER"] .slack-list-cell-text')?.textContent==='12.50'})"#).await?;
    open(desktop, "R_LIST_005", "C_NUMBER").await?;
    number(desktop, "123.456").await?;
    fixture::set_locale(&mut state.write(), Some("de-DE"));
    wait_dom(desktop,r#"({ok:document.getElementById('slack-list-number-draft')?.value==='123.456'&&!document.getElementById('slack-list-editor-save')?.disabled&&document.querySelector('[data-row-id="R_LIST_005"] [data-column-id="C_NUMBER"] .slack-list-cell-text')?.textContent==='12,50'})"#).await?;
    cancel(desktop).await?;
    fixture::set_locale(&mut state.write(), None);
    fixture::set_number_precision(&mut state.write(), None);
    assert_attempts(state, expected)?;
    Ok(
        json!({"cancelWithoutWrite":true,"emptyPrimaryName":true,"richBlocksPreserved":true,"serverRootBlockIdReadback":true,"invalidNumberNoWrite":true,"decimalZeroAndClear":true,"numericDisplayPrecision":true,"localeChangePreservesDraft":true,"singleAndMultiSelect":true,"staleDraftPreservedAndBlocked":true,"uncertainRefreshWithoutReplay":true,"attempts":expected,"captures":{"rich":rich_editor_capture,"select":select_editor_capture}}),
    )
}

fn assert_rich_readback(state: Signal<ShellState>, row: &str, key: &str) -> Result<(), String> {
    let shell = state.peek();
    let view = shell.slack_lists.view.as_ref().ok_or("List view missing")?;
    let blocks = view
        .loaded
        .as_ref()
        .and_then(|loaded| loaded.records.record(row))
        .and_then(|record| record.fields.get(key))
        .and_then(|field| field.get("rich_text"))
        .and_then(Value::as_array)
        .ok_or("Refreshed rich field missing")?;
    if blocks.is_empty()
        || !blocks.iter().all(|block| {
            block["block_id"]
                .as_str()
                .is_some_and(|id| id.starts_with("fixture-server-"))
        })
        || view.error.is_some()
        || view.editor.is_some()
        || shell.slack_lists.operations.contains_key(&view.id)
    {
        return Err("Server-assigned root block ID did not resolve rich readback cleanly".into());
    }
    Ok(())
}

async fn open(
    desktop: &dioxus::desktop::DesktopContext,
    row: &str,
    column: &str,
) -> Result<(), String> {
    let row = json!(row);
    let column = json!(column);
    checked(desktop,&format!(r#"(()=>{{const cell=document.querySelector(`[data-row-id=${{CSS.escape({row})}}] [data-column-id=${{CSS.escape({column})}}]`);if(!cell?.querySelector('.slack-list-edit-cell:not(:disabled)'))return {{ok:false,row:{row},column:{column}}};cell.focus();cell.dispatchEvent(new KeyboardEvent('keydown',{{key:'F2',bubbles:true,cancelable:true}}));return {{ok:true}};}})()"#)).await?;
    wait_dom(desktop,r#"(()=>{const editor=document.getElementById('slack-list-cell-editor'),rich=editor?.querySelector('[contenteditable=true]'),focused=document.activeElement;return {ok:!!editor&&editor.contains(focused)&&(!rich||focused===rich)&&!document.getElementById('slack-list-editor-save').disabled,focused:focused?.id||focused?.getAttribute('aria-label'),rich:!!rich};})()"#).await?;
    checked(desktop,r#"(()=>{const e=document.getElementById('slack-list-cell-editor'),r=e.getBoundingClientRect(),save=document.getElementById('slack-list-editor-save').getBoundingClientRect(),cancel=document.getElementById('slack-list-editor-cancel').getBoundingClientRect();return {ok:r.left>=0&&r.right<=innerWidth&&r.top>=0&&r.bottom<=innerHeight&&save.bottom<=r.bottom&&save.top>=r.top&&Math.abs(save.top-cancel.top)<=1&&Math.abs(save.height-cancel.height)<=1};})()"#).await?;
    Ok(())
}
async fn number(desktop: &dioxus::desktop::DesktopContext, value: &str) -> Result<(), String> {
    let value = json!(value);
    checked(desktop,&format!("(()=>{{const input=document.getElementById('slack-list-number-draft');input.value={value};input.dispatchEvent(new Event('input',{{bubbles:true}}));return {{ok:true}};}})()")).await?;
    wait_dom(
        desktop,
        &format!("({{ok:document.getElementById('slack-list-number-draft').value==={value}}})"),
    )
    .await?;
    Ok(())
}
async fn rich_text(desktop: &dioxus::desktop::DesktopContext, value: &str) -> Result<(), String> {
    let value = json!(value);
    checked(desktop,&format!("(()=>{{const input=document.getElementById('slack-list-rich-draft');input.textContent={value};input.dispatchEvent(new Event('input',{{bubbles:true}}));return {{ok:true}};}})()")).await?;
    Ok(())
}
async fn click_save(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    checked(desktop,r#"(()=>{const b=document.getElementById('slack-list-editor-save');if(b.disabled)return {ok:false,reason:'save disabled'};b.click();return {ok:true};})()"#).await?;
    Ok(())
}
async fn save(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    expected: &mut usize,
) -> Result<(), String> {
    click_save(desktop).await?;
    *expected += 1;
    wait_attempts(state, *expected).await?;
    wait_dom(
        desktop,
        r#"({ok:document.getElementById('slack-list-editor-save')?.disabled})"#,
    )
    .await?;
    fixture::complete_pending(&mut state.write());
    crate::slack_lists::refresh(state);
    wait_dom(
        desktop,
        r#"({ok:!document.getElementById('slack-list-cell-editor')})"#,
    )
    .await?;
    assert_attempts(state, *expected)
}
async fn cancel(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    checked(desktop,r#"(()=>{document.getElementById('slack-list-editor-cancel').click();return {ok:true};})()"#).await?;
    wait_dom(
        desktop,
        r#"({ok:!document.getElementById('slack-list-cell-editor')})"#,
    )
    .await?;
    Ok(())
}
