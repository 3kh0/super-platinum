//! Inert pair review identifies the already-deleted child; no absent-row targeting.
use super::*;
pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    wait(desktop,r#"(()=>{const a=[...document.querySelectorAll('.canvas-paragraph-actions button')];return {ok:a.every(b=>b.disabled)&&document.body.textContent.includes('unconfirmed')};})()"#).await?;
    let source = snapshot(&state.peek());
    let receipt = recovery::stored()
        .await?
        .ok_or("Restart missing deletion receipt")?;
    let mut ordinal = 0;
    capture(
        state,
        "restored-lock",
        &mut ordinal,
        check::layout(desktop).await?,
    )
    .await?;
    checked(desktop,r#"(()=>{const b=[...document.querySelectorAll('.canvas-toolbar button')].find(b=>b.textContent==='Review saved change');if(!b||b.disabled)return {ok:false};b.id='retained-pair-review-open';return {ok:true};})()"#).await?;
    click(desktop, "#retained-pair-review-open").await?;
    wait(desktop,r#"({ok:!!document.querySelector('[data-review-owner-delete=child]')&&!document.querySelector('.canvas-local-review input')?.disabled})"#).await?;
    for (name, text, heading, deleted) in [
        (
            "original",
            owner_text(),
            "Paragraph requested for deletion",
            None,
        ),
        (
            "child",
            texts(&corpus()).1.to_owned(),
            "Previously deleted paragraph to preserve",
            Some("true"),
        ),
        (
            "current-child",
            texts(&corpus()).1.to_owned(),
            "Deleted paragraph in the current read",
            Some("true"),
        ),
    ] {
        checked(desktop,&format!(r#"(()=>{{const e=document.querySelector('[data-review-owner-delete="{name}"]');if(!e)return {{ok:false}};e.scrollIntoView({{block:'center'}});return {{ok:true}};}})()"#)).await?;
        let geometry=wait(desktop,&format!(r#"(()=>{{const e=document.querySelector('[data-review-owner-delete="{name}"]'),r=e?.getBoundingClientRect(),c=e?.closest('.canvas-scroll')?.getBoundingClientRect(),text=e?.querySelector('.canvas-review-preview')?.textContent,heading=e?.querySelector('h4')?.textContent,deleted=e?.getAttribute('data-review-deleted');return {{ok:!!r&&!!c&&text==={}&&heading==={}&&deleted==={}&&r.left>=Math.max(0,c.left)-1&&r.right<=Math.min(innerWidth,c.right)+1&&r.top>=Math.max(0,c.top)-1&&r.bottom<=Math.min(innerHeight,c.bottom)+1,section:{},text,heading,deleted,rect:r?[r.x,r.y,r.width,r.height]:null,clip:c?[c.x,c.y,c.width,c.height]:null,viewport:[innerWidth,innerHeight]}};}})()"#,json!(text),json!(heading),json!(deleted),json!(name))).await?;
        capture(state, &format!("review-{name}"), &mut ordinal, geometry).await?;
    }
    checked(desktop,r#"(()=>{const e=document.querySelector('.canvas-local-review .canvas-text-actions');if(!e)return {ok:false};e.scrollIntoView({block:'end'});return {ok:true};})()"#).await?;
    let geometry=wait(desktop,r#"(()=>{const p=document.querySelector('.canvas-local-review'),e=p?.querySelector('.canvas-text-actions'),r=e?.getBoundingClientRect(),c=e?.closest('.canvas-scroll')?.getBoundingClientRect(),input=p?.querySelector('input'),buttons=[...p?.querySelectorAll('button')||[]],cancel=buttons.find(b=>b.textContent==='Cancel'),remove=buttons.find(b=>b.textContent==='Remove local edit lock');return {ok:!!r&&!!c&&r.left>=Math.max(0,c.left)-1&&r.right<=Math.min(innerWidth,c.right)+1&&r.top>=Math.max(0,c.top)-1&&r.bottom<=Math.min(innerHeight,c.bottom)+1&&!!input&&!input.checked&&!input.disabled&&!!cancel&&!cancel.disabled&&!!remove&&remove.disabled,acknowledged:input?.checked,cancelEnabled:!!cancel&&!cancel.disabled,removeDisabled:!!remove&&remove.disabled,rect:r?[r.x,r.y,r.width,r.height]:null,clip:c?[c.x,c.y,c.width,c.height]:null,viewport:[innerWidth,innerHeight]};})()"#).await?;
    capture(state, "review-controls", &mut ordinal, geometry).await?;
    if !same_source(&state.peek(), &source)
        || recovery::stored().await?.as_ref() != Some(&receipt)
        || !fixture(&mut state.write()).writes.is_empty()
    {
        return Err("Inert review changed source/custody or replayed".into());
    }
    let mut result = terminal(state, desktop, ordinal).await?;
    result["noReplay"] = json!(true);
    Ok(result)
}
