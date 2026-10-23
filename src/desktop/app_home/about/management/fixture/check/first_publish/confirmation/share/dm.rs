//! Actual original Share controls for unavailable/retired existing-DM reads.
use super::*;
use backend::dm as data;

pub(super) fn native_base(base: &str) -> String {
    if data::requested() {
        // The existing destination observer remains exact; the opted-in
        // human IM uses the actual profile header button, not ChannelDetailsButton.
        base.replace(
            "document.getElementById('channel-details-trigger')",
            include_str!("dm/header.js").trim(),
        )
        .replace("'#review-destination'", &json!(data::LABEL).to_string())
    } else {
        base.to_owned()
    }
}
pub(super) async fn run(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    visit: u64,
    captures: &mut serde_json::Map<String, Value>,
    stages: &mut Vec<Value>,
) -> Result<Value, String> {
    let before = observe(state, desktop, visit).await?;
    let activation = action(desktop, "click", "workflow-confirmation-share-open", "").await?;
    let held = data::case() == "held-cancel";
    if held {
        until(state, visit, "actual original DM peer read held", |p, s| {
            value(p)["bound"] == true
                && value(p)["loading"] == true
                && s["confirmationAdd"]["share"]["dm"]["held"] == json!([3])
        })
        .await?;
    } else {
        until(
            state,
            visit,
            "unavailable original DM listing settled",
            |p, _| value(p)["bound"] == true && value(p)["loading"] == false,
        )
        .await?;
    }
    let opened = observe(state, desktop, visit).await?;
    stage(
        state,
        desktop,
        name,
        "share-open",
        visit,
        json!({"before":before,"activation":activation,"after":opened}),
        captures,
        stages,
    )
    .await?;
    let original = backend::capture(&state.peek())?;
    stage(state,desktop,name,if held{"share-held"}else{"share-unavailable"},visit,
        json!({"before":opened,"after":observe(state,desktop,visit).await?,"quiet":quiet(state,desktop,visit).await?}),captures,stages).await?;
    let before = observe(state, desktop, visit).await?;
    let cancel = action(desktop, "click", "workflow-share-cancel", "").await?;
    until(state, visit, "original DM Share Cancel", |p, _| {
        last(p)["closed"] == true && value(p)["active"] == false
    })
    .await?;
    let cancelled = observe(state, desktop, visit).await?;
    stage(
        state,
        desktop,
        name,
        "share-cancelled",
        visit,
        json!({"before":before,"activation":cancel,"after":cancelled}),
        captures,
        stages,
    )
    .await?;
    if held {
        let release = data::release(&original)?;
        until(
            state,
            visit,
            "original DM peer response settled after Cancel",
            |_, s| {
                s["confirmationAdd"]["share"]["dm"]["settlements"]
                    .as_array()
                    .is_some_and(|rows| rows.iter().any(|r| r["serial"] == 3))
            },
        )
        .await?;
        stage(state,desktop,name,"late-settled",visit,
            json!({"before":cancelled,"release":release,"after":observe(state,desktop,visit).await?,"quiet":quiet(state,desktop,visit).await?}),captures,stages).await?;
    }
    Ok(Value::Null)
}
