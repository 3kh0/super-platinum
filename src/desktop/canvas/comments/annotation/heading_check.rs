//! Named offline native selection, ordinary journal and no-replay acceptance.
use super::*;
use crate::canvas::text::check::{checked, click, wait};
use fixture::heading as data;
use serde_json::{Value, json};
#[path = "heading_check/capture.rs"]
mod capture;
#[path = "heading_check/diagnostics.rs"]
mod diagnostics;

pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !data::requested()
        || std::env::var("SUPER_PLATINUM_CANVAS_ANNOTATION_CHECK").as_deref() != Ok("1")
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = match run(state, &desktop).await {
            Ok(v) => v,
            Err(e) => {
                let diagnostic = diagnostics::capture(state, &desktop).await;
                json!({"ok":false,"error":e,"fixture":data::proof(&state.peek()),"model":model(&state.peek()),"diagnostic":diagnostic})
            }
        };
        // Full retained stage/journal proof can be large. Keep synchronous JSON
        // formatting and stderr I/O off the native event loop so its ownership
        // hit-test remains responsive while the unchanged receipt is emitted.
        let _ = tokio::task::spawn_blocking(move || {
            eprintln!("{}-check: {result}", data::receipt_prefix());
        })
        .await;
    });
}
async fn wait_named(
    desktop: &dioxus::desktop::DesktopContext,
    phase: &str,
    script: &str,
) -> Result<Value, String> {
    wait(desktop, script)
        .await
        .map_err(|e| format!("{phase}: {e}"))
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
    id: &str,
    start: usize,
    end: usize,
) -> Result<Value, String> {
    let script = include_str!("heading_check/select.js")
        .replace("__ID__", &json!(id).to_string())
        .replace("__START__", &start.to_string())
        .replace("__END__", &end.to_string());
    checked(desktop, &script).await
}
async fn author(desktop: &dioxus::desktop::DesktopContext, phase: &str) -> Result<(), String> {
    wait_named(desktop, phase,r#"({ok:!!document.getElementById('canvas-comment-create')&&!document.getElementById('canvas-comment-create').disabled&&!document.querySelector('.canvas-comments-scroll[aria-busy=true]')})"#).await?;
    Ok(())
}
async fn back(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    click(desktop, "#canvas-comments-back").await?;
    wait_named(desktop, "back to reader",r#"({ok:!document.getElementById('canvas-comments')&&!!document.querySelector('[data-canvas-text-section=heading-one] .canvas-annotation-text')})"#).await?;
    Ok(())
}
async fn reader_refresh(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    let old=checked(desktop,r#"(()=>{const n=document.querySelector('[data-canvas-text-section=heading-one]');return {ok:!!n,read:n?.dataset.canvasTextRead};})()"#).await?;
    checked(desktop,r#"(()=>{const b=[...document.querySelectorAll('#shell-overlay-dialog button')].find(b=>b.textContent==='Refresh');if(!b||b.disabled)return {ok:false};b.click();return {ok:true};})()"#).await?;
    wait_named(desktop, "reader refresh new read",&format!(r#"({{ok:!!document.querySelector('[data-canvas-text-section=heading-one] .canvas-annotation-text')&&document.querySelector('[data-canvas-text-section=heading-one]')?.dataset.canvasTextRead!=={}&&!document.getElementById('canvas-comment-selection')?.disabled}})"#,old["read"])).await?;
    Ok(())
}
fn writes(shell: &ShellState) -> usize {
    data::proof(shell)["prepared"]
        .as_array()
        .map_or(0, Vec::len)
}
async fn create(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    expected: usize,
) -> Result<(), String> {
    checked(desktop,r#"(()=>{const b=document.getElementById('canvas-comment-create');if(!b||b.disabled)return {ok:false};b.click();b.click();return {ok:true};})()"#).await?;
    wait_named(desktop, "created result readiness",r#"({ok:[...document.querySelectorAll('#canvas-comments button')].some(b=>b.textContent==='Open created thread'&&!b.disabled)})"#).await?;
    if writes(&state.peek()) != expected {
        return Err("Duplicate creation or missing anchor/bootstrap".into());
    }
    Ok(())
}
async fn finish_review(
    desktop: &dioxus::desktop::DesktopContext,
    phase: &str,
) -> Result<(), String> {
    button(desktop, "Close annotation review").await?;
    back(desktop).await?;
    reader_refresh(desktop)
        .await
        .map_err(|e| format!("{phase}: {e}"))
}
async fn run(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    wait_named(desktop, "initial heading reader",r#"({ok:!!document.querySelector('[data-canvas-text-section=heading-one] h1.canvas-annotation-text')&&!document.getElementById('canvas-comment-selection')?.disabled})"#).await?;
    if !data::enabled(&state.peek()) {
        return Err("Not the closed headings fixture".into());
    }
    let initial = data::proof(&state.peek());
    let mut stages = Vec::new();
    let mut selections = Vec::new();
    capture::stage(state, desktop, "initial", data::IDS[0], &mut stages).await?;
    selections.push(selected(desktop, data::IDS[0], 1, 4).await?);
    wait_named(desktop, "existing H1 thread",r#"({ok:!!document.getElementById('canvas-comments')&&document.getElementById('canvas-comments').textContent.includes('Existing comment')&&!document.querySelector('.canvas-comments-scroll[aria-busy=true]')})"#).await?;
    if writes(&state.peek()) != 0 {
        return Err("Existing heading control wrote".into());
    }
    capture::stage(state, desktop, "existing", data::IDS[0], &mut stages).await?;
    back(desktop).await?;
    selections.push(selected(desktop, data::IDS[0], 6, 14).await?);
    author(desktop, "H1 initial selection").await?;
    capture::stage(state, desktop, "h1-selected", data::IDS[0], &mut stages).await?;
    create(state, desktop, 2).await?;
    capture::stage(state, desktop, "h1-created", data::IDS[0], &mut stages).await?;
    finish_review(desktop, "after H1 creation").await?;
    capture::stage(state, desktop, "h1-canonical", data::IDS[0], &mut stages).await?;
    selections.push(selected(desktop, data::IDS[1], 6, 14).await?);
    author(desktop, "H2 initial selection").await?;
    capture::stage(state, desktop, "h2-selected", data::IDS[1], &mut stages).await?;
    let stale = data::advance(&state.peek()).map_err(|_| "Could not advance original H2")?;
    click(desktop, "#canvas-comment-create").await?;
    wait_named(desktop, "H2 stale refusal",r#"({ok:document.querySelector('.canvas-comment-author [role=alert]')?.textContent==='The paragraph or access changed. Refresh before creating this comment.'})"#).await?;
    if writes(&state.peek()) != 2 {
        return Err("Stale H2 wrote".into());
    }
    capture::stage(state, desktop, "h2-stale", data::IDS[1], &mut stages).await?;
    finish_review(desktop, "after H2 stale refusal").await?;
    selections.push(selected(desktop, data::IDS[1], 6, 14).await?);
    author(desktop, "H2 fresh reselection").await?;
    create(state, desktop, 4).await?;
    capture::stage(state, desktop, "h2-created", data::IDS[1], &mut stages).await?;
    finish_review(desktop, "after H2 creation").await?;
    capture::stage(state, desktop, "h2-canonical", data::IDS[1], &mut stages).await?;
    selections.push(selected(desktop, data::IDS[2], 6, 14).await?);
    author(desktop, "H3 selection").await?;
    capture::stage(state, desktop, "h3-selected", data::IDS[2], &mut stages).await?;
    create(state, desktop, 6).await?;
    capture::stage(state, desktop, "h3-created", data::IDS[2], &mut stages).await?;
    finish_review(desktop, "after H3 creation").await?;
    capture::stage(state, desktop, "h3-canonical", data::IDS[2], &mut stages).await?;
    capture::stage(state, desktop, "twin", data::IDS[3], &mut stages).await?;
    Ok(
        json!({"ok":true,"noSession":true,"noTransport":true,"existingReadOnly":true,"staleNoWrite":true,"createdOnce":true,"canonicalReadback":true,"syntheticInput":true,"writes":6,"initial":initial,"fixture":data::proof(&state.peek()),"stale":stale,"selections":selections,"layout":stages.last().unwrap()["geometry"],"stages":stages}),
    )
}
