//! Owner coalescence is distinct from the previously verified child-coalesced seed.
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
            Ok("canvas-coalesced-owner-rich" | "canvas-coalesced-owner-rich-narrow")
        )
}
fn cadence() -> String {
    env("SUPER_PLATINUM_CANVAS_COALESCED_OWNER_CADENCE", "revert")
}
fn has_gap() -> bool {
    env("SUPER_PLATINUM_CANVAS_COALESCED_OWNER_GAP", "0") == "1"
}
fn format() -> &'static str {
    if corpus() == "unicode" {
        "link"
    } else {
        "bold"
    }
}
fn config() -> Value {
    json!({"corpus":corpus(),"history":mode(),"parentCadence":cadence(),"format":format(),"gap":has_gap(),"phase":phase()})
}
pub(super) fn add_metadata(value: &mut Value) {
    value["ownerRich"] = config();
}
fn owner_text() -> String {
    match cadence().as_str() {
        "correction" => format!("{} revised", texts(&corpus()).0),
        "revert" => texts(&corpus()).0.to_owned(),
        "erase" => {
            if corpus() == "unicode" {
                "Restored \u{65e5}\u{672c}\u{1f600} owner".into()
            } else {
                "Restored owner".into()
            }
        }
        _ => panic!("Unsupported parent cadence"),
    }
}
fn gap_text() -> &'static str {
    if corpus() == "unicode" {
        "Independent \u{65e5}\u{672c}\u{1f600} gap"
    } else {
        "Independent gap"
    }
}
fn rich_text() -> String {
    let text = escaped(&owner_text());
    if format() == "bold" {
        format!("<b>{text}</b>")
    } else {
        format!("<a href=\"https://example.test/\">{text}</a>")
    }
}
pub(super) fn seed(corpus: &str, mode: &str) -> Arc<CanvasCollabSnapshot> {
    let source = seed::seed_parent(corpus, mode, &cadence());
    if !has_gap() {
        return source;
    }
    add_gap(source, parent(mode), gap_text())
}
fn add_gap(
    source: Arc<CanvasCollabSnapshot>,
    owner: &str,
    text: &str,
) -> Arc<CanvasCollabSnapshot> {
    use super_platinum_core::canvas::edit::structure::{Side, StructureIntent};
    let intent = StructureIntent::insert(
        &source,
        owner,
        Side::After,
        &TextDraft::from_rtml(&escaped(text)).unwrap(),
        100,
    )
    .unwrap();
    let p = intent.prepare(&source, 100).unwrap();
    let (a, fresh, _) = completion::delete(p, source.clone());
    assert!(a.confirms_readback(&fresh).unwrap());
    assert!(
        source
            .sections
            .iter()
            .all(|s| fresh.section(s.id()) == Some(s))
    );
    fresh
}
fn gap_id(source: &CanvasCollabSnapshot, owner: &str, child: &str) -> Option<String> {
    source
        .sections
        .iter()
        .find(|s| !matches!(s.id(), "title" | "neighbor") && s.id() != owner && s.id() != child)
        .map(|s| s.id().to_owned())
}
fn script(call: &str) -> String {
    format!("{DOM}\nwindow.superPlatinumLiveRichCheck.{call}")
}
pub(super) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !requested()
        || env("SUPER_PLATINUM_CANVAS_COALESCED_OWNER_RICH_CHECK", "0") != "1"
        || STARTED.swap(true, Ordering::Relaxed)
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = match interaction::run(state, &desktop).await {
            Ok(v) => v,
            Err(error) => json!({"ok":false,"error":error}),
        };
        eprintln!("canvas-coalesced-owner-rich-check: {result}");
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
async fn row(
    desktop: &dioxus::desktop::DesktopContext,
    id: &str,
    focus: bool,
    delete: bool,
) -> Result<Value, String> {
    wait(desktop, &script(&format!("show({})", json!(id)))).await?;
    check::row_actions(desktop, id, focus, delete).await
}
async fn initial_row(desktop: &dioxus::desktop::DesktopContext, id: &str) -> Result<Value, String> {
    let mut geometry = row(desktop, id, false, true).await?;
    let text = if cadence() == "erase" {
        String::new()
    } else {
        owner_text()
    };
    let content=wait(desktop,&format!(r#"(()=>{{const e=[...document.querySelectorAll('[data-canvas-text-section]')].find(e=>e.getAttribute('data-canvas-text-section')==={}),c=e?.querySelector('.canvas-paragraph-content');return {{ok:!!c&&c.textContent==={},emptyContent:c?.textContent==='',emptyLabel:null}};}})()"#,json!(id),json!(text))).await?;
    geometry["emptyContent"] = content["emptyContent"].clone();
    geometry["emptyLabel"] = Value::Null;
    Ok(geometry)
}
async fn open(desktop: &dioxus::desktop::DesktopContext, id: &str) -> Result<(), String> {
    row(desktop, id, false, true).await?;
    click(
        desktop,
        &format!(
            "[data-canvas-text-section={}] .canvas-text-open[aria-label='Edit paragraph']",
            json!(id)
        ),
    )
    .await?;
    check::editor_ready(desktop, false).await?;
    editor_layout(desktop).await?;
    Ok(())
}
async fn apply_format(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    remove: bool,
    ordinal: &mut usize,
) -> Result<(), String> {
    let text = owner_text();
    if !remove && cadence() == "erase" {
        set_draft(desktop, &format!("<div>{}</div>", escaped(&text))).await?;
    }
    checked(desktop, &script(&format!("select({})", json!(text)))).await?;
    checked(desktop, &script(&format!("toolbar({})", json!(format())))).await?;
    if format() == "link" {
        let geometry = wait(desktop, &script(&format!("linkReady({remove})"))).await?;
        capture(
            state,
            if remove {
                "remove-link-form"
            } else {
                "link-form"
            },
            ordinal,
            geometry,
        )
        .await?;
        checked(desktop, &script(&format!("applyLink({remove})"))).await?;
    }
    wait(
        desktop,
        &script(&format!(
            "formatted({},{remove},{})",
            json!(format()),
            json!(text)
        )),
    )
    .await?;
    Ok(())
}
async fn save(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    text: bool,
) -> Result<(), String> {
    let count = {
        let mut shell = state.write();
        fixture(&mut shell).writes.len()
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
            if fixture(&mut state.write()).writes.len() != count + 1 {
                return Err("Duplicate save dispatched".into());
            }
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err("Preparation did not dispatch".into())
}
async fn finish(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<(), String> {
    completion::complete_text(state).await?;
    wait(
        desktop,
        r#"({ok:!document.getElementById('canvas-text-editor')})"#,
    )
    .await?;
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
        return Err("Rich owner operation changed another raw row".into());
    }
    Ok(())
}
fn canonical(source: &CanvasCollabSnapshot, id: &str, text: &str) -> Result<(), String> {
    let value = source
        .section(id)
        .and_then(|s| s.raw().message(12).ok().flatten())
        .and_then(|v| v.message(1).ok().flatten())
        .and_then(|v| v.string(1).ok().flatten().map(str::to_owned));
    if value.as_deref() != Some(text) {
        return Err("Canonical owner content differs".into());
    }
    Ok(())
}
async fn terminal(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    ordinal: usize,
) -> Result<Value, String> {
    let source = snapshot(&state.peek());
    let canonical = tokio::task::spawn_blocking(move || seed::saved(&source))
        .await
        .map_err(|_| "Evidence worker failed")?;
    Ok(
        json!({"ok":true,"noSession":true,"noTransport":true,"noHttp":true,"ownerRich":config(),"coalesced":true,"stages":ordinal,"cancelNoWrite":true,"duplicateGuard":true,"layout":check::layout(desktop).await?,"canonical":canonical,"journal":recovery::observe().await?}),
    )
}
