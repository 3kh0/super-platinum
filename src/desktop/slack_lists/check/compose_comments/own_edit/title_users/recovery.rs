use super::*;
pub(super) async fn run(
    mut state: Signal<ShellState>,
    m: &Model,
    d: &Desktop,
    p: &mut Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    source::invalidate(&mut state.write()).map_err(|e| e.to_string())?;
    p["invalidated"] = source::snapshot(&state.peek());
    command(d, "open", json!({"ts":data::RECEIVED_TS})).await?;
    let first = next(state, m, "discovery").await?;
    pending(state, m, d, "discovery", p, rows).await?;
    release(state, p, first, "discovery", true)?;
    let member = next(state, m, "member").await?;
    pending(state, m, d, "member-held", p, rows).await?;
    release(state, p, member, "member", false)?;
    wait(state, m, |v| {
        v["edit"]["active"]["titleUsers"]["status"] == "failed"
            && v["titleSource"]["knownDriver"]["active"] == 0
    })
    .await?;
    pending(state, m, d, "failure", p, rows).await?;
    action(d, p, "slack-list-comment-edit-title-retry").await?;
    let discovery = next(state, m, "discovery").await?;
    pending(state, m, d, "retry-discovery", p, rows).await?;
    release(state, p, discovery, "discovery", true)?;
    let member = next(state, m, "member").await?;
    pending(state, m, d, "retry-member", p, rows).await?;
    release(state, p, member, "member", true)?;
    editor(d, true).await?;
    ready(state, m, d, p).await?;
    let typed = command(
        d,
        "append",
        json!({"text":" Recovered edit B \u{65e5}\u{1f98a}"}),
    )
    .await?;
    p["editedBlocks"] = typed["blocks"].clone();
    durable(state, &p["editedBlocks"]).await?;
    capture(state, m, d, "editing", p, rows).await?;
    save(state, m, d, p, rows).await
}
