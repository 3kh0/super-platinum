//! Actual single-month Date Range interaction against the closed Activity endpoint.
use super::*;
mod controls;
mod exercises;
pub(super) use controls::select;
use controls::{action, closed, day, inspect, open, settled};

fn count(s: Signal<ShellState>) -> usize {
    snapshot(&s.peek())["ledger"]["requests"]
        .as_array()
        .unwrap()
        .len()
}
fn custody(s: Signal<ShellState>) -> Value {
    let v = snapshot(&s.peek());
    json!({"filters":v["activity"]["filters"],"query":v["activity"]["query"],"requests":v["ledger"]["requests"].as_array().map(Vec::len),"requestLedger":v["ledger"]["requests"],"metadata":v["metadata"]})
}
fn unchanged(s: Signal<ShellState>, before: &Value) -> Result<Value, String> {
    let after = custody(s);
    if &after != before {
        return Err(format!(
            "Calendar temporary action changed committed custody: before={before},after={after}"
        ));
    }
    Ok(after)
}
async fn stage(
    s: Signal<ShellState>,
    d: &DesktopContext,
    name: &str,
    label: &str,
    exercise: Value,
    rows: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let model = snapshot(&s.peek());
    if label != "about"
        && label != "account-replaced"
        && (!exact(&model) || model["metadata"]["writes"] != 0)
    {
        return Err(format!("Calendar lost dirty metadata at {label}: {model}"));
    }
    let date_range = inspect(d).await?;
    let proof = json!({"dateRange":date_range,"exercise":exercise});
    capture(s, d, name, label, proof, rows, captures).await
}

pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let readiness = readiness(name).await?;
    let mut stages = vec![];
    let mut captures = serde_json::Map::new();
    stage(
        state,
        desktop,
        name,
        "about",
        Value::Null,
        &mut stages,
        &mut captures,
    )
    .await?;
    click(desktop, "workflow-configuration").await?;
    wait(state, "metadata editor", |v| {
        v["metadata"]["route"] == "editor" && v["metadata"]["busy"] == false
    })
    .await?;
    input(desktop, "workflow-management-title", TITLE).await?;
    input(desktop, "workflow-management-description", DESCRIPTION).await?;
    wait(state, "exact dirty metadata", exact).await?;
    stage(
        state,
        desktop,
        name,
        "draft",
        Value::Null,
        &mut stages,
        &mut captures,
    )
    .await?;
    click(desktop, "workflow-management-activity").await?;
    settled(state, 0).await?;
    stage(
        state,
        desktop,
        name,
        "list",
        Value::Null,
        &mut stages,
        &mut captures,
    )
    .await?;
    let mut replacement = Value::Null;
    if case() == "date-range-retirement" {
        mode(&state.peek(), Mode::Held)?;
        open(desktop).await?;
        day(desktop, "2026-06-15").await?;
        day(desktop, "2026-06-16").await?;
        let selected = inspect(desktop).await?;
        click(desktop, "workflow-activity-calendar-save").await?;
        closed(desktop).await?;
        wait(state, "held changed range", |v| {
            v["ledger"]["held"] == 2 && v["metadata"]["busy"] == true
        })
        .await?;
        stage(
            state,
            desktop,
            name,
            "held",
            json!({"selected":selected,"epochs":action(desktop,"epochs",Value::Null).await?}),
            &mut stages,
            &mut captures,
        )
        .await?;
        replacement = replace_account(state)?;
        wait(state, "original calendar read cancelled", |v| {
            v["ledger"]["cancelled"] == 1
        })
        .await?;
        let late = release(&state.peek())?;
        if late["cancelledBeforeReply"] != true || late["delivered"] != false {
            return Err("Retired calendar read delivered".into());
        }
        let retained = state
            .peek()
            .app_home
            .about
            .management
            .documents
            .iter()
            .any(|d| d.title == TITLE && d.description == DESCRIPTION && d.dirty());
        if !retained {
            return Err("Calendar account retirement lost original dirty metadata".into());
        }
        replacement["lateReply"] = late;
        replacement["retainedDraftExact"] = json!(retained);
        wait_dom(desktop,"({ok:!!document.getElementById('workflow-configuration')&&!document.getElementById('workflow-management')&&!document.getElementById('workflow-activity-calendar')})").await?;
        stage(
            state,
            desktop,
            name,
            "account-replaced",
            replacement.clone(),
            &mut stages,
            &mut captures,
        )
        .await?;
    } else {
        exercises::run(state, desktop, name, &mut stages, &mut captures).await?;
        click(desktop, "workflow-activity-back").await?;
        wait(state, "exact metadata returned", |v| {
            exact(v) && v["activity"]["active"] == false && v["metadata"]["writes"] == 0
        })
        .await?;
        stage(
            state,
            desktop,
            name,
            "returned",
            Value::Null,
            &mut stages,
            &mut captures,
        )
        .await?;
    }
    Ok(
        json!({"ok":true,"fixture":name,"case":case(),"readiness":readiness,"viewport":stages.last().unwrap()["geometry"]["viewport"],"aboutConfiguration":true,"actualActivityEntry":true,"noLiveNetwork":true,"stages":stages,"captures":captures,"replacement":replacement,"final":snapshot(&state.peek())}),
    )
}
