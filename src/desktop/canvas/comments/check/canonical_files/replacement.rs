//! Actual rich DOM -> loaded B shell -> late original capture -> loaded A shell.
use super::*;
use reply::fixture::replacement::{self as seam, Replacement, Retained};
const PREFIX: &str = "canvas-replacement-custody";
mod auto_refresh;
mod body;
fn action(command: &str) -> String {
    include_str!("replacement/actions.js").replace("__COMMAND__", &json!(command).to_string())
}
async fn observe(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    retained: &Retained,
    replacement: &Replacement,
    original: &reply::fixture::DestinationWitness,
) -> Result<Value, String> {
    let mut value = json!({"shell":seam::shell(state),"retainedEntry":retained.snapshot(state).await?,
        "successor":replacement.b_snapshot(state).await?,"layout":eval(desktop,&action("state")).await?,
        "originalDestinationCurrent":original.runtime(state).is_ok(),
        "reads":{"requests":fixture::ledger(),"responses":fixture::responses()}});
    if data::return_body::requested() {
        value["returnBody"] = json!({"source":data::return_body::snapshot(&state.peek()),
            "native":eval(desktop,&body::action("state")).await?});
    }
    if data::auto_refresh::requested() {
        value["autoRefresh"] = data::auto_refresh::snapshot(&state.peek());
    }
    Ok(value)
}
async fn stage(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    retained: &Retained,
    replacement: &Replacement,
    original: &reply::fixture::DestinationWitness,
    name: &str,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    if matches!(
        name,
        "thread" | "edited-held" | "remounted-a" | "reply-restored"
    ) {
        eval(desktop, &custody::script("scroll", name)).await?;
        eval(desktop, &custody::script("state", name)).await?;
    }
    let mut receipt = observe(state, desktop, retained, replacement, original).await?;
    if let Some(first) = stages.first() {
        if receipt["successor"] != first["successor"] {
            return Err(format!("Independent B changed: {receipt}"));
        }
    }
    if receipt["shell"]["noSession"] != true
        || receipt["shell"]["noTransport"] != true
        || receipt["shell"]["replyLedger"] != json!({"requests":[],"prepared":[]})
    {
        return Err(format!("Foreign send or session appeared: {receipt}"));
    }
    let before = receipt.clone();
    receipt["ok"] = json!(true);
    receipt["stage"] = json!(name);
    receipt["ordinal"] = json!(stages.len());
    receipt["noSession"] = json!(true);
    receipt["noTransport"] = json!(true);
    capture::emit(
        if data::auto_refresh::requested() {
            "canvas-auto-refresh"
        } else if data::return_body::requested() {
            "canvas-return-body"
        } else {
            PREFIX
        },
        name,
        receipt.clone(),
        stages.len(),
    )
    .await?;
    let after = observe(state, desktop, retained, replacement, original).await?;
    if before != after {
        return Err(format!(
            "Replacement capture changed: {}",
            json!({"before":before,"after":after})
        ));
    }
    stages.push(receipt);
    Ok(())
}
async fn held(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    retained: &Retained,
    phase: &str,
) -> Result<Value, String> {
    let mut last = Value::Null;
    for _ in 0..160 {
        let layout = eval(desktop, &action("state")).await?;
        let runtime = retained.runtime(state)?;
        if layout["bridge"]["phase"] == phase
            && runtime["saving"] == false
            && runtime["loading"] == false
            && runtime["error"].is_null()
        {
            return Ok(layout);
        }
        last = json!({"layout":layout,"runtime":runtime});
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err(format!(
        "Genuine original capture not held at {phase}: {last}"
    ))
}
pub(super) async fn run(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
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
    let original = reply::fixture::DestinationWitness::capture(state)?;
    let retained = Retained::capture(state)?;
    let mut replacement = Replacement::prepare(state).await?;
    let mut stages = Vec::new();
    stage(
        state,
        desktop,
        &retained,
        &replacement,
        &original,
        "thread",
        &mut stages,
    )
    .await?;
    eval(desktop, &action("arm")).await?;
    eval(desktop, &custody::script("append", "edited-held")).await?;
    let edited = held(state, desktop, &retained, "edited-held").await?;
    let envelope = edited["bridge"]["held"][0].clone();
    if envelope["blocks"] == stages[0]["retainedEntry"]["runtime"]["blocks"]
        || retained.runtime(state)?["blocks"] != stages[0]["retainedEntry"]["runtime"]["blocks"]
        || envelope["detached"] != false
        || envelope["finalCapture"] != false
    {
        return Err(format!(
            "Native edit was not held ahead of original model: {edited}"
        ));
    }
    stage(
        state,
        desktop,
        &retained,
        &replacement,
        &original,
        "edited-held",
        &mut stages,
    )
    .await?;
    replacement.install_b(state)?;
    wait(desktop,"!document.querySelector('.canvas-dialog')&&!document.getElementById('canvas-comment-reply')").await?;
    if original.runtime(state).is_ok() || retained.runtime(state)?["originalOwnerCurrent"] != false
    {
        return Err("Original authority survived B replacement".into());
    }
    stage(
        state,
        desktop,
        &retained,
        &replacement,
        &original,
        "replacement-b",
        &mut stages,
    )
    .await?;
    eval(desktop, &action("release")).await?;
    let detached = held(state, desktop, &retained, "detached-held").await?;
    if detached["bridge"]["delivered"][0] != envelope
        || detached["bridge"]["held"][1]["blocks"] != envelope["blocks"]
        || detached["bridge"]["held"][1]["detached"] != true
        || detached["original"]["connected"] != false
    {
        return Err(format!("Original detached envelope changed: {detached}"));
    }
    stage(
        state,
        desktop,
        &retained,
        &replacement,
        &original,
        "detached-held",
        &mut stages,
    )
    .await?;
    eval(desktop, &action("release")).await?;
    let mut ready = false;
    let mut last = Value::Null;
    for _ in 0..160 {
        let observed = observe(state, desktop, &retained, &replacement, &original).await?;
        let a = &observed["retainedEntry"];
        if observed["layout"]["bridge"]["closed"] == true
            && observed["layout"]["bridge"]["phase"] == "detached-delivered"
            && a["runtime"]["nativePresent"] == false
            && a["runtime"]["saving"] == false
            && a["runtime"]["error"].is_null()
            && a["runtime"]["blocks"] == envelope["blocks"]
            && a["journal"]["drafts"][0]["blocks"] == envelope["blocks"]
        {
            ready = true;
            break;
        }
        last = observed;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    if !ready {
        return Err(format!("Detached original save did not settle: {last}"));
    }
    stage(
        state,
        desktop,
        &retained,
        &replacement,
        &original,
        "late-saved",
        &mut stages,
    )
    .await?;
    replacement.return_a(state)?;
    wait(desktop,"!document.querySelector('.canvas-dialog')&&!document.getElementById('canvas-comment-reply')").await?;
    stage(
        state,
        desktop,
        &retained,
        &replacement,
        &original,
        "returned-a",
        &mut stages,
    )
    .await?;
    replacement.reopen(state)?;
    wait(desktop,"document.querySelector('.canvas-dialog')?.getAttribute('aria-label')==='Canvas: Project handbook'").await?;
    if eval_raw(
        desktop,
        "({open:!!document.getElementById('canvas-comments')})",
    )
    .await?["open"]
        != true
    {
        click(desktop, "#canvas-comments-open").await?;
    }
    settled(state, desktop, 3, 5).await?;
    click(
        desktop,
        &format!(
            ".canvas-comment[data-comment-ts='{}'] .canvas-comment-open",
            data::ROOT
        ),
    )
    .await?;
    settled(state, desktop, 4, 2).await?;
    custody::saved(state, desktop).await?;
    let reopened = eval(desktop, &action("state")).await?;
    if reopened["editor"]["blocks"] != envelope["blocks"]
        || reopened["editor"]["sameOriginalNode"] != false
        || reopened["editor"]["sameOriginalMount"] != false
        || original.runtime(state).is_ok()
        || retained.runtime(state)?["originalOwnerCurrent"] != false
    {
        return Err(format!(
            "Fresh A remount restored stale authority or changed content: {reopened}"
        ));
    }
    stage(
        state,
        desktop,
        &retained,
        &replacement,
        &original,
        "remounted-a",
        &mut stages,
    )
    .await?;
    if data::return_body::requested() {
        body::run(
            state,
            desktop,
            &retained,
            &replacement,
            &original,
            &mut stages,
        )
        .await?;
    }
    Ok(
        json!({"ok":true,"noSession":true,"noTransport":true,"replacement":true,
        "originalEntryRetained":true,"noForeignSend":true,"requestCount":if data::return_body::requested(){6}else{4},
        "layout":stages.last().unwrap()["layout"],"stages":stages}),
    )
}
