use super::*;

pub(super) async fn action(d: &DesktopContext, name: &str, args: Value) -> Result<Value, String> {
    let script = include_str!("actions.js")
        .replace("ACTION", &json!(name).to_string())
        .replace("ARGS", &args.to_string());
    wait_dom(d, &script).await
}
pub(in super::super) async fn inspect(d: &DesktopContext) -> Result<Value, String> {
    Ok(action(d, "snapshot", Value::Null).await?["dateRange"].clone())
}
pub(super) async fn open(d: &DesktopContext) -> Result<(), String> {
    native::reveal_filters(d).await?;
    click(d, "workflow-activity-date-range").await?;
    wait_dom(
        d,
        "({ok:!!document.getElementById('workflow-activity-calendar')&&document.getElementById('workflow-activity-date-range')?.dataset.calendarPending==='false'})",
    )
    .await?;
    Ok(())
}
pub(super) async fn closed(d: &DesktopContext) -> Result<(), String> {
    wait_dom(
        d,
        "({ok:!document.getElementById('workflow-activity-calendar')&&document.getElementById('workflow-activity-date-range')?.dataset.calendarPending==='false'})",
    )
    .await?;
    Ok(())
}
pub(super) async fn month(d: &DesktopContext, date: &str) -> Result<(), String> {
    let desired = &date[..7];
    for _ in 0..36 {
        let current = action(d, "month", Value::Null).await?;
        let value = current["month"]
            .as_str()
            .ok_or("Missing actual calendar month")?;
        if value == desired {
            return Ok(());
        }
        click(
            d,
            if value < desired {
                "workflow-activity-calendar-next"
            } else {
                "workflow-activity-calendar-prev"
            },
        )
        .await?;
        wait_dom(d,&format!("({{ok:document.getElementById('workflow-activity-calendar')?.dataset.month!=={}&&document.getElementById('workflow-activity-date-range')?.dataset.calendarPending==='false'}})",json!(value))).await?;
    }
    Err("Calendar month navigation exceeded bounded fixture range".into())
}
pub(super) async fn day(d: &DesktopContext, date: &str) -> Result<Value, String> {
    month(d, date).await?;
    let before = inspect(d).await?;
    let start = before["calendar"]["start"].as_str().unwrap_or("");
    let end = before["calendar"]["end"].as_str().unwrap_or("");
    let (expected_start, expected_end) = if start.is_empty() || !end.is_empty() {
        (date, "")
    } else if date < start {
        (date, start)
    } else {
        (start, date)
    };
    let proof = action(d, "day", json!({"date":date})).await?;
    // A second same-day click must wait for end, not just the already-matching start.
    wait_dom(d,&format!("(()=>{{const c=document.getElementById('workflow-activity-calendar'),b=document.getElementById('workflow-activity-date-range');return{{ok:c?.dataset.start==={}&&c?.dataset.end==={}&&b?.dataset.calendarPending==='false'}}}})()",json!(expected_start),json!(expected_end))).await?;
    Ok(proof)
}
pub(super) async fn settled(s: Signal<ShellState>, before: usize) -> Result<Value, String> {
    wait(s, "one calendar-triggered read settled", |v| {
        v["ledger"]["requests"]
            .as_array()
            .is_some_and(|r| r.len() == before + 1)
            && v["metadata"]["busy"] == false
            && v["ledger"]["finished"] == json!(before + 1)
    })
    .await
}
pub(in super::super) async fn select(
    s: Signal<ShellState>,
    d: &DesktopContext,
    from: &str,
    to: &str,
) -> Result<Value, String> {
    open(d).await?;
    let first = day(d, from).await?;
    let second = day(d, to).await?;
    let selected = inspect(d).await?;
    let before = count(s);
    click(d, "workflow-activity-calendar-save").await?;
    closed(d).await?;
    settled(s, before).await?;
    Ok(
        json!({"first":first,"second":second,"selected":selected,"epochs":action(d,"epochs",Value::Null).await?}),
    )
}
