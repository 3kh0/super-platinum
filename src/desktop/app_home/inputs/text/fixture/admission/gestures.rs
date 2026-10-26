use super::*;
pub(super) async fn calendar(
    s: Signal<ShellState>,
    value: &str,
    observe: bool,
) -> Result<Value, String> {
    let before = snapshot(&s.peek());
    let script = format!(
        "await(async()=>{{{}const id={},value={},observe={},authority={};{} }})()",
        capture::ACTION_GUARD,
        json!(crate::view::app_modal::field_id(&key("day"))),
        json!(value),
        json!(observe),
        before["authority"],
        include_str!("calendar.js")
    );
    let dom = capture::dom(&script).await?;
    let after = snapshot(&s.peek());
    for k in [
        "authority",
        "rawView",
        "formState",
        "fields",
        "textMounts",
        "events",
        "queue",
        "requests",
        "responses",
    ] {
        if before[k] != after[k] {
            return Err(format!("Calendar observation changed {k}"));
        }
    }
    Ok(json!({"dom":dom,"before":before,"after":after}))
}

// Observe genuine retirement only; never remount or close a native entry here.
pub(super) async fn retired(start: &Value) -> Result<Value, String> {
    let index = start["attempt"]
        .as_u64()
        .ok_or("Missing admission attempt")? as usize;
    let expected = &start["held"]["admission"]["attempts"][index]["expected"];
    capture::dom(&format!(
        "await(async()=>{{const expected={};{} }})()",
        expected,
        include_str!("retired.js")
    ))
    .await
}
