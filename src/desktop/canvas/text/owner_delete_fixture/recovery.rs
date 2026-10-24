//! Restart/local-review observations remain inert and never replay requests.
use super::*;
use crate::canvas::text::check::{checked, click, wait};
use super_platinum_core::canvas::edit::structure::recovery::{
    Scope,
    journal::{Journal, Receipt},
};
fn path() -> Result<std::path::PathBuf, String> {
    std::env::var_os("SUPER_PLATINUM_CANVAS_RECOVERY_DB")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("Missing isolated journal".into())
}
fn scope() -> Scope {
    Scope {
        workspace: "T1".into(),
        user: "U1".into(),
        file: "Ffixture".into(),
    }
}
pub(super) async fn stored() -> Result<Option<Receipt>, String> {
    let p = path()?;
    tokio::task::spawn_blocking(move || Journal::open(p, ACCOUNT).and_then(|j| j.get(&scope())))
        .await
        .map_err(|_| "Journal worker failed")?
        .map_err(|_| "Journal read failed".into())
}
pub(super) async fn observe() -> Result<Value, String> {
    let p = path()?;
    tokio::task::spawn_blocking(move||{let j=Journal::open(p,ACCOUNT)?;Ok::<_,super_platinum_core::error::AppError>(json!({"active":j.get(&scope())?.map(|r|json!({"operation":r.operation_id(),"checkpoint":r.checkpoint()})),"archiveCount":j.local_review_count(&scope())?}))}).await.map_err(|_|"Journal worker failed")?.map_err(|_|"Journal observation failed".into())
}
async fn unchanged(
    state: Signal<ShellState>,
    source: &Arc<CanvasCollabSnapshot>,
    before: &Receipt,
) -> Result<(), String> {
    if !same_source(&state.peek(), source) || stored().await?.as_ref() != Some(before) {
        return Err("Recovery canonical or receipt changed".into());
    }
    Ok(())
}
async fn locked(desktop: &dioxus::desktop::DesktopContext) -> Result<Value, String> {
    wait(desktop,r#"(()=>{const a=[...document.querySelectorAll('.canvas-paragraph-actions button')];return {ok:a.length>0&&a.every(b=>b.disabled)&&document.body.textContent.includes('unconfirmed'),viewport:[innerWidth,innerHeight]};})()"#).await
}
async fn open(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    checked(desktop,r#"(()=>{const b=[...document.querySelectorAll('.canvas-toolbar button')].find(b=>b.textContent==='Review saved change');if(!b||b.disabled)return {ok:false};b.id='owner-delete-review-open';return {ok:true};})()"#).await?;
    click(desktop, "#owner-delete-review-open").await?;
    wait(desktop,r#"({ok:!!document.querySelector('[data-review-owner-delete=child]')&&!document.querySelector('.canvas-local-review input')?.disabled})"#).await?;
    Ok(())
}
async fn section(
    desktop: &dioxus::desktop::DesktopContext,
    name: &str,
    expected: &str,
) -> Result<Value, String> {
    checked(desktop,&format!(r#"(()=>{{const e=document.querySelector('[data-review-owner-delete="{name}"]');if(!e)return {{ok:false}};e.scrollIntoView({{block:'center'}});return {{ok:true}};}})()"#)).await?;
    wait(desktop,&format!(r#"(()=>{{const e=document.querySelector('[data-review-owner-delete="{name}"]'),r=e?.getBoundingClientRect(),c=e?.closest('.canvas-scroll')?.getBoundingClientRect(),text=e?.querySelector('.canvas-review-preview')?.textContent;return {{ok:!!r&&!!c&&text==={}&&r.left>=Math.max(0,c.left)-1&&r.right<=Math.min(innerWidth,c.right)+1&&r.top>=Math.max(0,c.top)-1&&r.bottom<=Math.min(innerHeight,c.bottom)+1,section:{},text,rect:r?[r.x,r.y,r.width,r.height]:null,clip:c?[c.x,c.y,c.width,c.height]:null,viewport:[innerWidth,innerHeight]}};}})()"#,json!(expected),json!(name))).await
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let phase = phase();
    wait(
        desktop,
        r#"({ok:!!document.querySelector('[data-canvas-text-section]')})"#,
    )
    .await?;
    // Install and journal load may finish after the first static reader frame.
    for _ in 0..600 {
        if enabled(&state.peek())
            && checklist::presentation(&state.peek())
                .is_some_and(|v| v.snapshot.is_some() && !v.loading)
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let source = snapshot(&state.peek());
    let owner = parent(&mode());
    let child = seed::child(&source, owner);
    if source
        .section(owner)
        .and_then(|s| s.deleted().ok().flatten())
        != Some(true)
    {
        return Err("Recovery requires retained owner tombstone".into());
    }
    if phase == "reviewed" {
        if stored().await?.is_some() || observe().await?["archiveCount"] != 1 {
            return Err("Archived decision missing".into());
        }
        check::stage(
            state,
            "archive-restored",
            0,
            check::row(desktop, &child, false).await?,
        )
        .await?;
    } else {
        let before = stored().await?.ok_or("Missing inert deletion receipt")?;
        let checkpoint = before.checkpoint().clone();
        let retained_child = tokio::task::spawn_blocking(move || checkpoint.split_owner_child())
            .await
            .map_err(|_| "Owner proof worker failed")?
            .map_err(|_| "Invalid owner proof")?;
        if retained_child.as_ref() != source.section(&child) {
            return Err("Retained child differs".into());
        }
        check::stage(state, "restored-locked", 0, locked(desktop).await?).await?;
        if phase == "restart" {
            reload(state).await;
            unchanged(state, &source, &before).await?;
            check::stage(state, "refresh-locked", 1, locked(desktop).await?).await?;
        } else {
            open(desktop).await?;
            // Cancel leaves exact durable row and canonical bytes untouched.
            checked(desktop,r#"(()=>{const p=document.querySelector('.canvas-local-review'),b=[...p.querySelectorAll('button')].find(b=>b.textContent==='Cancel');b.click();return {ok:true};})()"#).await?;
            wait(
                desktop,
                r#"({ok:!document.querySelector('.canvas-local-review')})"#,
            )
            .await?;
            unchanged(state, &source, &before).await?;
            open(desktop).await?;
            let (left, tail) = texts(&corpus());
            for (ordinal, (name, text)) in
                [("original", left), ("child", tail), ("current-child", tail)]
                    .into_iter()
                    .enumerate()
            {
                let g = section(desktop, name, text).await?;
                unchanged(state, &source, &before).await?;
                check::stage(state, &format!("review-{name}"), ordinal + 1, g).await?;
            }
            checked(desktop,r#"(()=>{const p=document.querySelector('.canvas-local-review'),a=p.querySelector('.canvas-text-actions');a.scrollIntoView({block:'end'});return {ok:true};})()"#).await?;
            let g=wait(desktop,r#"(()=>{const p=document.querySelector('.canvas-local-review'),a=p?.querySelector('.canvas-text-actions'),r=a?.getBoundingClientRect(),c=a?.closest('.canvas-scroll')?.getBoundingClientRect();return {ok:!!r&&!!c&&r.top>=Math.max(0,c.top)-1&&r.bottom<=Math.min(innerHeight,c.bottom)+1&&r.left>=Math.max(0,c.left)-1&&r.right<=Math.min(innerWidth,c.right)+1&&!p.querySelector('input').checked&&[...p.querySelectorAll('button')].find(b=>b.textContent==='Remove local edit lock')?.disabled,rect:r?[r.x,r.y,r.width,r.height]:null,clip:c?[c.x,c.y,c.width,c.height]:null,viewport:[innerWidth,innerHeight]};})()"#).await?;
            check::stage(state, "review-controls", 4, g).await?;
            checked(desktop,r#"(()=>{const p=document.querySelector('.canvas-local-review');p.querySelector('input').click();return {ok:true};})()"#).await?;
            wait(desktop,r#"({ok:[...document.querySelectorAll('.canvas-local-review button')].some(b=>b.textContent==='Remove local edit lock'&&!b.disabled)})"#).await?;
            checked(desktop,r#"(()=>{[...document.querySelectorAll('.canvas-local-review button')].find(b=>b.textContent==='Remove local edit lock').click();return {ok:true};})()"#).await?;
            if phase == "storage-failure" {
                wait(desktop,r#"({ok:document.querySelector('.canvas-local-review [role=alert]')?.textContent.includes('remains locked')})"#).await?;
                unchanged(state, &source, &before).await?;
                check::stage(state, "storage-failed", 5, check::layout(desktop).await?).await?;
            } else {
                wait(desktop,r#"({ok:!document.querySelector('.canvas-local-review')&&document.body.textContent.includes('Refresh')})"#).await?;
                if stored().await?.is_some() || observe().await?["archiveCount"] != 1 {
                    return Err("Decision not atomically archived".into());
                }
                check::stage(state, "removed", 5, check::layout(desktop).await?).await?;
                reload(state).await;
                check::stage(
                    state,
                    "refreshed",
                    6,
                    check::row(desktop, &child, false).await?,
                )
                .await?;
            }
        }
    }
    if !fixture(&mut state.write()).writes.is_empty() {
        return Err("Recovery replayed a write".into());
    }
    Ok(
        json!({"ok":true,"phase":phase,"noSession":true,"noTransport":true,"noHttp":true,"noReplay":true,"layout":check::layout(desktop).await?,"journal":observe().await?}),
    )
}
