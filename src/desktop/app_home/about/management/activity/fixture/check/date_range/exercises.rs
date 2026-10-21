use super::*;

pub(super) async fn run(
    state: Signal<ShellState>,
    d: &DesktopContext,
    name: &str,
    rows: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let before = custody(state);
    open(d).await?;
    let defaults = inspect(d).await?;
    let calendar = &defaults["calendar"];
    let default_from = calendar["start"].as_str().ok_or("Missing default start")?;
    let default_to = calendar["end"].as_str().ok_or("Missing default end")?;
    let epochs = action(d, "epochs", json!({"from":default_from,"to":default_to})).await?;
    if epochs["defaultFrom"] != default_from
        || epochs["today"] != default_to
        || calendar["save"]["disabled"] != false
    {
        return Err(format!(
            "Calendar initial default mismatch: {defaults}, {epochs}"
        ));
    }
    let after = unchanged(state, &before)?;
    stage(
        state,
        d,
        name,
        "default-open",
        json!({"before":before,"after":after,"epochs":epochs}),
        rows,
        captures,
    )
    .await?;
    click(d, "workflow-activity-calendar-save").await?;
    closed(d).await?;
    settled(state, 1).await?;
    stage(
        state,
        d,
        name,
        "default-saved",
        json!({"selected":defaults,"epochs":epochs}),
        rows,
        captures,
    )
    .await?;

    let before = custody(state);
    open(d).await?;
    let first = day(d, "2026-06-15").await?;
    stage(
        state,
        d,
        name,
        "range-draft",
        json!({"before":before,"after":unchanged(state,&before)?,"first":first}),
        rows,
        captures,
    )
    .await?;
    click(d, "workflow-activity-calendar-cancel").await?;
    closed(d).await?;
    let after = unchanged(state, &before)?;
    open(d).await?;
    let reopened = inspect(d).await?;
    if reopened["calendar"]["save"]["disabled"] != true {
        return Err("Cancel/reopen retained an actionable temporary pair".into());
    }
    click(d, "workflow-activity-calendar-cancel").await?;
    closed(d).await?;
    stage(
        state,
        d,
        name,
        "cancelled",
        json!({"before":before,"after":after,"reopened":reopened}),
        rows,
        captures,
    )
    .await?;

    open(d).await?;
    let first = day(d, "2026-01-01").await?;
    controls::month(d, "2026-04-01").await?;
    let refused = action(d, "inspectDay", json!({"date":"2026-04-01"})).await?;
    if refused["cell"]["disabled"] != true {
        return Err("90-day boundary was enabled".into());
    }
    let blocked_before = inspect(d).await?;
    let blocked_click = action(d, "disabledClick", json!({"date":"2026-04-01"})).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let blocked_after = inspect(d).await?;
    if blocked_before != blocked_after {
        return Err("Disabled date activation changed actual calendar".into());
    }
    controls::month(d, "2026-03-31").await?;
    let allowed = action(d, "inspectDay", json!({"date":"2026-03-31"})).await?;
    if allowed["cell"]["disabled"] != false {
        return Err("89-day boundary was disabled".into());
    }
    let second = day(d, "2026-03-31").await?;
    let selected = inspect(d).await?;
    click(d, "workflow-activity-calendar-save").await?;
    closed(d).await?;
    settled(state, 2).await?;
    stage(state,d,name,"boundary",json!({"boundary":{"start":"2026-01-01","allowed":{"date":"2026-03-31","disabled":allowed["cell"]["disabled"]},"blocked":{"date":"2026-04-01","disabled":refused["cell"]["disabled"]}},"disabledActivation":{"before":blocked_before,"after":blocked_after,"event":blocked_click,"observedMs":100},"start":"2026-01-01","allowedDate":"2026-03-31","refusedDate":"2026-04-01","first":first,"second":second,"allowed":allowed,"refused":refused,"selected":selected,"epochs":action(d,"epochs",Value::Null).await?}),rows,captures).await?;

    let mut reversed = select(state, d, "2026-06-16", "2026-06-15").await?;
    reversed["clickOrder"] = json!(["2026-06-16", "2026-06-15"]);
    stage(state, d, name, "reversed", reversed, rows, captures).await?;
    let same = select(state, d, "2026-06-15", "2026-06-15").await?;
    stage(state, d, name, "same-day", same, rows, captures).await?;

    open(d).await?;
    let first = day(d, "2026-06-15").await?;
    action(d, "key", json!({"key":"ArrowDown"})).await?;
    wait_dom(d,"({ok:document.activeElement?.dataset.date==='2026-06-15'&&document.getElementById('workflow-activity-date-range')?.dataset.calendarPending==='false'})").await?;
    let arrow = action(d, "key", json!({"date":"2026-06-15","key":"ArrowRight"})).await?;
    wait_dom(
        d,
        "({ok:document.activeElement?.dataset.date==='2026-06-16'})",
    )
    .await?;
    let focus = inspect(d).await?;
    let enter = action(d, "key", json!({"date":"2026-06-16","key":"Enter"})).await?;
    wait_dom(
        d,
        "({ok:document.getElementById('workflow-activity-calendar')?.dataset.end==='2026-06-16'})",
    )
    .await?;
    let selected = inspect(d).await?;
    click(d, "workflow-activity-calendar-save").await?;
    closed(d).await?;
    settled(state, 5).await?;
    stage(state,d,name,"keyboard",json!({"keyboard":{"beforeDate":"2026-06-15","key":"ArrowRight","afterDate":focus["calendar"]["focusDate"],"selectedDate":"2026-06-16","activation":enter["activation"]},"first":first,"arrow":arrow,"focus":focus,"enter":enter,"selected":selected,"epochs":action(d,"epochs",Value::Null).await?}),rows,captures).await?;

    let mut tab_pending = Value::Null;
    for (key, label) in [("Tab", "tab-closed"), ("Escape", "escape-closed")] {
        let before = custody(state);
        open(d).await?;
        let reopened = inspect(d).await?;
        if key == "Escape"
            && (reopened["calendar"]["start"] != tab_pending["calendar"]["start"]
                || reopened["calendar"]["end"] != tab_pending["calendar"]["end"])
        {
            return Err("Tab close lost temporary pair on actual reopen".into());
        }
        day(d, "2026-06-17").await?;
        let pending = inspect(d).await?;
        if key == "Tab" {
            tab_pending = pending.clone();
        }
        let event = action(d, "key", json!({"key":key})).await?;
        closed(d).await?;
        let after = unchanged(state, &before)?;
        stage(
            state,
            d,
            name,
            label,
            json!({"before":before,"after":after,"pending":pending,"reopened":reopened,"tabPending":tab_pending,"event":event}),
            rows,
            captures,
        )
        .await?;
    }
    // Explicit Cancel removes any temporary state retained by Tab/Escape.
    open(d).await?;
    click(d, "workflow-activity-calendar-cancel").await?;
    closed(d).await?;
    mode(&state.peek(), Mode::Malformed)?;
    let selected = select(state, d, "2026-06-18", "2026-06-19").await?;
    wait(state, "Calendar malformed response guidance", |v| {
        v["activity"]["notice"].is_string() && v["activity"]["page"].is_null()
    })
    .await?;
    // Show the required failure feedback outside the overlaid Filter.
    click(d, "workflow-activity-filter").await?;
    wait_dom(d,"({ok:document.getElementById('workflow-activity-filter')?.getAttribute('aria-expanded')==='false'&&document.getElementById('workflow-activity-date-range')?.dataset.calendarPending==='false'})").await?;
    stage(state, d, name, "malformed", selected, rows, captures).await?;
    click(d, "workflow-activity-apply").await?;
    settled(state, 7).await?;
    stage(
        state,
        d,
        name,
        "retried",
        json!({"epochs":action(d,"epochs",Value::Null).await?}),
        rows,
        captures,
    )
    .await?;
    Ok(())
}
