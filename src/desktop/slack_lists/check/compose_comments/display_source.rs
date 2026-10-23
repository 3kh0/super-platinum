//! Actual supplied URL replacement with independent original reply custody.
use super::*;
use fixture::display_source as source;
fn script(action: &str, ready: bool) -> String {
    format!(
        "({})({},{ready})",
        include_str!("display_source.js"),
        json!(action)
    )
}
async fn take(
    state: Signal<ShellState>,
    model: &Model,
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    ready: bool,
    proof: &Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    stable_drafts(state, model).await?;
    let before = wait_dom(d, &script("observe", ready)).await?;
    let model_before = compose::snapshot(&state.peek(), model);
    let journal = compose::journal_snapshot(state).await?;
    if model_before["noSession"] != true
        || model_before["noTransport"] != true
        || model_before["writes"] != 0
        || model_before["networkCalls"] != 0
        || model_before["ledger"]["requests"]
            .as_array()
            .is_none_or(|r| r.len() != 4)
        || model_before["ledger"]["completed"] != 4
        || model_before["comments"]["loading"] != false
        || before["reply"]["blocks"] != proof["originalA"]
        || journal["blocks"] != proof["originalA"]
    {
        return Err("Display source changed original read/draft custody".into());
    }
    let capture = harness::capture(d, name).await?;
    let after = checked(d, &script("observe", ready)).await?;
    if before != after
        || compose::snapshot(&state.peek(), model) != model_before
        || compose::journal_snapshot(state).await? != journal
    {
        return Err(format!(
            "Original display/draft changed across {name} capture"
        ));
    }
    rows.push(json!({"stage":name,"model":model_before,"journal":journal,"geometry":before["geometry"],"native":{"before":before,"after":after},"source":source::snapshot(&state.peek()),"proof":proof,"capture":capture}));
    Ok(())
}
pub(super) async fn run(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let (readiness, model, mut proof) = edits::setup(state, d).await?;
    stable_drafts(state, &model).await?;
    proof["arm"] = checked(d, &script("arm", false)).await?;
    let mut rows = Vec::new();
    take(state, &model, d, "pending", false, &proof, &mut rows).await?;
    proof["mediaA"] = source::deliver_media(&state.peek(), false)?;
    take(state, &model, d, "ready", true, &proof, &mut rows).await?;
    proof["replacement"] = edits::inject(
        state,
        &model,
        "replacement",
        "T1",
        7,
        source::replacement(),
        true,
    )?;
    take(
        state,
        &model,
        d,
        "replacement-pending",
        false,
        &proof,
        &mut rows,
    )
    .await?;
    proof["mediaB"] = source::deliver_media(&state.peek(), true)?;
    take(
        state,
        &model,
        d,
        "replacement-ready",
        true,
        &proof,
        &mut rows,
    )
    .await?;
    Ok(edits::result(
        source::CASE,
        readiness,
        rows,
        vec![proof["replacement"].clone()],
        state,
        &model,
    ))
}
