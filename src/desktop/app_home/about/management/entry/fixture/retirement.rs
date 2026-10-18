//! Closed fixture-driven source refresh, followed by a real successor action.
//! This proves same-root / same-channel generation retirement, not item removal.
use super::*;
use crate::app_home::about::management::entry::fixture as entry_fixture;

const SUCCESSOR_CHANNEL: &str = "Successor channel draft \u{4e2d}\u{1f642}";
const SUCCESSOR_THREAD: &str = "Successor thread reply \u{4e2d}\u{1f642}";

fn request_count(value: &Value) -> usize {
    value["ledger"]["requests"].as_array().map_or(0, Vec::len)
}
fn documents(source: &Value) -> Result<&Value, String> {
    let value = &source["documents"];
    if !value.as_array().is_some_and(|rows| {
        rows.len() == 1
            && rows[0]["title"] == TITLE
            && rows[0]["description"] == DESCRIPTION
            && rows[0]["dirty"] == true
    }) {
        return Err("Original dirty workflow document was not retained".into());
    }
    Ok(value)
}
async fn stage(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    label: &str,
    exercise: Value,
    rows: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let hidden = matches!(label, "opened" | "held" | "reopened");
    wait_dom(desktop, &format!(r#"(()=>{{const root=document.getElementById('workflow-management');return{{ok:!!root==={hidden}&&(!root||!!root.__workflowManagement&&!root.__workflowManagement.lease&&!root.inert)}}}})()"#)).await?;
    let native = execution_surfaces::surface(desktop, label == "source", hidden).await?;
    capture(state, desktop, name, label,
        json!({"source":entry_fixture::snapshot(&state.peek()),"nativeSurface":native,"exercise":exercise}),
        rows, captures).await
}

async fn successor_witness(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    thread: bool,
) -> Result<Value, String> {
    wait_dom(
        desktop,
        "({ok:!document.getElementById('workflow-management')&&!document.body.inert})",
    )
    .await?;
    if thread {
        execution_surfaces::prepare_thread(state, desktop).await?;
    }
    // Source refresh legitimately retires the old native projection visit. Seed
    // a new witness only after its replacement controls are ready; never assert
    // that a retired composer must retain its old mount identity.
    let reset = wait_dom(desktop, r#"(()=>{const old=window.__executionSurfaceFixture;if(!old)return{ok:false};window.__executionRetiredSourceKey=old.key;delete window.__executionSurfaceFixture;return{ok:true,reset:true,previousActionKey:old.key}})()"#).await?;
    let params = json!({"seed":true,"hidden":false,"thread":thread,"focused":false,
        "channelDraft":SUCCESSOR_CHANNEL,"threadDraft":SUCCESSOR_THREAD,"label":entry_fixture::OPEN});
    let native = wait_dom(
        desktop,
        &format!("({})({params})", include_str!("surfaces.js")),
    )
    .await?;
    if native["actionKey"] == reset["previousActionKey"] {
        return Err("Refreshed source did not advance its native action key".into());
    }
    Ok(json!({"reset":reset,"nativeSurface":native,
        "channelDraft":if thread {Some(SUCCESSOR_CHANNEL)} else {None},
        "threadDraft":if thread {Some(SUCCESSOR_THREAD)} else {None}}))
}

pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let scenario = case();
    let thread = scenario == "execution-thread-retirement";
    if !thread && scenario != "execution-pins-retirement" {
        return Err("Wrong execution source-retirement case".into());
    }
    let readiness = readiness(name).await?;
    let mut stages = vec![];
    let mut captures = serde_json::Map::new();
    if thread {
        execution_surfaces::prepare_thread(state, desktop).await?;
    }
    stage(
        state,
        desktop,
        name,
        "source",
        Value::Null,
        &mut stages,
        &mut captures,
    )
    .await?;
    execution_surfaces::action(desktop).await?;
    execution::opened(state, desktop).await?;
    if request_count(&snapshot(&state.peek())) != 2 {
        return Err("Initial list/detail count changed".into());
    }
    stage(
        state,
        desktop,
        name,
        "opened",
        Value::Null,
        &mut stages,
        &mut captures,
    )
    .await?;

    click(desktop, "workflow-activity-back").await?;
    wait_dom(desktop,"({ok:!!document.getElementById('workflow-management-title')&&!document.getElementById('workflow-activity')})").await?;
    input(desktop, "workflow-management-title", TITLE).await?;
    input(desktop, "workflow-management-description", DESCRIPTION).await?;
    wait(state, "Source-retirement dirty document", exact).await?;
    click(desktop, "workflow-entry-back").await?;
    wait(state, "Return before source refresh", |v| {
        v["entry"]["active"] == false
    })
    .await?;
    let dirty = entry_fixture::snapshot(&state.peek());
    let original_documents = documents(&dirty)?.clone();
    stage(
        state,
        desktop,
        name,
        "dirty",
        Value::Null,
        &mut stages,
        &mut captures,
    )
    .await?;

    mode(&state.peek(), Mode::Held)?;
    execution_surfaces::action(desktop).await?;
    wait(state, "Held source-owned list", |v| {
        v["entry"]["active"] == true && v["ledger"]["held"] == 3 && request_count(v) == 3
    })
    .await?;
    let held_visit = snapshot(&state.peek())["entry"]["visit"].clone();
    stage(
        state,
        desktop,
        name,
        "held",
        json!({"heldVisit":held_visit,"heldSerial":3}),
        &mut stages,
        &mut captures,
    )
    .await?;
    let before_model = snapshot(&state.peek());
    let before_source = entry_fixture::snapshot(&state.peek());
    if documents(&before_source)? != &original_documents {
        return Err("Held entry replaced the dirty document".into());
    }
    {
        let mut shell = state.write();
        if thread {
            shell.begin_thread(entry_fixture::ROOT.into(), None);
            shell.thread_loading = false;
            shell.refresh_thread_from_core();
        } else {
            let items = shell.core.pins.items.values().cloned().collect();
            let generation = shell.core.pins.begin("C2");
            if !shell
                .core
                .pins
                .apply(generation, super_platinum_core::pins::PinsPage { items })
            {
                return Err("Fixture-driven Pins generation refresh refused".into());
            }
        }
    }
    wait(state, "Original source read cancelled", |v| {
        v["entry"]["active"] == false && v["ledger"]["cancelled"] == 1 && request_count(v) == 3
    })
    .await?;
    let released = release(&state.peek())?;
    if released["delivered"] != false || released["cancelledBeforeReply"] != true {
        return Err("Retired source received its old reply".into());
    }
    let successor = successor_witness(state, desktop, thread).await?;
    let after_model = snapshot(&state.peek());
    let after_source = entry_fixture::snapshot(&state.peek());
    let retained_activity = {
        let shell = state.peek();
        shell.app_home.about.management.documents.iter().map(|doc|json!({
            "document":doc.id,"page":doc.activity.page.as_ref().map(|p|p.raw()),
            "detail":doc.activity.detail.as_ref().map(|d|d.raw()),"detailRow":doc.activity.detail_row
        })).collect::<Vec<_>>()
    };
    if after_model["entry"]["active"] != false
        || !after_model["activity"].is_null()
        || request_count(&after_model) != 3
        || documents(&after_source)? != &original_documents
        || retained_activity.len() != 1
        || retained_activity
            .iter()
            .any(|d| !d["page"].is_null() || !d["detail"].is_null() || !d["detailRow"].is_null())
    {
        return Err("Retired response published an Activity view or changed draft".into());
    }
    let (before_generation, after_generation) = if thread {
        (
            &before_source["threadGeneration"],
            &after_source["threadGeneration"],
        )
    } else {
        (
            &before_source["pins"]["generation"],
            &after_source["pins"]["generation"],
        )
    };
    if before_generation.as_u64().and_then(|n| n.checked_add(1)) != after_generation.as_u64() {
        return Err("Source refresh did not advance exactly one generation".into());
    }
    stage(state,desktop,name,"retired",json!({"fixtureDrivenSourceRefresh":true,
        "kind":if thread{"same-root-thread-reopen"}else{"same-channel-pins-refresh"},
        "beforeSource":before_source,"afterSource":after_source,"beforeModel":before_model,"afterModel":after_model,
        "heldVisit":held_visit,"release":released,"retainedActivity":retained_activity,
        "successorWitnessReset":true,"successorWitness":successor}),
        &mut stages,&mut captures).await?;

    mode(&state.peek(), Mode::Success)?;
    execution_surfaces::action(desktop).await?;
    execution::opened(state, desktop).await?;
    wait(state, "Successor entry retains dirty document", exact).await?;
    let reopened = snapshot(&state.peek());
    let reopened_visit = reopened["entry"]["visit"].clone();
    if reopened_visit == held_visit
        || request_count(&reopened) != 5
        || reopened["ledger"]["cancelled"] != 1
        || documents(&entry_fixture::snapshot(&state.peek()))? != &original_documents
    {
        return Err("Successor entry identity, reads or draft did not match".into());
    }
    stage(
        state,
        desktop,
        name,
        "reopened",
        json!({"heldVisit":held_visit,"reopenedVisit":reopened_visit,"newVisit":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    Ok(
        json!({"ok":true,"fixture":name,"case":scenario,"readiness":readiness,
        "viewport":stages.last().unwrap()["geometry"]["viewport"],"actualMessageAction":true,
        "aboutConfiguration":false,"actualActivityEntry":true,"noLiveNetwork":true,
        "stages":stages,"captures":captures,"final":snapshot(&state.peek())}),
    )
}
