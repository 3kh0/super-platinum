//! Actual initial/cleared query controls with separate original response ledgers.
use super::*;

async fn take(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    visit: u64,
    label: &str,
    before: Value,
    activation: Value,
    extra: Value,
    captures: &mut serde_json::Map<String, Value>,
    stages: &mut Vec<Value>,
) -> Result<Value, String> {
    let after = ready(state, desktop, visit).await?;
    stage(
        state,
        desktop,
        name,
        label,
        visit,
        json!({"before":before,"activation":activation,"extra":extra,"after":after}),
        captures,
        stages,
    )
    .await?;
    Ok(after)
}
async fn query(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    visit: u64,
    label: &str,
    text: &str,
    before: Value,
    captures: &mut serde_json::Map<String, Value>,
    stages: &mut Vec<Value>,
) -> Result<Value, String> {
    let activation = input(desktop, text).await?;
    until(state, visit, "original empty input committed", |p, _| {
        value(p)["search"]["query"] == text
            && value(p)["search"]["status"] != "searching"
            && value(p)["bound"] == true
    })
    .await?;
    take(
        state,
        desktop,
        name,
        visit,
        label,
        before,
        activation,
        Value::Null,
        captures,
        stages,
    )
    .await
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    visit: u64,
    captures: &mut serde_json::Map<String, Value>,
    stages: &mut Vec<Value>,
) -> Result<Value, String> {
    let scenario = data::empty::case();
    let before = observe(state, desktop, visit).await?;
    let activation = action(desktop, "click", "workflow-confirmation-share-open", "").await?;
    let mut current = settled_paint(state, desktop, visit, true).await?;
    stage(
        state,
        desktop,
        name,
        "share-open",
        visit,
        json!({"before":before,"activation":activation,"after":current}),
        captures,
        stages,
    )
    .await?;
    if !matches!(
        scenario.as_str(),
        "initial" | "cold-failure" | "cold-missing" | "cold-offline"
    ) {
        // The original empty top24 can contain only people. Use the actual
        // prefix input to reveal channels before choosing the retained target.
        current = query(
            state,
            desktop,
            name,
            visit,
            "channel-choices",
            "#",
            current,
            captures,
            stages,
        )
        .await?;
        let before = current;
        let text = action(desktop, "text", "workflow-share-message", backend::MESSAGE).await?;
        until(state, visit, "original optional message settled", |p, _| {
            value(p)["text"] == backend::MESSAGE && value(p)["bound"] == true
        })
        .await?;
        settled_paint(state, desktop, visit, false).await?;
        let selected = action(
            desktop,
            "select",
            "workflow-share-destination",
            "CADDDEST01",
        )
        .await?;
        until(state, visit, "original destination retained", |p, _| {
            value(p)["selected"] == "CADDDEST01" && value(p)["bound"] == true
        })
        .await?;
        current = take(
            state,
            desktop,
            name,
            visit,
            "selection-retained",
            before,
            selected,
            json!({"text":text}),
            captures,
            stages,
        )
        .await?;
    }
    match scenario.as_str() {
        "clear" => {
            let (pending, serial) = super::pending(
                state,
                desktop,
                name,
                visit,
                "query-pending",
                "review",
                current,
                captures,
                stages,
            )
            .await?;
            current = query(
                state,
                desktop,
                name,
                visit,
                "empty-cleared",
                "",
                pending,
                captures,
                stages,
            )
            .await?;
            let model = backend::capture(&state.peek())?;
            let release = data::release(&model, serial)?;
            until(state, visit, "old query actual retirement", |_, s| {
                s["confirmationAdd"]["share"]["search"]["settlements"]
                    .as_array()
                    .is_some_and(|r| {
                        r.iter()
                            .any(|v| v["serial"] == serial && v["returned"] == false)
                    })
            })
            .await?;
            current = take(
                state,
                desktop,
                name,
                visit,
                "late-settled",
                current,
                Value::Null,
                json!({"release":release}),
                captures,
                stages,
            )
            .await?;
            let old = value(&current["firstPublish"])["search"]["revision"]
                .as_u64()
                .ok_or("Original revision absent")?;
            let refresh = action(desktop, "click", "workflow-share-refresh", "").await?;
            until(state, visit, "actual empty Refresh committed", |p, _| {
                value(p)["search"]["revision"]
                    .as_u64()
                    .is_some_and(|r| r > old)
                    && value(p)["bound"] == true
                    && value(p)["search"]["status"] != "searching"
            })
            .await?;
            current = take(
                state,
                desktop,
                name,
                visit,
                "empty-refreshed",
                current,
                refresh,
                Value::Null,
                captures,
                stages,
            )
            .await?;
        }
        "prefixes" => {
            current = query(
                state,
                desktop,
                name,
                visit,
                "member-empty",
                "@",
                current,
                captures,
                stages,
            )
            .await?;
            current = query(
                state,
                desktop,
                name,
                visit,
                "channel-empty",
                "#",
                current,
                captures,
                stages,
            )
            .await?;
            let (pending, serial) = super::pending(
                state,
                desktop,
                name,
                visit,
                "whitespace-pending",
                " ",
                current,
                captures,
                stages,
            )
            .await?;
            let model = backend::capture(&state.peek())?;
            let release = data::release(&model, serial)?;
            current = take(
                state,
                desktop,
                name,
                visit,
                "whitespace-ready",
                pending,
                Value::Null,
                json!({"release":release}),
                captures,
                stages,
            )
            .await?;
            current = query(
                state,
                desktop,
                name,
                visit,
                "empty-cleared",
                "",
                current,
                captures,
                stages,
            )
            .await?;
        }
        "warm-offline" => {
            let transition = backend::connectivity::offline(&mut state.write())?;
            // Even equal empty text is an actual new input/revision.
            let activation = input(desktop, "").await?;
            let old = value(&current["firstPublish"])["search"]["revision"]
                .as_u64()
                .ok_or("Original revision absent")?;
            until(state, visit, "offline empty committed", |p, _| {
                value(p)["search"]["revision"]
                    .as_u64()
                    .is_some_and(|r| r > old)
                    && value(p)["search"]["status"] != "searching"
                    && value(p)["bound"] == true
            })
            .await?;
            current = take(
                state,
                desktop,
                name,
                visit,
                "offline-empty",
                current,
                activation,
                json!({"transition":transition}),
                captures,
                stages,
            )
            .await?;
            current = query(
                state,
                desktop,
                name,
                visit,
                "offline-query",
                "review",
                current,
                captures,
                stages,
            )
            .await?;
        }
        "initial" | "cold-failure" | "cold-missing" | "cold-offline" => {}
        _ => return Err("Unknown empty query fixture".into()),
    }
    let activation = action(desktop, "click", "workflow-share-cancel", "").await?;
    until(state, visit, "original empty Cancel", |p, _| {
        last(p)["closed"] == true && value(p)["active"] == false
    })
    .await?;
    let after = observe(state, desktop, visit).await?;
    stage(state,desktop,name,"share-cancelled",visit,json!({"before":current,"activation":activation,"after":after,"quiet":quiet(state,desktop,visit).await?}),captures,stages).await?;
    Ok(Value::Null)
}
