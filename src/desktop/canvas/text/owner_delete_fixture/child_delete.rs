//! Delete an already saved split child without discarding its original owner.
use super::*;
use crate::canvas::text::check::{checked, click, editor_layout, set_draft, wait};
use std::sync::atomic::{AtomicBool, Ordering};
mod interaction;
mod review;
#[cfg(test)]
mod tests;
const DOM: &str = include_str!("live_rich/dom.js");
static STARTED: AtomicBool = AtomicBool::new(false);
pub(super) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-live-split-child-delete" | "canvas-live-split-child-delete-narrow")
        )
}
fn variant() -> String {
    env("SUPER_PLATINUM_CANVAS_CHILD_DELETE_VARIANT", "plain")
}
pub(super) fn coalesced() -> bool {
    requested() && variant() != "plain"
}
fn rich_gap() -> bool {
    variant() == "rich-gap"
}
fn owner_format() -> &'static str {
    if corpus() == "unicode" {
        "link"
    } else {
        "bold"
    }
}
fn owner_text() -> String {
    let left = texts(&corpus()).0;
    if rich_gap() {
        format!("{left} revised")
    } else {
        left.into()
    }
}
fn edited_text() -> String {
    format!("{} edited", owner_text())
}
fn config() -> Value {
    json!({"variant":variant(),"corpus":corpus(),"history":mode(),"gap":rich_gap(),"addAfter":variant()=="plain","richChild":rich_gap(),"ownerFormat":owner_format(),"phase":phase()})
}
pub(super) fn add_metadata(value: &mut Value) {
    value["childDelete"] = config();
}
fn valid() -> bool {
    matches!(
        (variant().as_str(), corpus().as_str(), mode().as_str()),
        ("plain", "ascii", "bootstrap")
            | ("revert", "ascii", "retained")
            | ("rich-gap", "unicode", "retained")
    ) && matches!(
        phase().as_str(),
        "success" | "stale" | "unknown" | "omitted" | "restart"
    )
}
pub(super) fn seed(corpus: &str, mode: &str) -> Arc<CanvasCollabSnapshot> {
    seed_variant(corpus, mode, &variant())
}
pub(super) fn seed_variant(corpus: &str, mode: &str, variant: &str) -> Arc<CanvasCollabSnapshot> {
    let source = seed::seed_child_delete(corpus, mode, variant);
    if variant != "rich-gap" {
        return source;
    }
    let child = seed::child(&source, parent(mode));
    let intent = TextIntent::capture(&source, &child, 100).unwrap();
    let mut session = super_platinum_core::canvas::edit::EditSession::new();
    let desired = TextDraft::from_rtml(&format!(
        "<a href=\"https://example.test/\">{}</a>",
        escaped(texts(corpus).1)
    ))
    .unwrap();
    let prepared = intent
        .prepare(&source, &mut session, &desired, 100)
        .unwrap();
    let (accepted, linked, _) = completion::text(prepared, source.clone());
    assert!(accepted.confirms_readback(&linked).unwrap());
    unchanged(&source, &linked, &child).unwrap();
    use super_platinum_core::canvas::edit::structure::{Side, StructureIntent};
    let p = StructureIntent::insert(
        &linked,
        parent(mode),
        Side::After,
        &TextDraft::from_rtml("Independent paragraph").unwrap(),
        100,
    )
    .unwrap()
    .prepare(&linked, 100)
    .unwrap();
    let (accepted, fresh, _) = completion::delete(p, linked.clone());
    assert!(accepted.confirms_readback(&fresh).unwrap());
    assert!(
        linked
            .sections
            .iter()
            .all(|s| fresh.section(s.id()) == Some(s))
    );
    fresh
}
pub(super) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !requested()
        || env("SUPER_PLATINUM_CANVAS_LIVE_SPLIT_CHILD_DELETE_CHECK", "0") != "1"
        || STARTED.swap(true, Ordering::Relaxed)
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = match interaction::run(state, &desktop).await {
            Ok(v) => v,
            Err(e) => json!({"ok":false,"error":e}),
        };
        eprintln!("canvas-live-split-child-delete-check: {result}");
    });
}
async fn capture(
    state: Signal<ShellState>,
    name: &str,
    ordinal: &mut usize,
    geometry: Value,
) -> Result<(), String> {
    check::stage(state, name, *ordinal, geometry).await?;
    *ordinal += 1;
    Ok(())
}
fn script(call: &str) -> String {
    format!("{DOM}\nwindow.superPlatinumLiveRichCheck.{call}")
}
async fn row(
    desktop: &dioxus::desktop::DesktopContext,
    id: &str,
    focus: bool,
    delete: bool,
) -> Result<Value, String> {
    wait(desktop, &script(&format!("show({})", json!(id)))).await?;
    check::row_actions(desktop, id, focus, delete).await
}
async fn open(
    desktop: &dioxus::desktop::DesktopContext,
    id: &str,
    deleting: bool,
) -> Result<(), String> {
    row(desktop, id, false, true).await?;
    let selector = if deleting {
        "[data-structure-action=delete]"
    } else {
        ".canvas-text-open[aria-label='Edit paragraph']"
    };
    click(
        desktop,
        &format!("[data-canvas-text-section={}] {selector}", json!(id)),
    )
    .await?;
    check::editor_ready(desktop, deleting).await?;
    editor_layout(desktop).await?;
    Ok(())
}
async fn save(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    text: bool,
) -> Result<(), String> {
    let (count, reservations) = {
        let mut shell = state.write();
        let f = fixture(&mut shell);
        (f.writes.len(), f.reservations)
    };
    checked(desktop,r#"(()=>{const b=document.getElementById('canvas-text-save');if(!b||b.disabled)return {ok:false};b.click();b.click();return {ok:true};})()"#).await?;
    wait(desktop,r#"({ok:document.getElementById('canvas-text-save')?.disabled&&document.getElementById('canvas-text-cancel')?.disabled&&document.getElementById('canvas-text-save')?.textContent.includes('Saving')})"#).await?;
    for _ in 0..600 {
        let ready = {
            let mut shell = state.write();
            let f = fixture(&mut shell);
            if text {
                f.pending_text.is_some()
            } else {
                f.pending.is_some()
            }
        };
        if ready {
            let valid = {
                let mut shell = state.write();
                let f = fixture(&mut shell);
                f.writes.len() == count + 1 && f.reservations == reservations + usize::from(!text)
            };
            if !valid || (!text && recovery::stored().await?.is_none()) {
                return Err("Duplicate save/custody guard failed".into());
            }
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err("Save did not dispatch".into())
}
async fn finish(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    text: bool,
) -> Result<(), String> {
    if text {
        completion::complete_text(state).await?;
    } else {
        completion::complete(state, false).await?;
    }
    wait(
        desktop,
        r#"({ok:!document.getElementById('canvas-text-editor')})"#,
    )
    .await?;
    if recovery::stored().await?.is_some() {
        return Err("Confirmed operation retained custody".into());
    }
    Ok(())
}
fn unchanged(
    before: &CanvasCollabSnapshot,
    after: &CanvasCollabSnapshot,
    target: &str,
) -> Result<(), String> {
    if before.sections.len() != after.sections.len()
        || before
            .sections
            .iter()
            .filter(|s| s.id() != target)
            .any(|s| after.section(s.id()) != Some(s))
    {
        return Err("Child deletion/owner edit changed another raw row".into());
    }
    Ok(())
}
fn canonical(source: &CanvasCollabSnapshot, id: &str, rtml: &str) -> Result<(), String> {
    let value = source
        .section(id)
        .and_then(|s| s.raw().message(12).ok().flatten())
        .and_then(|m| m.message(1).ok().flatten())
        .and_then(|m| m.string(1).ok().flatten().map(str::to_owned));
    if value.as_deref() != Some(rtml) {
        return Err("Canonical owner content differs".into());
    }
    Ok(())
}
async fn terminal(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    stages: usize,
) -> Result<Value, String> {
    let source = snapshot(&state.peek());
    let canonical = tokio::task::spawn_blocking(move || seed::saved(&source))
        .await
        .map_err(|_| "Evidence worker failed")?;
    Ok(
        json!({"ok":true,"childDelete":config(),"coalesced":coalesced(),"noHttp":true,"noSession":true,"noTransport":true,"cancelNoWrite":true,"duplicateGuard":true,"stages":stages,"canonical":canonical,"journal":recovery::observe().await?,"layout":check::layout(desktop).await?}),
    )
}
