//! Inert saved target review preserves exact custody even when child readback is absent.
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
    checked(desktop,r#"(()=>{const b=[...document.querySelectorAll('.canvas-toolbar button')].find(b=>b.textContent==='Review saved change');if(!b||b.disabled)return {ok:false};b.id='owner-first-review-open';return {ok:true};})()"#).await?;
    click(desktop, "#owner-first-review-open").await?;
    wait(desktop,r#"({ok:!!document.querySelector('.canvas-local-review > .canvas-review-preview')&&!document.querySelector('.canvas-local-review input')?.disabled})"#).await?;
    checked(desktop,r#"(()=>{const e=document.querySelector('.canvas-local-review > .canvas-review-preview');if(!e)return {ok:false};e.scrollIntoView({block:'center'});return {ok:true};})()"#).await?;
    let geometry=wait(desktop,&format!(r#"(()=>{{const e=document.querySelector('.canvas-local-review > .canvas-review-preview'),r=e?.getBoundingClientRect(),c=e?.closest('.canvas-scroll')?.getBoundingClientRect();return {{ok:!!r&&!!c&&e.textContent==={}&&r.left>=Math.max(0,c.left)-1&&r.right<=Math.min(innerWidth,c.right)+1&&r.top>=Math.max(0,c.top)-1&&r.bottom<=Math.min(innerHeight,c.bottom)+1,text:e?.textContent,section:'draft',rect:r?[r.x,r.y,r.width,r.height]:null,clip:c?[c.x,c.y,c.width,c.height]:null,viewport:[innerWidth,innerHeight]}};}})()"#,json!(child_text()))).await?;
    capture(state, "review-draft", &mut ordinal, geometry).await?;
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
