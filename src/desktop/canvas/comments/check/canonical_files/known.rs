//! Real known-row event refresh with original rich reply and private SQLite custody.
use super::*;
use data::known as source;
mod evidence;
use evidence::{
    event, observe, phase, quiet, ready, receiver_closed, requests, script, stable, stage,
};
fn configure(state: Signal<ShellState>, mode: source::Mode, version: u64) -> Result<(), String> {
    source::configure(&state.peek(), mode, version)
        .map_err(|_| "Closed known source configuration refused".into())
}
fn release(state: Signal<ShellState>, serial: usize, delivered: bool) -> Result<Value, String> {
    let result =
        source::release(&state.peek(), serial).map_err(|_| "Original known sender missing")?;
    if result["delivered"] != delivered || result["cancelledBeforeReply"] == delivered {
        return Err(format!("Unexpected original sender result: {result}"));
    }
    Ok(result)
}
async fn retry(desktop: &dioxus::desktop::DesktopContext) -> Result<Value, String> {
    eval(desktop,&format!(r#"(()=>{{const row=document.querySelector('[data-comment-ts="{}"]'),e=row?.querySelector('[data-canvas-file-retry="{}"]');if(!e||e.disabled)return {{ok:false}};const before={{root:row.dataset.commentTs,text:e.textContent,disabled:e.disabled}};e.click();return {{ok:true,before,sameNode:e===row.querySelector('[data-canvas-file-retry="{}"]'),connected:e.isConnected}};}})()"#,data::ROOT,data::ROOT,data::ROOT)).await
}
pub(super) async fn run(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let case = source::case().ok_or("Known fixture case missing")?;
    settled(state, desktop, 1, 5).await?;
    click(
        desktop,
        &format!(
            ".canvas-comment[data-comment-ts='{}'] .canvas-comment-open",
            data::ROOT
        ),
    )
    .await?;
    settled(state, desktop, 2, 2).await?;
    custody::saved(state, desktop).await?;
    wait(
        desktop,
        "!!document.querySelector('[data-canvas-file-id=\"FCANON001\"]')",
    )
    .await?;
    requests(0, 0).await?;
    eval(desktop, &script("remember-source", "known")).await?;
    let mut stages = Vec::new();
    let q = quiet(state, desktop, "known").await?;
    stage(
        state,
        desktop,
        "known",
        None,
        json!({"quiet":q}),
        &mut stages,
    )
    .await?;
    custody::action(desktop, "append").await?;
    custody::saved(state, desktop).await?;
    custody::action(desktop, "remember").await?;
    let draft = stage(state, desktop, "draft", None, json!({}), &mut stages).await?;
    let baseline = stable(&draft);
    configure(state, source::Mode::Held, 1)?;
    let first_event = event(state);
    phase(desktop, "loading").await?;
    requests(1, 0).await?;
    let q = quiet(state, desktop, "loading").await?;
    stage(
        state,
        desktop,
        "loading",
        Some(&baseline),
        json!({"event":first_event,"quiet":q}),
        &mut stages,
    )
    .await?;
    if case == "success" {
        let released = release(state, 1, true)?;
        requests(1, 1).await?;
        ready(desktop, 1).await?;
        let q = quiet(state, desktop, "renamed").await?;
        stage(
            state,
            desktop,
            "renamed",
            Some(&baseline),
            json!({"release":released,"quiet":q}),
            &mut stages,
        )
        .await?;
        configure(state, source::Mode::Offline, 2)?;
        let second_event = event(state);
        phase(desktop, "unavailable").await?;
        requests(2, 2).await?;
        let q = quiet(state, desktop, "offline").await?;
        stage(
            state,
            desktop,
            "offline",
            Some(&baseline),
            json!({"event":second_event,"quiet":q}),
            &mut stages,
        )
        .await?;
        for (name, mode, count) in [
            ("malformed", source::Mode::Malformed, 3),
            ("rejected", source::Mode::Error, 4),
        ] {
            configure(state, mode, 2)?;
            let clicked = retry(desktop).await?;
            phase(desktop, "unavailable").await?;
            requests(count, count).await?;
            let q = quiet(state, desktop, name).await?;
            stage(
                state,
                desktop,
                name,
                Some(&baseline),
                json!({"retry":clicked,"quiet":q}),
                &mut stages,
            )
            .await?;
        }
        configure(state, source::Mode::Success, 2)?;
        let clicked = retry(desktop).await?;
        requests(5, 5).await?;
        ready(desktop, 2).await?;
        let q = quiet(state, desktop, "recovered").await?;
        stage(
            state,
            desktop,
            "recovered",
            Some(&baseline),
            json!({"retry":clicked,"quiet":q}),
            &mut stages,
        )
        .await?;
    } else {
        configure(state, source::Mode::Held, 2)?;
        let second_event = event(state);
        receiver_closed(1).await?;
        requests(2, 1).await?;
        phase(desktop, "loading").await?;
        let q = quiet(state, desktop, "superseded").await?;
        stage(
            state,
            desktop,
            "superseded",
            Some(&baseline),
            json!({"event":second_event,"quiet":q}),
            &mut stages,
        )
        .await?;
        let before = observe(state, desktop, "late-refused").await?;
        let released = release(state, 1, false)?;
        let q = quiet(state, desktop, "late-refused").await?;
        let after = observe(state, desktop, "late-refused").await?;
        if stable(&before) != stable(&after)
            || before["proof"]["retained"] != after["proof"]["retained"]
        {
            return Err("Late original known response changed current source or draft".into());
        }
        stage(
            state,
            desktop,
            "late-refused",
            Some(&baseline),
            json!({"before":before,"release":released,"after":after,"quiet":q}),
            &mut stages,
        )
        .await?;
        let released = release(state, 2, true)?;
        requests(2, 2).await?;
        ready(desktop, 2).await?;
        let q = quiet(state, desktop, "recovered").await?;
        stage(
            state,
            desktop,
            "recovered",
            Some(&baseline),
            json!({"release":released,"quiet":q}),
            &mut stages,
        )
        .await?;
    }
    let final_stage = stages.last().ok_or("Known stages missing")?;
    Ok(
        json!({"ok":true,"case":case,"noSession":true,"noTransport":true,"draftPreserved":true,"originalReadRetained":true,"noNavigation":true,"noAutomaticLoop":true,"requestCount":2,"acquisitionCount":if case=="success"{5}else{2},"resolverCount":0,"layout":final_stage["layout"],"proof":final_stage["proof"],"reply":final_stage["reply"],"acquisition":source::snapshot(),"stages":stages}),
    )
}
