//! Unknown insertion restart stays locked through staged inert comparisons.
use super::*;
async fn section(
    desktop: &dioxus::desktop::DesktopContext,
    selector: &str,
    name: &str,
    text: &str,
) -> Result<Value, String> {
    wait(desktop,&format!(r#"(()=>{{const e=document.querySelector({});if(!e)return {{ok:false}};e.scrollIntoView({{block:'center'}});return {{ok:true}};}})()"#,json!(selector))).await?;
    wait(desktop,&format!(r#"(()=>{{const e=document.querySelector({}),r=e?.getBoundingClientRect(),c=e?.closest('.canvas-scroll')?.getBoundingClientRect(),text=e?.querySelector('.canvas-review-preview')?.textContent;return {{ok:!!r&&!!c&&text==={}&&r.left>=Math.max(0,c.left)-1&&r.right<=Math.min(innerWidth,c.right)+1&&r.top>=Math.max(0,c.top)-1&&r.bottom<=Math.min(innerHeight,c.bottom)+1,section:{},text,rect:r?[r.x,r.y,r.width,r.height]:null,clip:c?[c.x,c.y,c.width,c.height]:null,viewport:[innerWidth,innerHeight]}};}})()"#,json!(selector),json!(text),json!(name))).await
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    wait(desktop,r#"(()=>{const b=[...document.querySelectorAll('.canvas-paragraph-actions button')];return {ok:b.length>0&&b.every(b=>b.disabled)&&document.body.textContent.includes('unconfirmed')};})()"#).await?;
    let source = snapshot(&state.peek());
    let receipt = recovery::stored()
        .await?
        .ok_or("Restart missing insertion receipt")?;
    let mut ordinal = 0;
    capture(
        state,
        "restored-lock",
        &mut ordinal,
        check::layout(desktop).await?,
    )
    .await?;
    checked(desktop,r#"(()=>{const b=[...document.querySelectorAll('.canvas-toolbar button')].find(b=>b.textContent==='Review saved change');if(!b||b.disabled)return {ok:false};b.id='gap-review-open';return {ok:true};})()"#).await?;
    click(desktop, "#gap-review-open").await?;
    wait(desktop,r#"({ok:!!document.querySelector('[data-review-insertion]')&&!document.querySelector('.canvas-local-review input')?.disabled})"#).await?;
    capture(
        state,
        "review-open",
        &mut ordinal,
        check::layout(desktop).await?,
    )
    .await?;
    let owner = parent(&mode());
    let child = seed::child(&source, owner);
    let anchor = if side() == "after-owner" {
        owner
    } else {
        &child
    };
    let expected_side = if side() == "after-owner" {
        "after"
    } else {
        "before"
    };
    checked(desktop,&format!(r#"(()=>{{const e=document.querySelector('[data-review-insertion]');return {{ok:e?.getAttribute('data-anchor')==={}&&e?.getAttribute('data-side')==={}}};}})()"#,json!(anchor),json!(expected_side))).await?;
    capture(
        state,
        "review-intended",
        &mut ordinal,
        section(desktop, "[data-review-insert-draft]", "intended", &text(1)).await?,
    )
    .await?;
    let current = if side() == "after-owner" {
        texts(&corpus()).0
    } else {
        texts(&corpus()).1
    };
    capture(
        state,
        "review-observed",
        &mut ordinal,
        section(desktop, "[data-review-insert-current]", "observed", current).await?,
    )
    .await?;
    checked(desktop,r#"(()=>{const e=document.querySelector('.canvas-local-review .canvas-text-actions');if(!e)return {ok:false};e.scrollIntoView({block:'end'});return {ok:true};})()"#).await?;
    let geometry=wait(desktop,r#"(()=>{const p=document.querySelector('.canvas-local-review'),e=p?.querySelector('.canvas-text-actions'),r=e?.getBoundingClientRect(),c=e?.closest('.canvas-scroll')?.getBoundingClientRect(),input=p?.querySelector('input'),buttons=[...p?.querySelectorAll('button')||[]],cancel=buttons.find(b=>b.textContent==='Cancel'),remove=buttons.find(b=>b.textContent==='Remove local edit lock');return {ok:!!r&&!!c&&r.left>=Math.max(0,c.left)-1&&r.right<=Math.min(innerWidth,c.right)+1&&r.top>=Math.max(0,c.top)-1&&r.bottom<=Math.min(innerHeight,c.bottom)+1&&!!input&&!input.checked&&!input.disabled&&!!cancel&&!cancel.disabled&&!!remove&&remove.disabled,acknowledged:input?.checked,cancelEnabled:!!cancel&&!cancel.disabled,removeDisabled:!!remove&&remove.disabled,rect:r?[r.x,r.y,r.width,r.height]:null,clip:c?[c.x,c.y,c.width,c.height]:null,viewport:[innerWidth,innerHeight]};})()"#).await?;
    capture(state, "review-controls", &mut ordinal, geometry).await?;
    if !same_source(&state.peek(), &source)
        || recovery::stored().await?.as_ref() != Some(&receipt)
        || !fixture(&mut state.write()).writes.is_empty()
    {
        return Err("Inert review changed canonical/custody or replayed".into());
    }
    let mut result = terminal(state, desktop, ordinal).await?;
    result["noReplay"] = json!(true);
    Ok(result)
}
