//! Actual native fields/buttons and the production controller, with isolated I/O.
use super::*;
use std::time::Duration;
type Desktop = dioxus::desktop::DesktopContext;
mod departure;

pub(crate) fn start(state: Signal<ShellState>, desktop: Desktop) {
    if !enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = tokio::time::timeout(Duration::from_secs(110), run(state, &desktop)).await;
        let receipt = match result {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => json!({"ok":false,"error":e}),
            Err(_) => json!({"ok":false,"error":"Canvas fixture timed out"}),
        };
        eprintln!("canvas-creation-check: {receipt}");
    });
}
async fn dom(operation: &str, args: Value) -> Result<Value, String> {
    let script = format!(
        "{}\nlet receipt;try{{receipt=window.canvasCreationCheck({},{});}}catch(e){{receipt={{ok:false,reason:String(e)}};}}dioxus.send(receipt);await dioxus.recv();",
        include_str!("check.js"),
        json!(operation),
        args
    );
    let mut eval = document::eval(&script);
    // Keep the JS query alive until Rust has consumed its one receipt. Release it
    // on timeout/error too; a missing receipt never authorizes replaying an action.
    let received = tokio::time::timeout(Duration::from_secs(3), eval.recv::<Value>()).await;
    let released = eval.send(());
    let value = received
        .map_err(|_| "Native receipt timed out")?
        .map_err(|_| "Native receipt missing")?;
    if value["ok"] != true {
        return Err(format!("Native {operation}: {value}"));
    }
    released.map_err(|_| "Native receipt acknowledgment failed")?;
    Ok(value)
}
async fn wait_dom(operation: &str, args: Value) -> Result<Value, String> {
    if !matches!(
        operation,
        "read" | "palette" | "disclosure" | "reopen-ready"
    ) {
        return Err(format!("Native action must not be polled: {operation}"));
    }
    let mut last = String::new();
    for _ in 0..150 {
        match dom(operation, args.clone()).await {
            Ok(v) => return Ok(v),
            Err(e) => last = e,
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(last)
}
async fn wait_model(
    state: Signal<ShellState>,
    label: &str,
    test: impl Fn(&ShellState) -> bool,
) -> Result<(), String> {
    for _ in 0..400 {
        if test(&state.peek()) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let native = dom("read", json!({}))
        .await
        .unwrap_or_else(|e| json!({"error":e}));
    Err(format!("Timed out waiting for {label}: {native}"))
}
async fn ready(state: Signal<ShellState>) -> Result<(), String> {
    wait_model(state, "mounted real editor", |s| {
        s.canvas_create
            .as_ref()
            .is_some_and(|d| !d.loading && d.native_visit.is_some() && d.editor.is_some())
    })
    .await?;
    let visit = state
        .peek()
        .canvas_create
        .as_ref()
        .unwrap()
        .native_visit
        .clone()
        .unwrap();
    for _ in 0..150 {
        if dom("read", json!({}))
            .await
            .is_ok_and(|n| n["visit"] == visit)
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Native mounted identity did not match admitted visit".into())
}
async fn absent() -> Result<(), String> {
    for _ in 0..150 {
        if dom("absent", json!({})).await.is_ok() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Actual previous editor did not unmount".into())
}
async fn open(state: Signal<ShellState>) -> Result<(), String> {
    super::super::controller::open(state);
    ready(state).await
}
async fn click(label: &str) -> Result<(), String> {
    dom("click", json!({"label":label})).await.map(|_| ())
}
async fn close(state: Signal<ShellState>) -> Result<(), String> {
    click("Close header").await?;
    wait_model(state, "native Close", |s| {
        s.overlay != Some(Overlay::CanvasCreate)
    })
    .await?;
    absent().await
}
async fn input(state: Signal<ShellState>, body: &str) -> Result<(), String> {
    dom("input", json!({"title":TITLE,"markdown":body})).await?;
    wait_model(state, "exact accepted input saved", |s| {
        s.canvas_create
            .as_ref()
            .and_then(|d| d.editor.as_ref())
            .is_some_and(|e| {
                let r = e.snapshot();
                r.saved && r.content.title() == TITLE && r.content.markdown() == body
            })
    })
    .await
}
async fn exact(body: &str) -> Result<Value, String> {
    let n = dom("read", json!({})).await?;
    if n["title"] != TITLE || n["markdown"] != body {
        return Err(format!("Exact native pair differs: {n}"));
    }
    Ok(n)
}
async fn stage(
    b: &Binding,
    d: &Desktop,
    name: &str,
    captures: &mut serde_json::Map<String, Value>,
    geometry: &mut Vec<Value>,
) -> Result<(), String> {
    if name != "closed" {
        geometry.push(
            dom(
                if name == "palette" {
                    "palette"
                } else {
                    "geometry"
                },
                json!({}),
            )
            .await?,
        );
    }
    captures.insert(name.into(), harness::capture(b, d, name).await?);
    Ok(())
}
async fn editing(
    state: Signal<ShellState>,
    b: &Binding,
    d: &Desktop,
    c: &mut serde_json::Map<String, Value>,
    g: &mut Vec<Value>,
) -> Result<Value, String> {
    input(state, BODY).await?;
    stage(b, d, "edited", c, g).await?;
    dom("input", json!({"markdown":AHEAD,"dispatch":false})).await?;
    if state
        .peek()
        .canvas_create
        .as_ref()
        .unwrap()
        .editor
        .as_ref()
        .unwrap()
        .snapshot()
        .content
        .markdown()
        != BODY
    {
        return Err("Native-ahead test was precommitted".into());
    }
    close(state).await?;
    open(state).await?;
    exact(AHEAD).await?;
    stage(b, d, "native-ahead", c, g).await?;
    dom("composition-start", json!({"markdown":COMPOSED})).await?;
    click("Close header").await?;
    wait_model(state, "composition refusal", |s| {
        s.canvas_create.as_ref().is_some_and(|d| {
            !d.busy
                && d.error
                    .as_deref()
                    .is_some_and(|e| e.contains("Finish composing"))
        })
    })
    .await?;
    if b.inventory().await?["active"]["content"]["markdown"] != AHEAD {
        return Err("Provisional composition changed durable draft".into());
    }
    exact(COMPOSED).await?;
    stage(b, d, "composing", c, g).await?;
    dom("composition-end", json!({})).await?;
    wait_model(state, "composition committed input", |s| {
        s.canvas_create
            .as_ref()
            .and_then(|d| d.editor.as_ref())
            .is_some_and(|e| {
                let v = e.snapshot();
                v.saved && v.content.markdown() == COMPOSED
            })
    })
    .await?;
    close(state).await?;
    open(state).await?;
    let n = exact(COMPOSED).await?;
    stage(b, d, "reopened", c, g).await?;
    Ok(
        json!({"autosave":true,"nativeAheadCaptured":true,"compositionDeferred":true,"closeReopen":true,"native":n}),
    )
}
async fn known_unsent(
    state: Signal<ShellState>,
    b: &Binding,
    d: &Desktop,
    c: &mut serde_json::Map<String, Value>,
    g: &mut Vec<Value>,
) -> Result<Value, String> {
    input(state, BODY).await?;
    let before = b.inventory().await?;
    b.set_mode(1)?;
    click("Create canvas").await?;
    wait_model(state, "known-unsent exact saved restoration", |s| {
        b.sources.load(Ordering::Acquire) == 1
            && s.canvas_create
                .as_ref()
                .is_some_and(|d| !d.busy && d.saved && d.attempt.is_none())
    })
    .await?;
    if b.inventory().await? != before || b.ledger().len() != 1 || b.ledger()[0]["executed"] != false
    {
        return Err("Known-unsent changed inventory or dispatched".into());
    }
    stage(b, d, "unsent", c, g).await?;
    let editor = state
        .peek()
        .canvas_create
        .as_ref()
        .unwrap()
        .editor
        .clone()
        .unwrap();
    let base = editor.snapshot().base;
    b.storage()
        .run(move |store| {
            store.save(
                &base,
                &uuid::Uuid::new_v4().to_string(),
                domain::Content::new(TITLE.into(), FOREIGN.into())?,
            )
        })
        .await
        .map_err(|_| "Competing synthetic writer failed")?;
    dom("input", json!({"markdown":OURS})).await?;
    wait_model(state, "retained conflicting input", |s| {
        s.canvas_create
            .as_ref()
            .and_then(|d| d.editor.as_ref())
            .is_some_and(|e| e.snapshot().failed)
    })
    .await?;
    stage(b, d, "conflict", c, g).await?;
    click("Review retained versions").await?;
    wait_model(state, "complete explicit review", |s| {
        s.canvas_create
            .as_ref()
            .is_some_and(|d| d.review.is_some() && !d.busy)
    })
    .await?;
    dom("review", json!({"count":2})).await?;
    let reviewed = b.inventory().await?;
    if reviewed["conflicts"].as_array().map(Vec::len) != Some(1) {
        return Err("Wrong complete conflict inventory".into());
    }
    click("Read current saved content").await?;
    let canonical = wait_dom("disclosure", json!({"kind":"canonical","markdown":FOREIGN})).await?;
    click("Read retained content").await?;
    let retained = wait_dom("disclosure", json!({"kind":"retained","markdown":OURS})).await?;
    stage(b, d, "review", c, g).await?;
    dom("choose-retained", json!({})).await?;
    click("Keep selected version").await?;
    wait_model(state, "chosen version reload", |s| {
        s.canvas_create
            .as_ref()
            .is_some_and(|d| !d.loading && d.review.is_none() && d.markdown == OURS)
    })
    .await?;
    ready(state).await?;
    let n = exact(OURS).await?;
    if !b.inventory().await?["conflicts"]
        .as_array()
        .is_some_and(Vec::is_empty)
    {
        return Err("Reviewed conflicts were not retired".into());
    }
    stage(b, d, "chosen", c, g).await?;
    Ok(
        json!({"knownUnsentExactBase":true,"noDispatchOnRefusal":true,"conflictPreserved":true,"noDefaultChoice":true,
        "explicitChoice":true,"reviewedInventory":reviewed,"reviewedNative":{"canonical":canonical,"retained":retained},"native":n}),
    )
}
async fn late_result(
    state: Signal<ShellState>,
    b: &Binding,
    d: &Desktop,
    c: &mut serde_json::Map<String, Value>,
    g: &mut Vec<Value>,
) -> Result<Value, String> {
    input(state, BODY).await?;
    b.set_mode(2)?;
    click("Create canvas").await?;
    wait_model(state, "first actual prepared request", |_| {
        b.ledger().len() == 1 && b.ledger()[0]["executed"] == true
    })
    .await?;
    stage(b, d, "pending", c, g).await?;
    close(state).await?;
    stage(b, d, "closed", c, g).await?;
    b.release(Ok(json!({"ok":true,"file_id":FILE_ID})))?;
    wait_model(state, "late result stored without navigation", |s| {
        s.canvas_creation.delivery.tickets(&b.owner).is_empty()
    })
    .await?;
    if state.peek().overlay.is_some() {
        return Err("Late result stole navigation".into());
    }
    open(state).await?;
    if state
        .peek()
        .canvas_create
        .as_ref()
        .unwrap()
        .created
        .as_deref()
        != Some(FILE_ID)
    {
        return Err("Returned ID not restored".into());
    }
    stage(b, d, "created", c, g).await?;
    click("Start a new canvas").await?;
    wait_model(state, "explicit new draft", |s| {
        s.canvas_create
            .as_ref()
            .is_some_and(|d| !d.loading && d.created.is_none() && d.title.is_empty())
    })
    .await?;
    ready(state).await?;
    input(state, AHEAD).await?;
    stage(b, d, "archived", c, g).await?;
    b.set_mode(2)?;
    click("Create canvas").await?;
    wait_model(state, "second actual request", |_| {
        b.ledger().len() == 2 && b.ledger()[1]["executed"] == true
    })
    .await?;
    b.release(Err(Error::Offline(
        "synthetic post-handoff uncertainty".into(),
    )))?;
    wait_model(state, "uncertain receipt", |s| {
        s.canvas_create
            .as_ref()
            .is_some_and(|d| d.uncertain && !d.busy)
    })
    .await?;
    stage(b, d, "uncertain", c, g).await?;
    close(state).await?;
    open(state).await?;
    if !state.peek().canvas_create.as_ref().unwrap().uncertain || b.ledger().len() != 2 {
        return Err("Uncertain close/reopen replayed or unlocked".into());
    }
    let n = exact(AHEAD).await?;
    stage(b, d, "reopened", c, g).await?;
    Ok(
        json!({"heldResponse":true,"lateCreatedRetained":true,"noNavigationSteal":true,"archiveOrthogonal":true,
        "uncertainNoReplay":true,"native":n}),
    )
}
fn model(state: Signal<ShellState>, scenario: &str) -> Result<Value, String> {
    let shell = state.peek();
    let d = shell.canvas_create.as_ref().ok_or("Missing final model")?;
    let locked = scenario == "late-result";
    let delivery = d.attempt.as_ref().map(|a| a.delivery().clone());
    if d.uncertain != locked
        || d.editable() == locked
        || d.busy
        || d.loading
        || (locked && !matches!(delivery, Some(domain::Delivery::Uncertain)))
        || (!locked && delivery.is_some())
    {
        return Err("Final model lost exact uncertain/editable policy".into());
    }
    Ok(json!({"uncertain":d.uncertain,"editable":d.editable(),"delivery":delivery}))
}
async fn run(mut state: Signal<ShellState>, desktop: &Desktop) -> Result<Value, String> {
    let b = harness::prepare(state).await?;
    {
        let mut s = state.write();
        s.canvas_create = None;
        s.overlay = None;
    }
    absent().await?;
    open(state).await?;
    let mut captures = serde_json::Map::new();
    let mut geometry = vec![];
    let result = if b.phase == "restart" {
        let expected = match b.scenario.as_str() {
            "editing" => COMPOSED,
            "known-unsent-conflict" | "departure" => OURS,
            "late-result" => AHEAD,
            _ => return Err("Unknown case".into()),
        };
        let n = exact(expected).await?;
        model(state, &b.scenario)?;
        stage(&b, desktop, "restart", &mut captures, &mut geometry).await?;
        if !b.ledger().is_empty() {
            return Err("Restart dispatched a request".into());
        }
        json!({"restoredFromStore":true,"noReplay":true,"native":n})
    } else {
        stage(&b, desktop, "initial", &mut captures, &mut geometry).await?;
        match b.scenario.as_str() {
            "editing" => editing(state, &b, desktop, &mut captures, &mut geometry).await?,
            "departure" => departure::run(state, &b, desktop, &mut captures, &mut geometry).await?,
            "known-unsent-conflict" => {
                known_unsent(state, &b, desktop, &mut captures, &mut geometry).await?
            }
            "late-result" => late_result(state, &b, desktop, &mut captures, &mut geometry).await?,
            _ => return Err("Unknown case".into()),
        }
    };
    let inventory = b.inventory().await?;
    let model = model(state, &b.scenario)?;
    if !b.current(&state.peek()) {
        return Err("Final synthetic owner changed".into());
    }
    Ok(
        json!({"ok":true,"fixture":b.fixture,"scenario":b.scenario,"phase":b.phase,"nonce":b.nonce,"pid":std::process::id(),
        "isolatedDatabase":b.path,"noSession":true,"noTransport":true,"noLiveNetwork":true,"syntheticInput":true,
        "syntheticIME":matches!(b.scenario.as_str(),"editing"|"departure")&&b.phase=="exercise","viewport":dom("viewport",json!({})).await?,
        "model":model,"captures":captures,"geometry":geometry,"preparedRequests":b.ledger().len(),"executedRequests":b.ledger().iter().filter(|r|r["executed"]==true).count(),"requests":b.ledger(),"result":result,"inventory":inventory}),
    )
}
