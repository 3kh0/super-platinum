//! Native shortcut -> production typed departure; no direct route mutation.
use super::*;

async fn shortcut(state: Signal<ShellState>, ordinal: u64) -> Result<Value, String> {
    if !state.peek().signed_in || state.peek().loading {
        return Err("Actual global shortcut is not enabled for this fixture".into());
    }
    let event = dom("palette-shortcut", json!({})).await?;
    if event["count"] != ordinal || event["trusted"] != false || event["prevented"] != true {
        return Err("Unexpected actual shortcut dispatch count".into());
    }
    Ok(event)
}
async fn editable(body: &str) -> Result<Value, String> {
    let n = wait_dom("read", json!({"editable":true})).await?;
    if n["title"] != TITLE || n["markdown"] != body {
        return Err(format!("Retained editable native pair differs: {n}"));
    }
    Ok(n)
}
async fn refusal(
    state: Signal<ShellState>,
    editor: &Arc<super::super::super::editor::Editor>,
    visit: &str,
    message: &str,
    body: &str,
) -> Result<Value, String> {
    wait_model(state, "failed departure retained original dialog", |s| {
        s.overlay == Some(Overlay::CanvasCreate)
            && s.canvas_create.as_ref().is_some_and(|d| {
                d.visit == visit
                    && !d.busy
                    && !d.close_requested
                    && d.pending_departure.is_none()
                    && d.error.as_deref() == Some(message)
                    && d.editor.as_ref().is_some_and(|e| Arc::ptr_eq(e, editor))
                    && editor.snapshot().capture.is_none()
            })
    })
    .await
    .map_err(|error| {
        let shell = state.peek();
        let model = shell.canvas_create.as_ref().map(|draft| {
            json!({
                "sameVisit": draft.visit == visit,
                "busy": draft.busy,
                "closeRequested": draft.close_requested,
                "pendingDeparture": draft.pending_departure.is_some(),
                "error": draft.error,
                "expectedError": message,
                "sameEditor": draft.editor.as_ref().is_some_and(|e| Arc::ptr_eq(e, editor)),
                "captureNone": editor.snapshot().capture.is_none(),
            })
        });
        format!(
            "{error}; model={model:?}; canvasOverlay={}",
            shell.overlay == Some(Overlay::CanvasCreate)
        )
    })?;
    let n = editable(body).await?;
    Ok(json!({"sameEditor":true,"pending":false,"closeRequested":false,"native":n,"error":message}))
}

