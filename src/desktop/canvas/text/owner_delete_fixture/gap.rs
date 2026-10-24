//! Saved split gaps exercise the shared structural worker and retained raw rows.
use super::*;
use crate::canvas::text::check::{checked, click, editor_layout, set_draft, wait};
use std::sync::atomic::{AtomicBool, Ordering};
mod interaction;
mod review;
static STARTED: AtomicBool = AtomicBool::new(false);
pub(in crate::canvas::text) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-split-gap" | "canvas-split-gap-narrow")
        )
}
pub(in crate::canvas::text) fn enabled(shell: &ShellState) -> bool {
    requested() && super::enabled(shell)
}
fn side() -> String {
    env("SUPER_PLATINUM_CANVAS_SPLIT_GAP_SIDE", "after-owner")
}
fn count() -> usize {
    env("SUPER_PLATINUM_CANVAS_SPLIT_GAP_COUNT", "1")
        .parse()
        .unwrap_or(0)
}
fn rich() -> String {
    env("SUPER_PLATINUM_CANVAS_SPLIT_GAP_RICH", "none")
}
fn owner_delete() -> bool {
    env("SUPER_PLATINUM_CANVAS_SPLIT_GAP_OWNER_DELETE", "0") == "1"
}
fn config() -> Value {
    json!({"corpus":corpus(),"history":mode(),"side":side(),"gaps":count(),"rich":rich(),"ownerDelete":owner_delete(),"phase":phase()})
}
pub(super) fn add_metadata(value: &mut Value) {
    value["gap"] = config();
}
fn text(index: usize) -> String {
    if corpus() == "unicode" {
        format!("Inserted \u{65e5}\u{672c}\u{1f600} gap {index}")
    } else {
        format!("Inserted gap {index}")
    }
}
fn markup(text: &str, kind: &str) -> String {
    let text = escaped(text);
    match kind {
        "bold" => format!("<b>{text}</b>"),
        "link" => format!("<a href=\"https://example.test/\">{text}</a>"),
        _ => text,
    }
}
pub(super) fn seed(corpus: &str, mode: &str) -> Arc<CanvasCollabSnapshot> {
    let source = seed::seed_variant(corpus, mode, true);
    if rich() == "none" {
        return source;
    }
    assert!(matches!(rich().as_str(), "bold" | "link"));
    let child = seed::child(&source, parent(mode));
    let intent = TextIntent::capture(&source, &child, 100).unwrap();
    let mut session = super_platinum_core::canvas::edit::EditSession::new();
    let draft = TextDraft::from_rtml(&markup(texts(corpus).1, &rich())).unwrap();
    let p = intent.prepare(&source, &mut session, &draft, 100).unwrap();
    let (accepted, fresh, _) = completion::text(p, source.clone());
    assert!(accepted.confirms_readback(&fresh).unwrap());
    assert_eq!(source.section(parent(mode)), fresh.section(parent(mode)));
    fresh
}
pub(super) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !requested()
        || env("SUPER_PLATINUM_CANVAS_SPLIT_GAP_CHECK", "0") != "1"
        || STARTED.swap(true, Ordering::Relaxed)
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = match interaction::run(state, &desktop).await {
            Ok(v) => v,
            Err(error) => json!({"ok":false,"error":error}),
        };
        eprintln!("canvas-split-gap-check: {result}");
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
    can_delete: bool,
) -> Result<Value, String> {
    wait(desktop,&format!(r#"(()=>{{const e=[...document.querySelectorAll('[data-canvas-text-section]')].find(e=>e.getAttribute('data-canvas-text-section')==={});if(!e?.isConnected||!e.querySelector('button'))return {{ok:false}};e.scrollIntoView({{block:'nearest'}});return {{ok:true}};}})()"#,json!(id))).await?;
    check::row_actions(desktop, id, focus, can_delete).await
}
fn unchanged(
    before: &CanvasCollabSnapshot,
    after: &CanvasCollabSnapshot,
    target: &str,
) -> Result<(), String> {
    if before
        .sections
        .iter()
        .filter(|s| s.id() != target)
        .any(|s| after.section(s.id()) != Some(s))
        || after
            .sections
            .iter()
            .any(|s| s.id() != target && before.section(s.id()).is_none())
    {
        return Err("Independent change mutated another row".into());
    }
    Ok(())
}
fn canonical(source: &CanvasCollabSnapshot, id: &str, rtml: &str) -> Result<(), String> {
    if source
        .section(id)
        .and_then(|s| s.raw().message(12).ok().flatten())
        .and_then(|v| v.message(1).ok().flatten())
        .and_then(|v| v.string(1).ok().flatten().map(str::to_owned))
        .as_deref()
        != Some(rtml)
    {
        return Err("Canonical paragraph differs".into());
    }
    Ok(())
}
async fn terminal(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    stages: usize,
) -> Result<Value, String> {
    let source = snapshot(&state.peek());
    let rows = tokio::task::spawn_blocking(move || seed::saved(&source))
        .await
        .map_err(|_| "Evidence worker failed")?;
    let (writes, acks, reservations) = {
        let mut shell = state.write();
        let f = fixture(&mut shell);
        (f.writes.clone(), f.acknowledgements.clone(), f.reservations)
    };
    Ok(
        json!({"ok":true,"gap":config(),"coalesced":true,"phase":phase(),"noSession":true,"noTransport":true,"noHttp":true,"cancelNoWrite":true,"duplicateGuard":true,"stages":stages,"layout":check::layout(desktop).await?,"canonical":rows,"journal":recovery::observe().await?,"writes":writes,"acknowledgements":acks,"reservations":reservations}),
    )
}
