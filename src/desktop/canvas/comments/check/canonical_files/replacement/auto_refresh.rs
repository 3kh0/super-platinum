//! Drives original revision ingress and release only; the CanvasDialog effect owns every read.
use super::*;
use data::auto_refresh as source;
async fn pending(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    serial: usize,
) -> Result<(), String> {
    let mut last = Value::Null;
    for _ in 0..160 {
        let a = source::snapshot(&state.peek());
        let native = eval(desktop, &body::action("state")).await?;
        let view = data::return_body::snapshot(&state.peek());
        if a["held"] == serial
            && a["pairs"][serial - 1]["status"] == "pending"
            && view["presentation"]["loading"] == true
            && native["title"] == data::return_body::TITLE
            && native["body"]
                .as_str()
                .is_some_and(|s| s.contains(data::return_body::CONTENT))
            && native["error"].is_null()
        {
            return Ok(());
        }
        last = json!({"autoRefresh":a,"returnBody":view,"native":native});
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err(format!(
        "Original effect did not reach held pair {serial}: {last}"
    ))
}
async fn settled(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    failed: bool,
) -> Result<(), String> {
    let mut last = Value::Null;
    for _ in 0..160 {
        let a = source::snapshot(&state.peek());
        let native = eval(desktop, &body::action("state")).await?;
        let view = data::return_body::snapshot(&state.peek());
        let p = &view["presentation"];
        let done = if failed {
            p["stale"] == true
                && native["error"]
                    == "Canvas state could not refresh. Try again or open it in Slack."
                && native["controls"]["retry"]["disabled"] == false
                && native["title"] == data::return_body::TITLE
                && native["status"]
                    .as_array()
                    .is_some_and(|s| !s.iter().any(|v| v == &native["error"]))
        } else {
            p["stale"] == false
                && native["title"] == source::TITLE
                && native["body"]
                    .as_str()
                    .is_some_and(|s| s.contains(source::CONTENT))
                && native["error"].is_null()
        };
        if p["loading"] == false
            && done
            && a["pairs"]
                .as_array()
                .and_then(|v| v.last())
                .is_some_and(|v| v["status"] == if failed { "failed" } else { "ready" })
        {
            return Ok(());
        }
        last = json!({"autoRefresh":a,"returnBody":view,"native":native});
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err(format!("Automatic body did not settle: {last}"))
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    retained: &Retained,
    replacement: &Replacement,
    original: &reply::fixture::DestinationWitness,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    {
        let mut s = state.write();
        source::arm(&s)?;
        source::burst(&mut s, 10)?;
    }
    pending(state, desktop, 1).await?;
    stage(
        state,
        desktop,
        retained,
        replacement,
        original,
        "auto-held",
        stages,
    )
    .await?;
    source::burst(&mut state.write(), 10)?;
    stage(
        state,
        desktop,
        retained,
        replacement,
        original,
        "newer-event",
        stages,
    )
    .await?;
    source::release(&state.peek(), 1)?;
    pending(state, desktop, 2).await?;
    let a = source::snapshot(&state.peek());
    if a["pairs"][0]["status"] != "stale-success"
        || a["pairs"][0]["readCurrentAtFinish"] != false
        || a["pairs"][0]["ownerCurrentAtFinish"] != true
    {
        return Err(format!(
            "Original stale response was not retained accurately: {a}"
        ));
    }
    stage(
        state,
        desktop,
        retained,
        replacement,
        original,
        "latest-pending",
        stages,
    )
    .await?;
    source::release(&state.peek(), 2)?;
    if source::failure() {
        settled(state, desktop, true).await?;
        eval(desktop, &body::action("scroll")).await?;
        stage(
            state,
            desktop,
            retained,
            replacement,
            original,
            "auto-failed",
            stages,
        )
        .await?;
        source::burst(&mut state.write(), 10)?;
        let before = source::snapshot(&state.peek());
        let started = std::time::Instant::now();
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        source::quiet(&state.peek(), before, started.elapsed().as_millis())?;
        stage(
            state,
            desktop,
            retained,
            replacement,
            original,
            "failure-latched",
            stages,
        )
        .await?;
        eval(desktop, &body::action("click-retry")).await?;
        settled(state, desktop, false).await?;
    } else {
        settled(state, desktop, false).await?;
    }
    eval(desktop, &body::action("scroll")).await?;
    stage(
        state,
        desktop,
        retained,
        replacement,
        original,
        "auto-ready",
        stages,
    )
    .await?;
    Ok(())
}
