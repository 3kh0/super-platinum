//! Actual original recipient events; one group opening and one rich send.
use super::*;
use backend::multiple as data;
mod tokens;

pub(super) fn native_base(base: &str) -> String {
    if data::requested() {
        base.replace("'#review-destination'", &json!(data::DISPLAY).to_string())
    } else {
        base.into()
    }
}
pub(super) fn native(base: &str) -> String {
    if data::requested() {
        include_str!("multiple/native.js").replace("BASE", base)
    } else {
        base.into()
    }
}
async fn take(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    label: &str,
    visit: u64,
    proof: Value,
    captures: &mut serde_json::Map<String, Value>,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    stage(state, desktop, name, label, visit, proof, captures, stages).await
}
async fn stable(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
) -> Result<Value, String> {
    until(
        state,
        visit,
        "recipient source and original binding settled",
        |p, _| {
            let q = value(p);
            q["bound"] == true
                && q["loading"] == false
                && (matches!(
                    q["search"]["status"].as_str(),
                    Some("ready" | "partial" | "failed")
                ) || q["search"]["status"] == "recipient"
                    && q["groupOpening"]["status"] == "failed")
        },
    )
    .await?;
    until(
        state,
        visit,
        "original ranking callbacks settled",
        |_, s| {
            let ledger = &s["confirmationAdd"]["share"]["ranking"];
            ledger["requests"]
                .as_array()
                .zip(ledger["settlements"].as_array())
                .is_some_and(|(requests, settled)| requests.len() == settled.len())
        },
    )
    .await?;
    let expected = value(&product::snapshot(&state.peek(), visit)).clone();
    let script = include_str!("search/ready.js")
        .replace(
            "['ready','failed','partial']",
            "['ready','failed','partial','recipient']",
        )
        .replace("EXPECTED_SHARE", &expected.to_string())
        .replace("INITIAL_OPEN", "false");
    evidence::observed_wait(
        state,
        desktop,
        visit,
        "original recipient settled controls",
        &script,
    )
    .await?;
    if value(&product::snapshot(&state.peek(), visit)) != &expected {
        return Err("Recipient source changed during paint".into());
    }
    observe(state, desktop, visit).await
}
// Removing a token schedules a debounced search after binding. The preceding
// settled search is not evidence that this removal's search has completed.
async fn stable_after_removal(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
    before: &Value,
) -> Result<Value, String> {
    let revision = value(&before["firstPublish"])["search"]["revision"]
        .as_u64()
        .ok_or("Original pre-removal search revision missing")?;
    until(state, visit, "removal search revision committed", |p, _| {
        value(p)["search"]["revision"]
            .as_u64()
            .is_some_and(|current| current > revision)
    })
    .await?;
    stable(state, desktop, visit).await
}
async fn select(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
    peer: &str,
) -> Result<Value, String> {
    let q = value(&product::snapshot(&state.peek(), visit)).clone();
    let id = if q["options"]
        .as_array()
        .is_some_and(|rows| rows.iter().any(|r| r["id"] == peer))
    {
        peer
    } else if peer == data::PEERS[0]
        && q["options"]
            .as_array()
            .is_some_and(|rows| rows.iter().any(|r| r["id"] == backend::dm::CHANNEL))
    {
        backend::dm::CHANNEL
    } else {
        return Err(format!("Actual recipient option absent: {peer}"));
    };
    action(desktop, "select", "workflow-share-destination", id).await
}
async fn token(desktop: &DesktopContext, kind: &str, peer: &str) -> Result<Value, String> {
    evaluate(
        desktop,
        &include_str!("multiple/action.js")
            .replace("KIND", &json!(kind).to_string())
            .replace("PEER", &json!(peer).to_string()),
    )
    .await
}
async fn cancel(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    visit: u64,
    before: Value,
    captures: &mut serde_json::Map<String, Value>,
    stages: &mut Vec<Value>,
) -> Result<Value, String> {
    let activation = action(desktop, "click", "workflow-share-cancel", "").await?;
    until(state, visit, "original group Cancel", |p, _| {
        last(p)["closed"] == true && value(p)["active"] == false
    })
    .await?;
    let after = observe(state, desktop, visit).await?;
    take(
        state,
        desktop,
        name,
        "share-cancelled",
        visit,
        json!({"before":before,"activation":activation,"after":after}),
        captures,
        stages,
    )
    .await?;
    Ok(after)
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
    let opened = stable(state, desktop, visit).await?;
    take(
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
    let input = action(desktop, "text", "workflow-share-message", backend::MESSAGE).await?;
    until(state, visit, "original group optional text", |p, _| {
        value(p)["text"] == backend::MESSAGE
    })
    .await?;
    stable(state, desktop, visit).await?;
    let selection = select(state, desktop, visit, data::PEERS[0]).await?;
    until(state, visit, "first original human token", |p, _| {
        value(p)["recipients"]
            .as_array()
            .is_some_and(|v| v.len() == 1)
            && value(p)["selected"] == backend::dm::CHANNEL
    })
    .await?;
    let first = stable(state, desktop, visit).await?;
    take(
        state,
        desktop,
        name,
        "first-recipient",
        visit,
        json!({"before":opened,"input":input,"activation":selection,"after":first}),
        captures,
        stages,
    )
    .await?;
    let first = if data::case() == "success" {
        tokens::exercise(state, desktop, name, visit, first, captures, stages).await?
    } else {
        first
    };
    let activation = select(state, desktop, visit, data::PEERS[1]).await?;
    let model = backend::capture(&state.peek())?;
    let reuse = data::case() == "reuse";
    let mut release = Value::Null;
    let pending = if reuse {
        first.clone()
    } else {
        until(state, visit, "actual group open held", |p, s| {
            value(p)["groupOpening"]["status"] == "opening"
                && s["confirmationAdd"]["share"]["multiple"]["held"] == json!([1])
        })
        .await?;
        let pending = observe(state, desktop, visit).await?;
        if pending["native"]["share"]["controls"]["text"]["readOnly"] != true
            || pending["native"]["share"]["controls"]["cancel"]["disabled"] != false
            || pending["native"]["share"]["controls"]["send"]["disabled"] != true
        {
            return Err("Held group controls changed".into());
        }
        take(state,desktop,name,"group-opening",visit,json!({"before":first,"activation":activation,"after":pending,"quiet":quiet(state,desktop,visit).await?}),captures,stages).await?;
        pending
    };
    if data::case() == "held-change-cancel" {
        let removed = token(desktop, "remove", data::PEERS[1]).await?;
        until(state, visit, "held group recipient retired", |p, _| {
            value(p)["recipients"]
                .as_array()
                .is_some_and(|v| v.len() == 1)
                && value(p)["selected"] == backend::dm::CHANNEL
        })
        .await?;
        let after = stable_after_removal(state, desktop, visit, &pending).await?;
        take(
            state,
            desktop,
            name,
            "recipient-changed",
            visit,
            json!({"before":pending,"activation":removed,"after":after}),
            captures,
            stages,
        )
        .await?;
        let cancelled = cancel(state, desktop, name, visit, after, captures, stages).await?;
        let release = data::release(&model)?;
        until(state, visit, "old group result retained only", |_, s| {
            s["confirmationAdd"]["share"]["multiple"]["settlements"]
                .as_array()
                .is_some_and(|v| v.len() == 1)
        })
        .await?;
        take(state,desktop,name,"late-settled",visit,json!({"before":cancelled,"release":release,"after":observe(state,desktop,visit).await?,"quiet":quiet(state,desktop,visit).await?}),captures,stages).await?;
        return Ok(Value::Null);
    }
    if !reuse {
        release = data::release(&model)?;
    }
    let success = matches!(data::case().as_str(), "success" | "reuse");
    until(state, visit, "group resolution terminal", |p, _| {
        value(p)["groupOpening"]["status"] == if success { "ready" } else { "failed" }
    })
    .await?;
    let resolved = stable(state, desktop, visit).await?;
    take(state,desktop,name,if success{"group-ready"}else{"group-failed"},visit,json!({"before":pending,"activation":if reuse{activation}else{Value::Null},"release":release,"after":resolved,"quiet":quiet(state,desktop,visit).await?}),captures,stages).await?;
    if !success {
        cancel(state, desktop, name, visit, resolved, captures, stages).await?;
        return Ok(Value::Null);
    }
    let activation = action(desktop, "click", "workflow-share-send", "").await?;
    until(state, visit, "one group rich send consumed", |p, _| {
        value(p)["operation"]["finished"] == true && last(p)["closed"] == true
    })
    .await?;
    let sent = observe(state, desktop, visit).await?;
    take(
        state,
        desktop,
        name,
        "share-settled",
        visit,
        json!({"before":resolved,"activation":activation,"after":sent}),
        captures,
        stages,
    )
    .await?;
    let activation = action(desktop, "sidebar", data::DISPLAY, "").await?;
    let script = format!(
        "({{ok:document.getElementById('channel-details-trigger')?.textContent==={}&&!!document.getElementById('message-timeline')&&!document.getElementById('workflow-finish-up')}})",
        json!(data::DISPLAY)
    );
    evidence::observed_wait(
        state,
        desktop,
        visit,
        "actual group destination timeline",
        &script,
    )
    .await?;
    take(
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
