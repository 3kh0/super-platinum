//! Original person option, held quiet open, and independent explicit Send.
use super::*;
use backend::person as data;

pub(super) fn native_base(base: &str) -> String {
    if !data::requested() {
        return base.into();
    }
    base.replace(
        "document.getElementById('channel-details-trigger')",
        include_str!("dm/header.js").trim(),
    )
    .replace("'#review-destination'", &json!(data::LABEL).to_string())
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
    let opened = settled(state, desktop, visit).await?;
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
    // Put actual optional text in the original buffer before selection locks it.
    let input = action(desktop, "text", "workflow-share-message", backend::MESSAGE).await?;
    until(state, visit, "person original text input", |p, _| {
        value(p)["text"] == backend::MESSAGE && value(p)["bound"] == true
    })
    .await?;
    let selection = action(desktop, "select", "workflow-share-destination", data::PEER).await?;
    until(state, visit, "actual quiet open held", |p, s| {
        value(p)["opening"]["status"] == "opening"
            && s["confirmationAdd"]["share"]["person"]["held"] == json!([1])
    })
    .await?;
    let pending = observe(state, desktop, visit).await?;
    let controls = &pending["native"]["share"]["controls"];
    if controls["text"]["readOnly"] != true
        || controls["cancel"]["disabled"] != false
        || controls["send"]["disabled"] != true
    {
        return Err("Person pending editor/Cancel/Send state changed".into());
    }
    take(state,desktop,name,"person-opening",visit,json!({"before":opened,"input":input,"selectionActivation":selection,"after":pending,"quiet":quiet(state,desktop,visit).await?}),captures,stages).await?;
    let model = backend::capture(&state.peek())?;
    if data::case() == "held-cancel" {
        let activation = action(desktop, "click", "workflow-share-cancel", "").await?;
        until(state, visit, "person original Cancel", |p, _| {
            last(p)["closed"] == true && value(p)["active"] == false
        })
        .await?;
        let cancelled = observe(state, desktop, visit).await?;
        take(
            state,
            desktop,
            name,
            "share-cancelled",
            visit,
            json!({"before":pending,"activation":activation,"after":cancelled}),
            captures,
            stages,
        )
        .await?;
        let release = data::release(&model)?;
        until(state, visit, "retired original open settled", |p, s| {
            value(p)["opening"]["status"] == "retired"
                && s["confirmationAdd"]["share"]["person"]["settlements"]
                    .as_array()
                    .is_some_and(|v| v.len() == 1)
        })
        .await?;
        take(state,desktop,name,"late-settled",visit,json!({"before":cancelled,"release":release,"after":observe(state,desktop,visit).await?,"quiet":quiet(state,desktop,visit).await?}),captures,stages).await?;
        return Ok(Value::Null);
    }
    let release = data::release(&model)?;
    let success = data::case() == "success";
    let settled = until(state, visit, "person selection settled", |p, _| {
        value(p)["bound"] == true
            && value(p)["loading"] == false
            && value(p)["opening"]["status"] == if success { "ready" } else { "failed" }
    })
    .await;
    if let Err(error) = settled {
        // Failure-only passive proof. Do not call the scrolling paint observer or
        // rebind/repair controls while diagnosing the original rejected binding.
        let expected = {
            let s = state.peek();
            product::snapshot(&s, visit)
        };
        let script =
            include_str!("person/diagnostic.js").replace("EXPECTED_PRODUCT", &expected.to_string());
        let diagnostic = evaluate(desktop, &script).await;
        return Err(format!("{error}; personBindingDiagnostic={diagnostic:?}"));
    }
    let expected = {
        let s = state.peek();
        value(&product::snapshot(&s, visit)).clone()
    };
    let script = include_str!("person/ready.js").replace("EXPECTED_SHARE", &expected.to_string());
    evidence::observed_wait(
        state,
        desktop,
        visit,
        "person settled original control paint",
        &script,
    )
    .await?;
    if value(&product::snapshot(&state.peek(), visit)) != &expected {
        return Err("Person selection source changed during settled control paint".into());
    }
    let resolved = observe(state, desktop, visit).await?;
    take(state,desktop,name,if success{"person-ready"}else{"person-failed"},visit,json!({"before":pending,"release":release,"after":resolved,"quiet":quiet(state,desktop,visit).await?}),captures,stages).await?;
    if !success {
        let activation = action(desktop, "click", "workflow-share-cancel", "").await?;
        until(state, visit, "failed person Cancel", |p, _| {
            last(p)["closed"] == true && value(p)["active"] == false
        })
        .await?;
        take(state,desktop,name,"share-cancelled",visit,json!({"before":resolved,"activation":activation,"after":observe(state,desktop,visit).await?}),captures,stages).await?;
        return Ok(Value::Null);
    }
    let activation = action(desktop, "click", "workflow-share-send", "").await?;
    until(
        state,
        visit,
        "person explicit Share queue consumed",
        |p, _| value(p)["operation"]["finished"] == true && last(p)["closed"] == true,
    )
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
    let activation = action(desktop, "sidebar", data::LABEL, "").await?;
    let script = format!(
        "({{ok:({})?.textContent==={}&&!!document.getElementById('message-timeline')&&!document.getElementById('workflow-finish-up')}})",
        include_str!("dm/header.js").trim(),
        json!(data::LABEL)
    );
    evidence::observed_wait(
        state,
        desktop,
        visit,
        "actual person destination timeline",
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
