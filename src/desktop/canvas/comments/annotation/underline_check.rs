//! Named offline native selection, ordinary journal and no-replay acceptance.
use super::*;
use crate::canvas::text::check::{checked, click, wait};
use fixture::underline as data;
use serde_json::{Value, json};
mod capture;

pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !data::requested()
        || std::env::var("SUPER_PLATINUM_CANVAS_ANNOTATION_CHECK").as_deref() != Ok("1")
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = match run(state, &desktop).await {
            Ok(v) => v,
            Err(e) => json!({"ok":false,"error":e,"fixture":data::proof(&state.peek())}),
        };
        // Full retained stage/journal proof can be large. Keep synchronous JSON
        // formatting and stderr I/O off the native event loop so its ownership
        // hit-test remains responsive while the unchanged receipt is emitted.
        let _ = tokio::task::spawn_blocking(move || {
            eprintln!("canvas-comment-underline-check: {result}");
        })
        .await;
    });
}
fn model(shell: &ShellState) -> Value {
    json!(shell.canvas_checklist.comment_annotations.entries.iter().map(|e|json!({
        "id":e.id,"busy":e.busy,"current":e.owner.current(shell),"error":e.error,"selected":e.selected,
        "range":e.selected_range.as_ref().map(|r|json!({"start":r.start,"end":r.end,"text":r.text})),
        "pending":e.pending,"created":e.created,"review":e.review.is_some(),
        "history":e.history.iter().map(|h|json!({"id":h.id,"status":h.status,"pending":h.pending,"observed":h.observed})).collect::<Vec<_>>()
    })).collect::<Vec<_>>())
}
async fn journal(state: Signal<ShellState>) -> Result<Value, String> {
    let disk = {
        let s = state.peek();
        let owner = Owner::capture(&s).ok_or("No original annotation owner")?;
        Disk::capture(&s, &owner).ok_or("No original journal")?
    };
    let scope = disk.scope.clone();
    disk.work(move|j|Ok(json!({"pending":j.annotation(&scope)?,"history":j.annotation_history(&scope,None)?.iter().map(|h|json!({"id":h.id,"status":h.status,"pending":h.pending,"observed":h.observed})).collect::<Vec<_>>()}))).await.map_err(|_|"Annotation journal unavailable".into())
}
async fn button(desktop: &dioxus::desktop::DesktopContext, label: &str) -> Result<(), String> {
    checked(desktop,&format!(r#"(()=>{{const b=[...document.querySelectorAll('#canvas-comments button')].find(b=>b.textContent==={});if(!b||b.disabled)return {{ok:false}};b.scrollIntoView({{block:'nearest'}});b.focus();b.click();return {{ok:true}};}})()"#,json!(label))).await?;
    Ok(())
}
async fn selected(
    desktop: &dioxus::desktop::DesktopContext,
    start: usize,
    end: usize,
) -> Result<Value, String> {
    let script = include_str!("underline_check/select.js")
        .replace("__START__", &start.to_string())
        .replace("__END__", &end.to_string());
    checked(desktop, &script).await
}
async fn author(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    wait(desktop,r#"({ok:!!document.getElementById('canvas-comment-create')&&!document.getElementById('canvas-comment-create').disabled&&!document.querySelector('.canvas-comments-scroll[aria-busy=true]')})"#).await?;
    Ok(())
}
async fn back(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    click(desktop, "#canvas-comments-back").await?;
    wait(desktop,r#"({ok:!document.getElementById('canvas-comments')&&!!document.querySelector('[data-canvas-text-section=paragraph] .canvas-annotation-text')})"#).await?;
    Ok(())
}
async fn reader_refresh(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    let before=checked(desktop,r#"(()=>{const row=document.querySelector('[data-canvas-text-section=paragraph]');return {ok:!!row,read:row?.dataset.canvasTextRead};})()"#).await?;
    checked(desktop,r#"(()=>{const b=[...document.querySelectorAll('#shell-overlay-dialog button')].find(b=>b.textContent==='Refresh');if(!b||b.disabled)return {ok:false};b.click();return {ok:true};})()"#).await?;
    wait(desktop,&format!(r#"({{ok:!!document.querySelector('[data-canvas-text-section=paragraph] .canvas-annotation-text')&&document.querySelector('[data-canvas-text-section=paragraph]')?.dataset.canvasTextRead!=={}&&!document.getElementById('canvas-comment-selection')?.disabled}})"#,before["read"])).await?;
    Ok(())
}
fn writes(shell: &ShellState) -> usize {
    data::proof(shell)["prepared"]
        .as_array()
        .map_or(0, Vec::len)
}
async fn reviewed_after(state: Signal<ShellState>, requests: usize) -> Result<(), String> {
    for _ in 0..120 {
        let ready = {
            let shell = state.peek();
            data::proof(&shell)["requests"]
                .as_array()
                .is_some_and(|r| r.len() > requests)
                && shell
                    .canvas_checklist
                    .comment_annotations
                    .entries
                    .iter()
                    .all(|e| !e.busy)
        };
        if ready {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err("Fresh original review did not settle".into())
}
async fn run(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    wait(desktop,r#"({ok:!!document.querySelector('[data-canvas-text-section=paragraph] .canvas-annotation-text')&&!document.getElementById('canvas-comment-selection')?.disabled})"#).await?;
    if !data::enabled(&state.peek()) {
        return Err("Not the closed underline fixture".into());
    }
    let initial = data::proof(&state.peek());
    let mut stages = Vec::new();
    let mut selections = Vec::new();
    capture::stage(state, desktop, "initial", &mut stages).await?;
    selections.push(selected(desktop, 1, 4).await?);
    wait(desktop,r#"({ok:!!document.getElementById('canvas-comments')&&document.getElementById('canvas-comments').textContent.includes('Existing comment')&&!document.querySelector('.canvas-comments-scroll[aria-busy=true]')})"#).await?;
    if writes(&state.peek()) != 0 {
        return Err("Existing thread routing wrote".into());
    }
    capture::stage(state, desktop, "existing", &mut stages).await?;
    back(desktop).await?;
    selections.push(selected(desktop, 5, 13).await?);
    author(desktop).await?;
    capture::stage(state, desktop, "selected", &mut stages).await?;
    let stale = data::advance(&state.peek()).map_err(|_| "Could not advance original source")?;
    click(desktop, "#canvas-comment-create").await?;
    wait(desktop,r#"({ok:document.querySelector('.canvas-comment-author [role=alert]')?.textContent==='The paragraph or access changed. Refresh before creating this comment.'})"#).await?;
    if writes(&state.peek()) != 0 {
        return Err("Stale selected source wrote".into());
    }
    capture::stage(state, desktop, "stale", &mut stages).await?;
    button(desktop, "Close annotation review").await?;
    back(desktop).await?;
    reader_refresh(desktop).await?;
    selections.push(selected(desktop, 5, 13).await?);
    author(desktop).await?;
    capture::stage(state, desktop, "selected-fresh", &mut stages).await?;
    // One ordinary invocation plus immediate duplicate activation; busy admission owns once-only dispatch.
    checked(desktop,r#"(()=>{const b=document.getElementById('canvas-comment-create');if(!b||b.disabled)return {ok:false};b.click();b.click();return {ok:true};})()"#).await?;
    wait(desktop,r#"({ok:[...document.querySelectorAll('#canvas-comments button')].some(b=>b.textContent==='Open created thread'&&!b.disabled)})"#).await?;
    if writes(&state.peek()) != 2 {
        return Err("Creation did not issue exactly anchor plus bootstrap".into());
    }
    capture::stage(state, desktop, "created", &mut stages).await?;
    button(desktop, "Close annotation review").await?;
    back(desktop).await?;
    reader_refresh(desktop).await?;
    capture::stage(state, desktop, "canonical", &mut stages).await?;
    selections.push(selected(desktop, 14, 18).await?);
    author(desktop).await?;
    capture::stage(state, desktop, "uncertain-selected", &mut stages).await?;
    data::lose_next(&state.peek()).map_err(|_| "Could not hold uncertain outcome")?;
    click(desktop, "#canvas-comment-create").await?;
    wait(desktop,&format!(r#"({{ok:document.querySelector('.canvas-comment-author [role=alert]')?.textContent==={}}})"#,json!(UNKNOWN))).await?;
    if writes(&state.peek()) != 3 {
        return Err("Uncertain annotation repeated bootstrap".into());
    }
    capture::stage(state, desktop, "uncertain", &mut stages).await?;
    for name in ["reviewed", "reviewed-again"] {
        let before = data::proof(&state.peek())["requests"]
            .as_array()
            .ok_or("Missing ledger")?
            .len();
        button(desktop, "Refresh current annotation state").await?;
        reviewed_after(state, before).await?;
        wait(desktop,r#"({ok:!!document.querySelector('.canvas-comment-reply-version')&&document.querySelector('.canvas-comment-author')?.textContent.includes('The original annotation control is present.')&&!document.querySelector('.canvas-comment-author')?.textContent.includes('Checking comment state...')})"#).await?;
        if writes(&state.peek()) != 3 {
            return Err("Review repeated an annotation write".into());
        }
        capture::stage(state, desktop, name, &mut stages).await?;
    }
    let final_proof = data::proof(&state.peek());
    Ok(
        json!({"ok":true,"noSession":true,"noTransport":true,"existingReadOnly":true,"staleNoWrite":true,"createdOnce":true,"canonicalReadback":true,"uncertainNoReplay":true,"syntheticInput":true,"writes":3,"initial":initial,"fixture":final_proof,"stale":stale,"selections":selections,"layout":stages.last().unwrap()["geometry"],"stages":stages}),
    )
}
