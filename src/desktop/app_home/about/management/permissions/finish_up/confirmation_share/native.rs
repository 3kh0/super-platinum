use super::*;
const SCRIPT: &str = include_str!("native.js");
pub(super) async fn bind_entry(o: &Original, local: &Value) -> bool {
    let original = o.expected();
    evaluate(format!("await workflowShareEntryBind({original},{local})")).await
}
pub(super) async fn entry(o: &Original, local: &Value) -> bool {
    let original = o.expected();
    evaluate(format!("workflowShareEntry({original},{local})")).await
}
async fn evaluate(expression: String) -> bool {
    let mut eval = document::eval(&format!(
        "{}\n{SCRIPT}\nawait workflowPermissionsReply({expression},dioxus);",
        super::super::native::source()
    ));
    let result = tokio::time::timeout(Duration::from_secs(3), eval.recv::<bool>()).await;
    let _ = eval.send(());
    matches!(result, Ok(Ok(true)))
}
pub(super) async fn bind(q: &Query) -> bool {
    let original = q.original.expected();
    let local = &q.local;
    let expected = &q.expected;
    evaluate(format!(
        "await workflowShareBind({original},{local},{expected})"
    ))
    .await
}
pub(super) async fn current(q: &Query, action: Option<&Value>) -> bool {
    let original = q.original.expected();
    let local = &q.local;
    let expected = &q.expected;
    let action = action.cloned().unwrap_or(Value::Null);
    evaluate(format!(
        "workflowShareCurrent({original},{local},{expected},{action})"
    ))
    .await
}

/// Read fixed renderer helpers under the same original Share controls. Query
/// replacement may retire this read while the native reply is in flight.
pub(super) async fn observe<T: serde::de::DeserializeOwned>(
    state: Signal<ShellState>,
    q: &Query,
    current: &impl Fn(&ShellState) -> bool,
    helpers: &str,
    expression: &str,
) -> Result<T, Error> {
    if !q.current(&state.peek()) || !current(&state.peek()) {
        return Err(unavailable());
    }
    let original = q.original.expected();
    let local = &q.local;
    let expected = &q.expected;
    // Expressions are fixed internal helper calls with JSON value arguments,
    // never interpolated user source. The helper does not change the DOM.
    let script = format!(
        "{}\n{SCRIPT}\n{helpers}\nconst observed=workflowShareCurrent({original},{local},{expected},null)?({expression}):null;\nawait workflowPermissionsReply(observed,dioxus);",
        super::super::native::source()
    );
    let mut eval = document::eval(&script);
    let reply = tokio::time::timeout(Duration::from_secs(3), eval.recv::<Option<T>>()).await;
    let _ = eval.send(());
    if !q.current(&state.peek()) || !current(&state.peek()) {
        return Err(unavailable());
    }
    reply
        .map_err(|_| unavailable())?
        .map_err(|_| unavailable())?
        .ok_or_else(unavailable)
}
