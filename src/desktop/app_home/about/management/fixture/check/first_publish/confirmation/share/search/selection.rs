use super::*;
pub(super) async fn run(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    visit: u64,
    captures: &mut serde_json::Map<String, Value>,
    stages: &mut Vec<Value>,
) -> Result<Value, String> {
    let person = data::person_enabled();
    let id = if person {
        backend::dm::PEER
    } else if let Some(id) = data::ranking::destination() {
        id
    } else {
        data::CHANNEL
    };
    let before = observe(state, desktop, visit).await?;
    let activation = action(desktop, "select", "workflow-share-destination", id).await?;
    let mut selection_proof = json!({"before":before,"activation":activation});
    if person {
        until(
            state,
            visit,
            "search selected person actual open held",
            |p, s| {
                value(p)["opening"]["status"] == "opening"
                    && s["confirmationAdd"]["share"]["person"]["held"] == json!([1])
            },
        )
        .await?;
        let pending = observe(state, desktop, visit).await?;
        selection_proof["after"] = pending.clone();
        stage(
            state,
            desktop,
            name,
            "person-opening",
            visit,
            selection_proof,
            captures,
            stages,
        )
        .await?;
        let model = backend::capture(&state.peek())?;
        let release = backend::person::release(&model)?;
        selection_proof = json!({"before":pending,"release":release});
    }
    let destination = if person {
        backend::dm::CHANNEL
    } else if let Some(id) = data::ranking::destination() {
        id
    } else {
        data::CHANNEL
    };
    until(
        state,
        visit,
        "search original destination validated",
        |p, _| {
            value(p)["bound"] == true
                && value(p)["loading"] == false
                && value(p)["selected"] == destination
        },
    )
    .await?;
    let selected = ready(state, desktop, visit).await?;
    selection_proof["after"] = selected.clone();
    stage(
        state,
        desktop,
        name,
        "selection-ready",
        visit,
        selection_proof,
        captures,
        stages,
    )
    .await?;
    let activation = action(desktop, "click", "workflow-share-send", "").await?;
    until(state, visit, "search explicit Share consumed", |p, _| {
        value(p)["operation"]["finished"] == true && last(p)["closed"] == true
    })
    .await?;
    let sent = observe(state, desktop, visit).await?;
    stage(
        state,
        desktop,
        name,
        "share-settled",
        visit,
        json!({"before":selected,"activation":activation,"after":sent}),
        captures,
        stages,
    )
    .await?;
    let label = if person {
        backend::dm::LABEL
    } else if data::ranking::destination().is_some() {
        data::ranking::DESTINATION_LABEL
    } else {
        data::LABEL
    };
    let activation = action(desktop, "sidebar", label, "").await?;
    let header = if person {
        include_str!("../dm/header.js").trim()
    } else {
        "document.getElementById('channel-details-trigger')"
    };
    let label = if person {
        label.into()
    } else {
        format!("#{label}")
    };
    let script = format!(
        "({{ok:({header})?.textContent==={}&&!!document.getElementById('message-timeline')&&!document.getElementById('workflow-finish-up')}})",
        json!(label)
    );
    evidence::observed_wait(
        state,
        desktop,
        visit,
        "actual searched destination timeline",
        &script,
    )
    .await?;
    stage(
        state,
        desktop,
        name,
        "destination",
        visit,
        json!({"before":sent,"activation":activation,"after":observe(state,desktop,visit).await?}),
        captures,
        stages,
    )
    .await?;
    Ok(Value::Null)
}
