use super::*;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
static STARTED: AtomicBool = AtomicBool::new(false);
pub(crate) fn start(state: Signal<ShellState>, _desktop: dioxus::desktop::DesktopContext) {
    if !enabled() || STARTED.swap(true, Ordering::Relaxed) {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = match tokio::time::timeout(Duration::from_secs(180), run(state)).await {
            Ok(Ok(v)) => v,
            Ok(Err(error)) => json!({"ok":false,"error":error}),
            Err(_) => json!({"ok":false,"error":"Channel thread emoji timeout"}),
        };
        eprintln!("channel-thread-emoji-check: {result}");
    });
}
async fn call(action: &str, mode: &str) -> Result<Value, String> {
    document::eval(&format!(
        "dioxus.send(({})({},{}));",
        include_str!("native.js"),
        json!(action),
        json!(mode)
    ))
    .recv::<Value>()
    .await
    .map_err(|e| format!("Native evaluation: {e}"))
}
async fn wait(action: &str, mode: &str) -> Result<Value, String> {
    let mut last = Value::Null;
    for _ in 0..150 {
        last = call(action, mode).await?;
        if last["ok"] == true {
            return Ok(last);
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    Err(format!("Native original state unavailable: {last}"))
}
async fn ack(ordinal: usize) -> Result<(), String> {
    let directory = PathBuf::from(
        std::env::var_os("SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR")
            .ok_or("Missing capture ACK directory")?,
    );
    if !directory.is_absolute() {
        return Err("ACK directory not absolute".into());
    }
    for _ in 0..600 {
        let path = directory.join(format!("{ordinal}.ack"));
        match tokio::task::spawn_blocking(move || std::fs::read(path))
            .await
            .map_err(|_| "ACK worker")?
        {
            Ok(bytes) if bytes == b"captured\n" => return Ok(()),
            Ok(_) => return Err("Invalid capture ACK".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err("Capture ACK read failed".into()),
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err("Capture ACK timeout".into())
}
fn isolation(shell: &ShellState) -> Value {
    json!({"syntheticContext":context_enabled(shell),"noHttp":true,"networkDispatched":false,"account":fixture::ACCOUNT,"team":fixture::TEAM,"user":fixture::USER})
}
fn guard(shell: &ShellState, native: &Value) -> Value {
    let c = &native["surfaces"]["channel"];
    let t = &native["surfaces"]["thread"];
    let cg = c["generation"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .unwrap_or(u64::MAX);
    let tg = t["generation"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .unwrap_or(u64::MAX);
    json!({"kind":"retained-original-DOM-measurement","asyncCallbackHeld":false,"sample":native["surfaces"],"channelAccepted":shell.channel_projection_receipt_is_current(cg,c["generation"].as_str()),"threadAccepted":shell.thread_window.projection_receipt_is_current(tg,t["generation"].as_str())})
}
fn custody(snapshot: &Value) -> Value {
    let mut value = snapshot.clone();
    let o = value.as_object_mut().unwrap();
    for key in [
        "requests",
        "witnesses",
        "selected",
        "socket",
        "channelProjection",
        "threadProjection",
        "mediaEpoch",
        "registered",
        "channelWindow",
        "threadWindow",
    ] {
        o.remove(key);
    }
    value
}
async fn stage(
    state: Signal<ShellState>,
    name: &str,
    ordinal: usize,
    mode: &str,
    proof: &Value,
    original: &Value,
    held: Option<&Value>,
) -> Result<Value, String> {
    let mut native = Value::Null;
    let mut model = Value::Null;
    for _ in 0..100 {
        native = wait("observe", mode).await?;
        model = fixture::snapshot(&state.peek());
        if guard(&state.peek(), &native)["channelAccepted"] == true
            && guard(&state.peek(), &native)["threadAccepted"] == true
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    if custody(&model) != *original || !context_enabled(&state.peek()) {
        return Err("Original raw bags/drafts/context changed".into());
    }
    let current = guard(&state.peek(), &native);
    if current["channelAccepted"] != true || current["threadAccepted"] != true {
        return Err("Current original DOM measurements did not settle".into());
    }
    let old = held.map(|n| guard(&state.peek(), n));
    if old
        .as_ref()
        .is_some_and(|v| v["channelAccepted"] != false || v["threadAccepted"] != false)
    {
        return Err("Old presentation measurement remained accepted".into());
    }
    let row = json!({"ok":true,"stage":name,"ordinal":ordinal,"noSession":false,"noTransport":false,"isolation":isolation(&state.peek()),"model":model,"native":native,"proof":proof,"measurementGuard":{"current":current,"old":old}});
    eprintln!("channel-thread-emoji-stage: {row}");
    ack(ordinal).await?;
    let after = wait("observe", mode).await?;
    if after["channelComposer"] != native["channelComposer"]
        || after["threadComposer"] != native["threadComposer"]
        || custody(&fixture::snapshot(&state.peek())) != *original
    {
        return Err("Original composer/raw custody changed during PNG capture".into());
    }
    Ok(row)
}
async fn run(mut state: Signal<ShellState>) -> Result<Value, String> {
    if !context_enabled(&state.peek()) {
        return Err("Exact disconnected fixture required".into());
    }
    let setup = wait("setup", "").await?;
    for _ in 0..100 {
        let expected = if std::env::var("SUPER_PLATINUM_FIXTURE")
            .unwrap()
            .ends_with("-narrow")
        {
            fixture::CHANNEL_DRAFT
        } else {
            "Channel draft caf\u{e9} \u{4e2d} edited"
        };
        if state.peek().core.composer.text == expected
            && state.peek().core.thread_composer.text == "Thread draft kept \u{1f642} edited"
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    let arm = wait("arm", "").await?;
    let original = custody(&fixture::snapshot(&state.peek()));
    let mut proof = json!({"setup":setup,"arm":arm});
    let mut stages = Vec::new();
    stages.push(
        stage(
            state,
            "unobserved",
            0,
            "unobserved",
            &proof,
            &original,
            None,
        )
        .await?,
    );
    let initial = stages[0]["native"].clone();
    proof["assignment1"] = fixture::assignments(state).await?;
    stages.push(
        stage(
            state,
            "assigned-pending",
            1,
            "pending",
            &proof,
            &original,
            Some(&initial),
        )
        .await?,
    );
    proof["media"] = fixture::media(&state.peek())?;
    stages.push(
        stage(
            state,
            "assigned-ready",
            2,
            "ready",
            &proof,
            &original,
            Some(&initial),
        )
        .await?,
    );
    let selected = stages[2]["native"].clone();
    proof["retirement"] = fixture::retire(&mut state.write())?;
    stages.push(
        stage(
            state,
            "socket-retired",
            3,
            "unobserved",
            &proof,
            &original,
            Some(&selected),
        )
        .await?,
    );
    let retired = stages[3]["native"].clone();
    proof["assignment2"] = fixture::assignments(state).await?;
    stages.push(
        stage(
            state,
            "fresh-assigned-ready",
            4,
            "ready",
            &proof,
            &original,
            Some(&retired),
        )
        .await?,
    );
    let layout = stages.last().unwrap()["native"].clone();
    Ok(
        json!({"ok":true,"noSession":false,"noTransport":false,"isolation":isolation(&state.peek()),"stages":stages,"layout":layout,"proof":proof,"model":fixture::snapshot(&state.peek())}),
    )
}
