use super::*;

async fn token_view(desktop: &DesktopContext, text: &str, deleted: bool) -> Result<Value, String> {
    let label = if deleted {
        "Deleted canvas"
    } else {
        "Private canvas"
    };
    let units = json!([
        label.encode_utf16().collect::<Vec<_>>(),
        label.encode_utf16().collect::<Vec<_>>()
    ]);
    let kind = if deleted { "deleted" } else { "private" };
    let icon = if deleted { "delete" } else { "lock" };
    wait_dom(desktop,&format!(r#"(()=>{{const e=document.getElementById('profile-section-rich');if(!e)return{{ok:false}};const c=window.superPlatinumRichEditor.resolved(e),tokens=[...e.querySelectorAll('.rich-editor-token')],expected={units},kind={kind},iconName={icon},disabled={deleted};const actual=tokens.map(t=>Array.from({{length:t.textContent.length}},(_,i)=>t.textContent.charCodeAt(i)));const icons=tokens.map(t=>{{const icon=t.querySelector(':scope > .rich-editor-'+kind+'-symbol'),r=icon?.getBoundingClientRect();return{{statusClass:t.classList.contains('rich-editor-'+kind+'-canvas'),sectionClass:t.classList.contains('rich-editor-section-canvas'),disabled:t.getAttribute('aria-disabled'),editable:t.contentEditable,icon:icon?{{hidden:icon.getAttribute('aria-hidden'),paths:[...icon.querySelectorAll('path')].map(p=>p.getAttribute('d')),width:r.width,height:r.height}}:null}};}});return{{ok:e.isContentEditable&&c.snapshot.blocks[0].elements[0].elements[0].text==={text}&&tokens.length===2&&JSON.stringify(actual)===JSON.stringify(expected)&&JSON.stringify(c.tokens.map(t=>t.labelUnits))===JSON.stringify(expected)&&c.tokens.every(t=>!Object.hasOwn(t,'label')&&t.presentation?.kind===kind+'-canvas'&&t.presentation.icon===iconName&&t.presentation.disabled===disabled)&&icons.every(i=>i.statusClass&&!i.sectionClass&&i.disabled===String(disabled)&&i.editable==='false'&&i.icon?.width>0&&i.icon?.height>0),capture:c,units:actual,icons,editorEditable:e.isContentEditable}};}})()"#,kind=json!(kind),icon=json!(icon),text=json!(text))).await
}
async fn edit(desktop: &DesktopContext, text: &str, deleted: bool) -> Result<(), String> {
    wait_dom(desktop,&format!(r#"(()=>{{const e=document.getElementById('profile-section-rich'),leaf=e?.querySelector('.rich-editor-section > span:not(.rich-editor-token)');if(!e?.isContentEditable||!leaf)return{{ok:false}};leaf.textContent={};e.dispatchEvent(new InputEvent('input',{{bubbles:true,inputType:'insertText'}}));return{{ok:true}};}})()"#,json!(text))).await?;
    token_view(desktop, text, deleted).await.map(|_| ())
}
async fn no_sections() -> Result<(), String> {
    settled(0, false).await?;
    let files = file::status();
    if files["requests"].as_array().map(Vec::len) != Some(1) || files["active"] != 0 {
        return Err("Section status acquired unexpected file metadata".into());
    }
    Ok(())
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    ready: Value,
) -> Result<Value, String> {
    let mut stages = Vec::new();
    let mut captures = serde_json::Map::new();
    file::mode(file::Mode::Normal);
    click(desktop, "Edit description", false).await?;
    let token = token_view(desktop, "Working with ", false).await?;
    no_sections().await?;
    shot(
        state,
        desktop,
        name,
        "private-opened",
        json!({"token":token,"sectionRequests":0}),
        &mut stages,
        &mut captures,
    )
    .await?;
    edit(desktop, "Retained draft with ", false).await?;
    let event = source::status::delete(&mut state.write())?;
    let token = token_view(desktop, "Retained draft with ", false).await?;
    no_sections().await?;
    shot(
        state,
        desktop,
        name,
        "event-mounted",
        json!({"token":token,"event":event,"mountedPrivateRetained":true,"sectionRequests":0}),
        &mut stages,
        &mut captures,
    )
    .await?;
    click(desktop, "Cancel description edit", false).await?;
    no_editor(desktop).await?;
    click(desktop, "Edit description", false).await?;
    let token = token_view(desktop, "Working with ", true).await?;
    no_sections().await?;
    shot(
        state,
        desktop,
        name,
        "deleted-reopened",
        json!({"token":token,"sectionRequests":0}),
        &mut stages,
        &mut captures,
    )
    .await?;
    edit(desktop, "Saved with ", true).await?;
    use_description(state, desktop, "Saved with ").await?;
    no_sections().await?;
    shot(
        state,
        desktop,
        name,
        "used",
        json!({"rawTokensPreserved":true,"sectionRequests":0}),
        &mut stages,
        &mut captures,
    )
    .await?;
    save(state, desktop, Outcome::Saved, 1, 1).await?;
    click(desktop, "Edit description", false).await?;
    let token = token_view(desktop, "Saved with ", true).await?;
    no_sections().await?;
    shot(
        state,
        desktop,
        name,
        "confirmed",
        json!({"token":token,"savedAndReadBack":true,"sectionRequests":0}),
        &mut stages,
        &mut captures,
    )
    .await?;
    Ok(
        json!({"ok":true,"fixture":name,"case":"status","canvasSection":true,"mentionKind":"canvas","titleBlocks":true,"readiness":ready,"stages":stages,"captures":captures,"viewport":stages[0]["geometry"]["viewport"],"captureChecksPassed":true,"files":file::status(),"sections":source::status(),"visualReview":"PNG inspection required separately"}),
    )
}
