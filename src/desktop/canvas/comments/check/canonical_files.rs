//! Offline canonical display and guarded, durable generic-file departure.
use super::*;
use fixture::canonical_files as data;
mod capture;
mod cold;
mod custody;
mod destinations;
mod known;
mod replacement;

pub(super) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !data::requested() || std::env::var(data::FLAG).as_deref() != Ok("1") {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let custody = data::custody();
        let result = if reply::fixture::replacement::requested() {
            replacement::run(state, &desktop).await
        } else if data::known::requested() {
            known::run(state, &desktop).await
        } else if data::cold::requested() {
            cold::run(state, &desktop).await
        } else if destinations::late_capture_requested() {
            destinations::late_capture::run(state, &desktop).await
        } else if destinations::storage_requested() {
            destinations::storage::run(state, &desktop).await
        } else if destinations::retirement_requested() {
            destinations::retirement::run(state, &desktop).await
        } else if data::destinations_requested() {
            destinations::run(state, &desktop).await
        } else if custody {
            custody::run(state, &desktop).await
        } else {
            run(state, &desktop).await
        };
        let result = result.unwrap_or_else(
            |error| json!({"ok":false,"error":error,"proof":data::proof(&state.peek())}),
        );
        let prefix = if data::auto_refresh::requested() {
            "canvas-auto-refresh"
        } else if data::return_body::requested() {
            "canvas-return-body"
        } else if reply::fixture::replacement::requested() {
            "canvas-replacement-custody"
        } else if data::known::requested() {
            "canvas-known-file-freshness"
        } else if data::cold::requested() {
            "canvas-cold-file-roots"
        } else if destinations::late_capture_requested() {
            "canvas-canonical-native-late-capture"
        } else if destinations::storage_requested() {
            "canvas-canonical-native-storage"
        } else if destinations::retirement_requested() {
            "canvas-canonical-native-retirement"
        } else if data::destinations_requested() {
            "canvas-canonical-native-destinations"
        } else if custody {
            "canvas-canonical-file-custody"
        } else {
            "canvas-canonical-file-roots"
        };
        let _ = tokio::task::spawn_blocking(move || eprintln!("{prefix}-check: {result}")).await;
    });
}
async fn settled(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    reads: usize,
    rows: usize,
) -> Result<(), String> {
    wait(desktop,&format!("document.querySelectorAll('.canvas-comment').length==={rows}&&!document.querySelector('#canvas-comments [aria-busy=true]')&&!document.querySelector('.canvas-comments-error')")).await?;
    for _ in 0..100 {
        let proof = data::proof(&state.peek());
        if proof["requests"].as_array().map(Vec::len) == Some(reads)
            && proof["retained"]["loading"] == false
            && proof["retained"]["rows"].as_array().map(Vec::len) == Some(rows)
        {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err(format!("Original read {reads} did not settle"))
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    settled(state, desktop, 1, 5).await?;
    click(desktop, "#canvas-comments-back").await?;
    wait(
        desktop,
        "document.activeElement?.id==='canvas-comments-open'",
    )
    .await?;
    click(desktop, "#canvas-comments-open").await?;
    settled(state, desktop, 2, 5).await?;
    let mut stages = Vec::new();
    for (name, ts) in ["hosted", "canvas", "list", "hidden", "blocked"]
        .into_iter()
        .zip(data::ROOTS)
    {
        capture::display(state, desktop, name, Some(ts), 2, &mut stages).await?;
    }
    click(
        desktop,
        &format!(
            ".canvas-comment[data-comment-ts='{}'] .canvas-comment-open",
            data::ROOT
        ),
    )
    .await?;
    settled(state, desktop, 3, 2).await?;
    capture::display(state, desktop, "thread", Some(data::ROOT), 3, &mut stages).await?;
    for (name, mode, reads) in [
        ("renamed", data::Mode::Renamed, 4),
        ("no-permalink", data::Mode::NoPermalink, 5),
        ("deleted", data::Mode::Deleted, 6),
    ] {
        data::configure(&state.peek(), mode).map_err(|_| "Cannot configure original source")?;
        click(desktop, "#canvas-comments-refresh").await?;
        settled(state, desktop, reads, 2).await?;
        capture::display(state, desktop, name, Some(data::ROOT), reads, &mut stages).await?;
    }
    fixture::fail_next();
    click(desktop, "#canvas-comments-refresh").await?;
    wait(desktop,"!!document.querySelector('.canvas-comments-error')&&!document.querySelector('#canvas-comments [aria-busy=true]')").await?;
    capture::display(state, desktop, "error", None, 7, &mut stages).await?;
    data::configure(&state.peek(), data::Mode::Original)
        .map_err(|_| "Cannot configure recovery")?;
    click(desktop, "#canvas-comments-refresh").await?;
    settled(state, desktop, 8, 2).await?;
    capture::display(
        state,
        desktop,
        "recovered",
        Some(data::ROOT),
        8,
        &mut stages,
    )
    .await?;
    data::remember_token(&state.peek())?;
    crate::file_events::invalidate(&mut state.write(), "T1", data::HOSTED);
    wait(
        desktop,
        "!document.querySelector('[data-canvas-file-id=\"FCANON001\"]')",
    )
    .await?;
    let stale_callback = data::invoke_original(state)?;
    capture::display(
        state,
        desktop,
        "event-invalidated",
        Some(data::ROOT),
        8,
        &mut stages,
    )
    .await?;
    click(desktop, "#canvas-comments-refresh").await?;
    settled(state, desktop, 9, 2).await?;
    capture::display(
        state,
        desktop,
        "event-restored",
        Some(data::ROOT),
        9,
        &mut stages,
    )
    .await?;
    click(desktop, "#canvas-comments-back").await?;
    wait(desktop,"!document.getElementById('canvas-comments')&&document.activeElement?.id==='canvas-comments-open'").await?;
    Ok(
        json!({"ok":true,"noSession":true,"noTransport":true,"canonicalDisplay":true,"firstOnly":true,
        "rawPreserved":true,"noDiscardedMedia":true,"readOnly":true,"requestCount":9,"staleCallback":stale_callback,
        "layout":stages.last().unwrap()["layout"],"proof":data::proof(&state.peek()),"stages":stages}),
    )
}
