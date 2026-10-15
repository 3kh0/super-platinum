//! Actual clean denial -> current recovery row -> explicit second Schedule.
use super::*;
pub(super) async fn after_first_rejection(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    binding: &fixture::Binding,
    frozen: &Value,
    first_op: &Value,
    intent: &Value,
    initial_queue: &Value,
    details: &mut serde_json::Map<String, Value>,
    captures: &mut serde_json::Map<String, Value>,
    geometry: &mut Vec<Value>,
) -> Result<(Value, Value), String> {
    let target = binding.target();
    let key = fixture::key(target);
    let rejected = binding
        .owner(&key)
        .load()
        .await
        .map_err(|e| format!("Rejected load: {e:?}"))?
        .stored
        .ok_or("Rejected managed row missing")?;
    let (retained, operation) = rejected
        .state
        .rejected_schedule()
        .ok_or("Raw denial did not produce reviewable rejected Schedule")?;
    ensure(
        json!(operation) == *first_op && json!(retained) == *intent,
        "Denial changed frozen operation/intent",
    )?;
    ensure(
        queue(&state.peek()) == *initial_queue,
        "Denied schedule projected queue row",
    )?;
    ensure(
        binding.requests.lock().unwrap().len() == 1,
        "Denial implicitly retried",
    )?;
    details.insert("firstOperation".into(), first_op.clone());
    details.insert("afterRejection".into(), fixture::durable(binding).await?);
    details.insert(
        "nativeAfterRejection".into(),
        wait_native(target, "").await?,
    );
    let typed = type_text(binding, target, fixture::NEWER).await?;
    details.insert("newerNative".into(), typed);
    details.insert(
        "afterRejectionTyping".into(),
        fixture::durable(binding).await?,
    );
    panel(state, false).await?;
    details.insert(
        "rejectionToast".into(),
        dom(target, "dismiss-notice", json!({"expected":"Message was not scheduled. Review it in Drafts to retry or change the schedule."})).await?,
    );
    capture(desktop, binding, "rejected", target, captures, geometry).await?;
    details.insert(
        "retryOpen".into(),
        dom(target, "open", json!({"rejected":true})).await?,
    );
    let review = wait_dom(target, "read").await?;
    ensure(
        review["blocks"] == frozen["blocks"] && review["postAt"] == frozen["postAt"],
        "Actual rejection review altered rich content/time",
    )?;
    details.insert("retryReview".into(), review);
    capture(desktop, binding, "retry-review", target, captures, geometry).await?;
    binding.arm("response")?;
    details.insert(
        "retrySubmit".into(),
        dom(target, "click", json!({"label":"Retry same schedule"})).await?,
    );
    binding.wait_reached().await?;
    let pending = binding
        .owner(&key)
        .load()
        .await
        .map_err(|e| format!("Retry pending load: {e:?}"))?
        .stored
        .ok_or("Retry state missing")?;
    ensure(
        matches!(pending.state.workflow(), Workflow::InFlight(_)),
        "Retry not durably dispatched",
    )?;
    let operation = pending.state.pending().ok_or("Retry operation missing")?;
    let retained = pending.state.consumption().ok_or("Retry intent missing")?;
    ensure(
        json!(retained) == *intent
            && operation.id != first_op["id"].as_str().unwrap_or("")
            && json!(operation.snapshot) == first_op["snapshot"]
            && json!(operation.kind) == first_op["kind"]
            && json!(operation.base) == first_op["base"]
            && json!(operation.client_msg_id) == first_op["client_msg_id"],
        "Explicit Retry changed frozen intent or reused operation ID",
    )?;
    details.insert("retryPending".into(), fixture::durable(binding).await?);
    ensure(
        details["retryPending"][label(target)]["canonical"]["composer"]["text"] == fixture::NEWER,
        "Retry consumed separate newer composer",
    )?;
    capture(
        desktop,
        binding,
        "retry-pending",
        target,
        captures,
        geometry,
    )
    .await?;
    binding.release();
    wait_dom(target, "closed").await?;
    Ok((json!(operation), json!(retained)))
}
