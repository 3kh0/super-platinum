//! Disconnected normal/narrow structural authoring workflow, with staged PNG ACKs.
use super::check::{checked, click, editor_layout, set_draft, wait};
use crate::state::ShellState;
use dioxus::prelude::*;
use serde_json::{Value, json};
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if std::env::var("SUPER_PLATINUM_CANVAS_STRUCTURE_CHECK").as_deref() != Ok("1")
        || !super::structure_fixture::requested()
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let receipt = match run(state, &desktop).await {
            Ok(receipt) => receipt,
            Err(error) => json!({"ok":false,"error":error}),
        };
        eprintln!("canvas-structure-check: {receipt}");
    });
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    wait(desktop,r#"({ok:!!document.querySelector('[data-canvas-text-section="paragraph"] [data-structure-action="after"]:not(:disabled)')})"#).await?;
    if !super::structure_fixture::enabled(&state.peek()) {
        return Err("Expected disconnected structural fixture".into());
    }
    let original = super::checklist::presentation(&state.peek())
        .unwrap()
        .snapshot
        .unwrap();
    capture_stage(state, "initial", 0).await?;
    click(
        desktop,
        r#"[data-canvas-text-section="paragraph"] [data-structure-action="before"]"#,
    )
    .await?;
    wait(
        desktop,
        r#"({ok:!!document.getElementById('canvas-text-draft')?.isContentEditable})"#,
    )
    .await?;
    set_draft(desktop, "<div><strong>Cancelled paragraph</strong></div>").await?;
    click(desktop, "#canvas-text-cancel").await?;
    wait(
        desktop,
        r#"({ok:!document.getElementById('canvas-text-editor')})"#,
    )
    .await?;
    if super::structure_fixture::attempts(&state.peek()) != 0 {
        return Err("Cancel sent a mutation".into());
    }
    click(
        desktop,
        r#"[data-canvas-text-section="paragraph"] [data-structure-action="after"]"#,
    )
    .await?;
    set_draft(
        desktop,
        "<div><strong>Inserted paragraph</strong> &amp; details</div>",
    )
    .await?;
    editor_layout(desktop).await?;
    capture_stage(state, "insert-draft", 1).await?;
    checked(desktop,r#"(()=>{const b=document.getElementById('canvas-text-save');b.click();b.click();return {ok:true};})()"#).await?;
    wait(desktop,r#"({ok:document.getElementById('canvas-text-save')?.disabled&&document.getElementById('canvas-text-cancel')?.disabled})"#).await?;
    if super::structure_fixture::attempts(&state.peek()) != 1 {
        return Err("Duplicate insertion dispatched".into());
    }
    capture_stage(state, "insert-saving", 2).await?;
    if !super::structure_fixture::complete(&mut state.write()) {
        return Err("Insertion did not confirm".into());
    }
    wait(desktop,r#"({ok:!document.getElementById('canvas-text-editor')&&[...document.querySelectorAll('[data-canvas-text-section]')].some(e=>e.textContent.includes('Inserted paragraph')&&e.querySelector('[data-structure-action="delete"]'))})"#).await?;
    let current = super::checklist::presentation(&state.peek())
        .unwrap()
        .snapshot
        .unwrap();
    let inserted = current
        .sections
        .iter()
        .find(|s| !original.sections.iter().any(|old| old.id() == s.id()))
        .ok_or("Inserted identity missing")?
        .id()
        .to_owned();
    capture_stage(state, "inserted", 3).await?;
    let remove = format!(
        "[data-canvas-text-section={}] [data-structure-action=delete]",
        json!(inserted)
    );
    click(desktop, &remove).await?;
    editor_layout(desktop).await?;
    checked(desktop,r#"({ok:document.getElementById('canvas-text-draft')?.contentEditable==='false'&&document.getElementById('canvas-text-save')?.textContent==='Delete paragraph'&&document.activeElement?.id==='canvas-text-cancel'})"#).await?;
    capture_stage(state, "delete-confirmation", 4).await?;
    click(desktop, "#canvas-text-cancel").await?;
    wait(
        desktop,
        r#"({ok:!document.getElementById('canvas-text-editor')})"#,
    )
    .await?;
    if super::structure_fixture::attempts(&state.peek()) != 1 {
        return Err("Delete Cancel sent a mutation".into());
    }
    click(desktop, &remove).await?;
    click(desktop, "#canvas-text-save").await?;
    wait(
        desktop,
        r#"({ok:document.getElementById('canvas-text-cancel')?.disabled})"#,
    )
    .await?;
    capture_stage(state, "delete-saving", 5).await?;
    if !super::structure_fixture::complete(&mut state.write()) {
        return Err("Deletion did not confirm".into());
    }
    wait(desktop,r#"({ok:!document.getElementById('canvas-text-editor')&&document.querySelectorAll('[data-canvas-text-section]').length===2})"#).await?;
    let restored = super::checklist::presentation(&state.peek())
        .unwrap()
        .snapshot
        .unwrap();
    if restored.sections != original.sections {
        return Err("Original complete section inventory was not restored".into());
    }
    capture_stage(state, "restored", 6).await?;
    click(
        desktop,
        r#"[data-canvas-text-section="paragraph"] [data-structure-action="after"]"#,
    )
    .await?;
    set_draft(desktop, "<div>Retain stale draft</div>").await?;
    let team = state.peek().core.active_team.clone().unwrap();
    super::checklist::invalidate(&mut state.write(), &team, "Ffixture");
    wait(desktop,r#"({ok:document.getElementById('canvas-text-save')?.disabled&&!document.getElementById('canvas-text-cancel')?.disabled&&document.getElementById('canvas-text-draft')?.textContent.includes('Retain stale draft')})"#).await?;
    capture_stage(state, "stale", 7).await?;
    click(desktop, "#canvas-text-cancel").await?;
    super::structure_fixture::refresh(&mut state.write());
    click(
        desktop,
        r#"[data-canvas-text-section="paragraph"] [data-structure-action="after"]"#,
    )
    .await?;
    set_draft(desktop, "<div>Uncertain paragraph</div>").await?;
    click(desktop, "#canvas-text-save").await?;
    wait(
        desktop,
        r#"({ok:document.getElementById('canvas-text-cancel')?.disabled})"#,
    )
    .await?;
    if !super::structure_fixture::uncertain(&mut state.write()) {
        return Err("Uncertain attempt was not retained".into());
    }
    wait(
        desktop,
        r#"({ok:document.getElementById('canvas-text-save')?.textContent==='Unconfirmed'})"#,
    )
    .await?;
    let layout = editor_layout(desktop).await?;
    capture_stage(state, "uncertain", 8).await?;
    let target = state.peek().canvas.clone().unwrap();
    state.write().dismiss_overlay();
    wait(
        desktop,
        r#"({ok:!document.querySelector('.canvas-dialog')})"#,
    )
    .await?;
    crate::canvas::open_target(state, target);
    wait(desktop,r#"({ok:!!document.querySelector('.canvas-dialog')&&document.querySelector('.canvas-dialog').textContent.includes('Could not confirm')&&[...document.querySelectorAll('.canvas-paragraph-actions button')].every(b=>b.disabled)})"#).await?;
    if super::structure_fixture::attempts(&state.peek()) != 3 {
        return Err("Unexpected structural request count".into());
    }
    capture_stage(state, "reopened", 9).await?;
    Ok(
        json!({"ok":true,"inserted":true,"deleteConfirmed":true,"cancelSafe":true,"duplicateSafe":true,"originalRestored":true,"stalePreserved":true,"uncertainRetained":true,"reopenBlocked":true,"attempts":3,"noSession":state.peek().core.session.is_none(),"noTransport":state.peek().core.transport.is_none(),"layout":layout}),
    )
}

async fn capture_stage(
    state: Signal<ShellState>,
    stage: &str,
    ordinal: usize,
) -> Result<(), String> {
    if !super::structure_fixture::enabled(&state.peek())
        || state.peek().core.session.is_some()
        || state.peek().core.transport.is_some()
    {
        return Err("Canvas capture requires a disconnected fixture".into());
    }
    // Older direct check invocations remain valid; the portable runner always
    // supplies a fresh private directory and requires every captured stage.
    let Some(directory) = std::env::var_os("SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR") else {
        return Ok(());
    };
    let directory = std::path::PathBuf::from(directory);
    if !directory.is_absolute() {
        return Err("Canvas capture acknowledgement directory must be absolute".into());
    }
    let ack = directory.join(format!("{ordinal}.ack"));
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    eprintln!(
        "canvas-structure-stage: {}",
        json!({"ok":true,"stage":stage,"ordinal":ordinal,"noSession":true,"noTransport":true})
    );
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let path = ack.clone();
        match tokio::task::spawn_blocking(move || std::fs::read(path))
            .await
            .map_err(|_| "Canvas capture acknowledgement task failed")?
        {
            Ok(bytes) if bytes == b"captured\n" => return Ok(()),
            Ok(_) => return Err("Canvas capture acknowledgement was malformed".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err("Canvas capture acknowledgement could not be read".into()),
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!(
                "Canvas capture acknowledgement timed out at {stage}"
            ));
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}