pub(super) async fn run(
    state: Signal<ShellState>,
    b: &Binding,
    desktop: &Desktop,
    captures: &mut serde_json::Map<String, Value>,
    geometry: &mut Vec<Value>,
) -> Result<Value, String> {
    input(state, BODY).await?;
    dom("input", json!({"markdown":AHEAD,"dispatch":false})).await?;
    let (before_generation, first_visit) = {
        let shell = state.peek();
        let draft = shell.canvas_create.as_ref().unwrap();
        if draft.editor.as_ref().unwrap().snapshot().content.markdown() != BODY {
            return Err("Departure native-ahead was precommitted".into());
        }
        (
            shell.shortcut_generation,
            draft.native_visit.clone().unwrap(),
        )
    };
    let first = shortcut(state, 1).await?;
    wait_model(
        state,
        "one shortcut completed durable Palette departure",
        |s| {
            s.overlay == Some(Overlay::Palette)
                && s.canvas_create.is_none()
                && s.shortcut_generation == before_generation.wrapping_add(1)
        },
    )
    .await?;
    let palette = wait_dom("palette", json!({})).await?;
    let saved = b.inventory().await?;
    if saved["active"]["content"] != json!({"title":TITLE,"markdown":AHEAD}) {
        return Err("Palette opened before exact native capture was saved".into());
    }
    let after_generation = state.peek().shortcut_generation;
    stage(b, desktop, "palette", captures, geometry).await?;
    if state.peek().shortcut_generation != after_generation {
        return Err("Palette navigated more than once".into());
    }
    dom("leave-palette", json!({})).await?;
    wait_model(state, "actual Escape left Palette", |s| s.overlay.is_none()).await?;
    wait_dom("reopen-ready", json!({})).await?;
    dom("reopen", json!({})).await?;
    ready(state).await?;
    let returned = editable(AHEAD).await?;
    if returned["visit"] == first_visit || b.inventory().await? != saved {
        return Err("Reopen did not restore a new native visit from exact saved draft".into());
    }
    stage(b, desktop, "returned", captures, geometry).await?;

    let (editor, visit, refusal_generation) = {
        let shell = state.peek();
        let d = shell.canvas_create.as_ref().unwrap();
        (
            d.editor.clone().unwrap(),
            d.visit.clone(),
            shell.shortcut_generation,
        )
    };
    dom("composition-start", json!({"markdown":COMPOSED})).await?;
    let second = shortcut(state, 2).await?;
    let capture_message = "Finish composing text, then try again.";
    let mut capture = refusal(state, &editor, &visit, capture_message, COMPOSED).await?;
    let capture_inventory = b.inventory().await?;
    if capture_inventory != saved
        || state.peek().shortcut_generation != refusal_generation
        || editor.snapshot().content.markdown() != AHEAD
    {
        return Err("Composition refusal changed saved content or navigated".into());
    }
    capture["inventory"] = capture_inventory;
    capture["editorMarkdown"] = json!(editor.snapshot().content.markdown());
    stage(b, desktop, "capture-refused", captures, geometry).await?;
    dom("composition-end", json!({})).await?;
    wait_model(
        state,
        "explicit composition input saved without latent navigation",
        |s| {
            s.overlay == Some(Overlay::CanvasCreate)
                && s.canvas_create.as_ref().is_some_and(|d| {
                    d.visit == visit
                        && d.pending_departure.is_none()
                        && !d.close_requested
                        && d.editor.as_ref().is_some_and(|e| {
                            let snap = e.snapshot();
                            snap.saved && snap.content.markdown() == COMPOSED
                        })
                })
        },
    )
    .await?;
    editable(COMPOSED).await?;
    stage(b, desktop, "capture-resumed", captures, geometry).await?;

    // A genuine competing private Store write refuses the old editor CAS.
    // No test-only error flag and no fabricated expected controller result.
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
        .map_err(|_| "Competing isolated save failed")?;
    dom("input", json!({"markdown":OURS,"dispatch":false})).await?;
    let third = shortcut(state, 3).await?;
    let save_message = "Your text is retained. Save or review it before navigating.";
    let mut save = refusal(state, &editor, &visit, save_message, OURS).await?;
    let retained = b.inventory().await?;
    let snapshot = editor.snapshot();
    let slots = state
        .peek()
        .canvas_creation
        .delivery
        .tickets(&b.owner)
        .len();
    if !snapshot.failed
        || snapshot.content.markdown() != OURS
        || slots != 1
        || retained["active"]["content"] != json!({"title":TITLE,"markdown":FOREIGN})
        || retained["conflicts"].as_array().map(Vec::len) != Some(1)
        || retained["conflicts"][0]["content"] != json!({"title":TITLE,"markdown":OURS})
        || state.peek().shortcut_generation != refusal_generation
    {
        return Err("Save refusal lost exact root/durable conflict custody".into());
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    let stable = b.inventory().await?;
    let final_refusal_generation = state.peek().shortcut_generation;
    if stable != retained
        || state.peek().overlay != Some(Overlay::CanvasCreate)
        || state.peek().shortcut_generation != refusal_generation
    {
        return Err("Refused departure automatically retried or navigated".into());
    }
    save["inventory"] = retained;
    save["stableInventory"] = stable;
    save["retainedSlots"] = json!(slots);
    save["editorFailed"] = json!(true);
    stage(b, desktop, "save-refused", captures, geometry).await?;

    click("Review retained versions").await?;
    wait_model(state, "explicit complete departure conflict review", |s| {
        s.canvas_create
            .as_ref()
            .is_some_and(|d| d.review.is_some() && !d.busy)
    })
    .await?;
    dom("review", json!({"count":2})).await?;
    let reviewed = b.inventory().await?;
    click("Read current saved content").await?;
    let canonical = wait_dom("disclosure", json!({"kind":"canonical","markdown":FOREIGN})).await?;
    click("Read retained content").await?;
    let retained = wait_dom("disclosure", json!({"kind":"retained","markdown":OURS})).await?;
    stage(b, desktop, "review", captures, geometry).await?;
    dom("choose-retained", json!({})).await?;
    click("Keep selected version").await?;
    wait_model(state, "departure selected version reloaded", |s| {
        s.canvas_create
            .as_ref()
            .is_some_and(|d| !d.loading && d.review.is_none() && d.markdown == OURS)
    })
    .await?;
    ready(state).await?;
    let native = editable(OURS).await?;
    stage(b, desktop, "chosen", captures, geometry).await?;
    if !b.ledger().is_empty() {
        return Err("Navigation fixture prepared a Slack write".into());
    }
    Ok(
        json!({"oneShortcutDeparture":true,"oneNavigation":true,"nativeAheadCaptured":true,
        "returnExactSaved":true,"captureRefusalRetained":true,"saveRefusalRetained":true,
        "intentCanceled":true,"noAutoRetry":true,"explicitChoice":true,"noDefaultChoice":true,
        "shortcuts":[first,second,third],"beforeGeneration":before_generation,"afterGeneration":after_generation,
        "refusalGeneration":refusal_generation,"finalRefusalGeneration":final_refusal_generation,
        "firstVisit":first_visit,"returned":returned,"savedInventory":saved,"palette":palette,
        "captureRefusal":capture,"saveRefusal":save,"reviewedInventory":reviewed,
        "reviewedNative":{"canonical":canonical,"retained":retained},"native":native}),
    )
}
