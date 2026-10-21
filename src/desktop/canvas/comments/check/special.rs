//! Named hidden-root reader exercises only ordinary UI and disconnected reads.
use super::*;
use fixture::special as data;
mod capture;

pub(super) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !data::requested()
        || std::env::var("SUPER_PLATINUM_CANVAS_SPECIAL_ROOTS_CHECK").as_deref() != Ok("1")
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = match run(state, &desktop).await {
            Ok(value) => value,
            Err(error) => json!({"ok":false,"error":error,"proof":data::proof(&state.peek())}),
        };
        let _ =
            tokio::task::spawn_blocking(move || eprintln!("canvas-special-roots-check: {result}"))
                .await;
    });
}
async fn settled(desktop: &dioxus::desktop::DesktopContext, rows: usize) -> Result<(), String> {
    wait(desktop,&format!("document.querySelectorAll('.canvas-comment').length==={rows}&&!document.querySelector('#canvas-comments [aria-busy=true]')&&!document.querySelector('#canvas-comments [role=alert]')")).await
}
async fn run(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    settled(desktop, 1).await?;
    if !data::enabled(&state.peek()) {
        return Err("Original disconnected special-root owner missing".into());
    }
    click(desktop, "#canvas-comments-back").await?;
    wait(
        desktop,
        "document.activeElement?.id==='canvas-comments-open'",
    )
    .await?;
    click(desktop, "#canvas-comments-open").await?;
    settled(desktop, 1).await?;
    let mut stages = Vec::new();
    capture::stage(
        state,
        desktop,
        "initial",
        Some(data::HIDDEN),
        2,
        &mut stages,
    )
    .await?;
    click(desktop, ".canvas-comment-open").await?;
    settled(desktop, 2).await?;
    capture::stage(state, desktop, "thread", Some(data::HIDDEN), 3, &mut stages).await?;
    click(desktop, "#canvas-comments-refresh").await?;
    settled(desktop, 2).await?;
    capture::stage(
        state,
        desktop,
        "thread-refreshed",
        Some(data::HIDDEN),
        4,
        &mut stages,
    )
    .await?;
    click(desktop, "#canvas-comments-all").await?;
    settled(desktop, 1).await?;
    click(desktop, "#canvas-comments-more").await?;
    settled(desktop, 3).await?;
    for (name, ts) in ["visible-true", "visible-absent", "visible-null"]
        .into_iter()
        .zip(data::CONTROLS)
    {
        capture::stage(state, desktop, name, Some(ts), 6, &mut stages).await?;
    }
    fixture::fail_next();
    click(desktop, "#canvas-comments-refresh").await?;
    wait(desktop,"!!document.querySelector('#canvas-comments [role=alert]')&&!document.querySelector('#canvas-comments [aria-busy=true]')").await?;
    capture::stage(state, desktop, "error", None, 7, &mut stages).await?;
    click(desktop, "#canvas-comments-refresh").await?;
    settled(desktop, 1).await?;
    capture::stage(
        state,
        desktop,
        "recovered",
        Some(data::HIDDEN),
        8,
        &mut stages,
    )
    .await?;
    click(desktop, "#canvas-comments-back").await?;
    wait(desktop,"!document.getElementById('canvas-comments')&&!!document.querySelector('.canvas-document')&&document.activeElement?.id==='canvas-comments-open'").await?;
    let proof = data::proof(&state.peek());
    Ok(
        json!({"ok":true,"noSession":true,"noTransport":true,"readOnly":true,"hiddenSuppressed":true,
        "noHiddenMedia":true,"originalIdentity":true,"explicitRetry":true,"returnedToCanvas":true,"syntheticInput":true,
        "requestCount":8,"proof":proof,"layout":stages.last().unwrap()["layout"],"stages":stages}),
    )
}
