use super::*;
pub(super) fn action(command: &str) -> String {
    include_str!("body/actions.js").replace("__COMMAND__", &json!(command).to_string())
}
async fn body_ready(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    failed: bool,
) -> Result<(), String> {
    let mut last = Value::Null;
    for _ in 0..160 {
        let source = data::return_body::snapshot(&state.peek());
        let native = eval(desktop, &action("state")).await?;
        let view = &source["presentation"];
        let settled = if failed {
            view["stale"] == true
                && view["snapshot"].is_null()
                && native["error"]
                    == "Canvas state could not refresh. Try again or open it in Slack."
                && native["controls"]["retry"]["disabled"] == false
        } else {
            view["stale"] == false
                && view["snapshot"]["file"] == "Ffixture"
                && native["title"] == data::return_body::TITLE
                && native["body"]
                    .as_str()
                    .is_some_and(|s| s.contains(data::return_body::CONTENT))
                && native["error"].is_null()
        };
        if source["current"] == true && view["loading"] == false && settled {
            return Ok(());
        }
        last = json!({"source":source,"native":native});
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err(format!("Returned body failed to settle: {last}"))
}
pub(super) async fn run(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    retained: &Retained,
    replacement: &Replacement,
    original: &reply::fixture::DestinationWitness,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    let baseline = stages[6]["retainedEntry"]["journal"].clone();
    let blocks = stages[6]["retainedEntry"]["runtime"]["blocks"].clone();
    let remembered = eval(desktop, &action("remember")).await?;
    let finish_owner = remembered["finishOwner"].clone();
    eval(desktop, &action("click-back")).await?;
    wait(desktop,"!document.getElementById('canvas-comments')&&!document.getElementById('canvas-comment-reply')").await?;
    let mut finished = false;
    for _ in 0..160 {
        let runtime = retained.runtime(state)?;
        let native = eval(desktop, &action("state")).await?;
        let capture = native["captures"]
            .as_array()
            .and_then(|rows| rows.iter().find(|r| r["mount"] == finish_owner["mount"]));
        if runtime["nativePresent"] == false
            && runtime["saving"] == false
            && runtime["error"].is_null()
            && capture.is_some_and(|c| {
                c["closed"] == true
                    && c["commands"].as_array().map(Vec::len) == Some(1)
                    && c["captures"].as_array().map(Vec::len) == Some(1)
                    && c["acks"].as_array().map(Vec::len) == Some(1)
                    && c["captures"][0]["blocks"] == blocks
                    && c["captures"][0]["finalCapture"] == true
                    && c["captures"][0]["request"] == c["commands"][0]["request"]
                    && c["acks"][0]["sequence"] == c["captures"][0]["sequence"]
            })
            && native["controls"]["refresh"]["disabled"] == false
        {
            finished = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    if !finished {
        return Err("Actual returned mount finish/ACK did not settle".into());
    }
    data::return_body::arm(&state.peek())?;
    eval(desktop, &action("scroll")).await?;
    stage(
        state,
        desktop,
        retained,
        replacement,
        original,
        "back-to-canvas",
        stages,
    )
    .await?;
    eval(desktop, &action("click-refresh")).await?;
    if data::return_body::failure() {
        body_ready(state, desktop, true).await?;
        eval(desktop, &action("scroll")).await?;
        stage(
            state,
            desktop,
            retained,
            replacement,
            original,
            "body-failed",
            stages,
        )
        .await?;
        eval(desktop, &action("click-retry")).await?;
    }
    body_ready(state, desktop, false).await?;
    eval(desktop, &action("scroll")).await?;
    stage(
        state,
        desktop,
        retained,
        replacement,
        original,
        "body-ready",
        stages,
    )
    .await?;
    if data::auto_refresh::requested() {
        super::auto_refresh::run(state, desktop, retained, replacement, original, stages).await?;
    }
    eval(desktop, &action("click-threads")).await?;
    settled(state, desktop, 5, 5).await?;
    click(
        desktop,
        &format!(
            ".canvas-comment[data-comment-ts='{}'] .canvas-comment-open",
            data::ROOT
        ),
    )
    .await?;
    settled(state, desktop, 6, 2).await?;
    custody::saved(state, desktop).await?;
    let native = eval(desktop, &action("state")).await?;
    if native["currentEditor"]["blocks"] != blocks
        || native["currentEditor"]["sameFinishedNode"] != false
        || native["currentEditor"]["mount"] == finish_owner["mount"]
    {
        return Err(format!(
            "Returned reply did not use fresh original-content mount: {native}"
        ));
    }
    stage(
        state,
        desktop,
        retained,
        replacement,
        original,
        "reply-restored",
        stages,
    )
    .await?;
    for s in &stages[7..] {
        if s["retainedEntry"]["journal"] != baseline
            || s["retainedEntry"]["runtime"]["blocks"] != blocks
            || s["retainedEntry"]["runtime"]["originalOwnerCurrent"] != false
            || s["originalDestinationCurrent"] != false
        {
            return Err(format!(
                "Returned body changed original custody/authority: {s}"
            ));
        }
    }
    Ok(())
}
