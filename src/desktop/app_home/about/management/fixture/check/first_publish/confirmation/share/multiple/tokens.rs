use super::*;

pub(super) async fn exercise(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    visit: u64,
    first: Value,
    captures: &mut serde_json::Map<String, Value>,
    stages: &mut Vec<Value>,
) -> Result<Value, String> {
    let activation = token(desktop, "remove", data::PEERS[0]).await?;
    until(
        state,
        visit,
        "pointer removes original single token",
        |p, _| value(p)["recipients"] == json!([]) && value(p)["selected"] == "",
    )
    .await?;
    let removed = stable_after_removal(state, desktop, visit, &first).await?;
    if removed["native"]["share"]["controls"]["send"]["disabled"] != true {
        return Err("Empty token selection permitted Send".into());
    }
    take(
        state,
        desktop,
        name,
        "token-removed",
        visit,
        json!({"before":first,"activation":activation,"after":removed}),
        captures,
        stages,
    )
    .await?;
    let added = select(state, desktop, visit, data::PEERS[0]).await?;
    until(state, visit, "single token readded", |p, _| {
        value(p)["recipients"]
            .as_array()
            .is_some_and(|v| v.len() == 1)
            && value(p)["selected"] == backend::dm::CHANNEL
    })
    .await?;
    let restored = stable(state, desktop, visit).await?;
    let activation = token(desktop, "Backspace", data::PEERS[0]).await?;
    until(
        state,
        visit,
        "first Backspace selects last token",
        |p, _| value(p)["selectedRecipient"] == 0,
    )
    .await?;
    let selected = stable(state, desktop, visit).await?;
    take(state,desktop,name,"token-selected",visit,json!({"before":removed,"readd":added,"restored":restored,"activation":activation,"after":selected}),captures,stages).await?;
    let activation = token(desktop, "Backspace", data::PEERS[0]).await?;
    until(
        state,
        visit,
        "second Backspace removes selected token",
        |p, _| value(p)["recipients"] == json!([]) && value(p)["selected"] == "",
    )
    .await?;
    let empty = stable_after_removal(state, desktop, visit, &selected).await?;
    if empty["native"]["share"]["controls"]["send"]["disabled"] != true {
        return Err("Backspace emptied tokens but Send remained enabled".into());
    }
    take(
        state,
        desktop,
        name,
        "tokens-empty",
        visit,
        json!({"before":selected,"activation":activation,"after":empty}),
        captures,
        stages,
    )
    .await?;
    let activation = select(state, desktop, visit, data::PEERS[0]).await?;
    until(state, visit, "final first human restored", |p, _| {
        value(p)["recipients"]
            .as_array()
            .is_some_and(|v| v.len() == 1)
            && value(p)["selected"] == backend::dm::CHANNEL
    })
    .await?;
    let restored = stable(state, desktop, visit).await?;
    take(
        state,
        desktop,
        name,
        "token-restored",
        visit,
        json!({"before":empty,"activation":activation,"after":restored}),
        captures,
        stages,
    )
    .await?;
    Ok(restored)
}
