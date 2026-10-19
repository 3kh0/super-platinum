//! Closed selected-root replacement / original-pin removal, with old callback refusal.
use super::*;
use crate::app_home::about::management::entry::fixture as entry_fixture;
use crate::message_buttons::source::{Bag, Source};
use super_platinum_core::block_actions::{BlockElementMetadata, BlockElementSource};

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
    let native = surface(desktop, label == "source", hidden).await?;
    capture(
        state,
        desktop,
        name,
        label,
        json!({"source":source_snapshot(&state.peek()),"nativeSurface":native,"exercise":exercise}),
        rows,
        captures,
    )
    .await
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
        prepare_thread(state, desktop, true).await?;
    }
    // Source refresh legitimately retires the old native projection visit. Seed
    // a new witness only after its replacement controls are ready; never assert
    // that a retired composer must retain its old mount identity.
    let reset = wait_dom(desktop, r#"(()=>{const old=window.__executionSurfaceFixture;if(!old)return{ok:false};window.__executionRetiredSourceKey=old.key;delete window.__executionSurfaceFixture;window.__executionReplacementSuccessor=true;return{ok:true,reset:true,previousActionKey:old.key}})()"#).await?;
    let params = json!({"seed":true,"hidden":false,"thread":thread,"focused":false,
        "channelDraft":SUCCESSOR_CHANNEL,"threadDraft":SUCCESSOR_THREAD,"label":entry_fixture::SUCCESSOR_OPEN});
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

fn source_snapshot(s: &ShellState) -> Value {
    let mut value = entry_fixture::snapshot(s);
    value["originalPinVersion"] = json!(
        s.core
            .pins
            .version("C2", &format!("message:{}", entry_fixture::TS))
    );
    value["selectedThreadSource"] = s
        .thread_root
        .as_ref()
        .and_then(|root| {
            s.core
                .threads
                .get(&("TWORKFLOW".into(), "C2".into(), root.clone()))
        })
        .map(|bag| json!(bag.messages))
        .unwrap_or(Value::Null);
    value["successorThreadSource"] = s
        .core
        .threads
        .get(&(
            "TWORKFLOW".into(),
            "C2".into(),
            entry_fixture::SUCCESSOR_ROOT.into(),
        ))
        .map(|bag| json!(bag.messages))
        .unwrap_or(Value::Null);
    value["documentRevisions"] = json!(
        s.app_home
            .about
            .management
            .documents
            .iter()
            .map(|d| json!({"id":d.id,"revision":d.revision}))
            .collect::<Vec<_>>()
    );
    value
}
fn capture_original(
    s: &ShellState,
    thread: bool,
) -> Result<(Source, BlockElementMetadata), String> {
    let key = format!("message:{}", entry_fixture::TS);
    let message = if thread {
        s.core
            .threads
            .get(&("TWORKFLOW".into(), "C2".into(), entry_fixture::ROOT.into()))
            .and_then(|bag| {
                bag.messages
                    .iter()
                    .find(|m| m.ts.as_deref() == Some(entry_fixture::TS))
            })
    } else {
        s.core.pins.items.get(&key).and_then(|p| p.message.as_ref())
    }
    .ok_or("Original source message missing")?;
    let raw = message
        .blocks
        .iter()
        .find(|b| b["block_id"] == "execution-open")
        .and_then(|b| b["elements"].as_array())
        .and_then(|r| r.first())
        .ok_or("Original action missing")?;
    let metadata =
        crate::blocks::action::metadata(raw, Some("execution-open"), BlockElementSource::Button);
    let row = crate::message_vm::message_vm(&s.core.workspaces["TWORKFLOW"], message, &s.media);
    let bag = if thread {
        Bag::Thread {
            root: entry_fixture::ROOT.into(),
        }
    } else {
        Bag::Pins { item_key: key }
    };
    let source =
        Source::capture(s, &row, "C2", bag).map_err(|_| "Original action source refused")?;
    Ok((source, metadata))
}
fn replace_source(s: &mut ShellState, thread: bool) -> Result<Value, String> {
    if thread {
        let prior = s.thread_root.clone();
        s.begin_thread(entry_fixture::SUCCESSOR_ROOT.into(), None);
        s.thread_loading = false;
        s.refresh_thread_from_core();
        Ok(json!({"kind":"begin-thread","previousRoot":prior,"nextRoot":s.thread_root}))
    } else {
        let raw:super_platinum_core::slack::events::RawEvent=serde_json::from_value(json!({"type":"pin_removed","channel_id":"C2",
            "item":{"type":"message","channel":"C2","ts":entry_fixture::TS},"event_ts":"1781524999.000001"})).map_err(|e|e.to_string())?;
        let update = super_platinum_core::pins::PinUpdate::from_event(&raw)
            .ok_or("Pin removal decode refused")?;
        if !s.core.apply_pin_event("TWORKFLOW", &update) {
            return Err("Pin removal reducer refused".into());
        }
        s.core
            .update_pin_copies("TWORKFLOW", "C2", entry_fixture::TS, false);
        Ok(
            json!({"kind":"pin-removed-event","channel":"C2","messageTs":entry_fixture::TS,"eventTs":update.event_ts,"applied":true}),
        )
    }
}
fn stale_callback(
    state: Signal<ShellState>,
    source: Source,
    metadata: BlockElementMetadata,
) -> Result<Value, String> {
    let (before_model, before_source, source_current, enabled) = {
        let s = state.peek();
        (
            snapshot(&s),
            source_snapshot(&s),
            source.client_message(&s).is_ok(),
            crate::message_buttons::presentation_for_button(&s, &source, &metadata).enabled,
        )
    };
    if source_current || enabled {
        return Err("Retired source still authorizes action".into());
    }
    crate::message_buttons::click(state, source, metadata);
    let after_model = snapshot(&state.peek());
    let after_source = source_snapshot(&state.peek());
    if before_model != after_model || before_source != after_source {
        return Err("Stale callback changed owned state".into());
    }
    Ok(
        json!({"sourceCurrent":source_current,"presentationEnabled":enabled,"unchanged":true,
        "requestsBefore":request_count(&before_model),"requestsAfter":request_count(&after_model),
        "beforeModel":before_model,"afterModel":after_model,"beforeSource":before_source,"afterSource":after_source}),
    )
}
async fn prepare_thread(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    successor: bool,
) -> Result<(), String> {
    let committed = format!(
        "{}.",
        if successor {
            SUCCESSOR_THREAD
        } else {
            entry_fixture::THREAD_COMPOSER
        }
    );
    input(desktop, "thread-composer", &committed).await?;
    for _ in 0..100 {
        if state.peek().core.thread_composer.text == committed {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err("Native thread draft input did not commit".into())
}
async fn surface(desktop: &DesktopContext, seed: bool, hidden: bool) -> Result<Value, String> {
    let successor = wait_dom(
        desktop,
        "({ok:true,successor:!!window.__executionReplacementSuccessor})",
    )
    .await?["successor"]
        == true;
    let params = json!({"seed":seed,"hidden":hidden,"thread":case()=="execution-thread-replacement","focused":false,
        "channelDraft":entry_fixture::COMPOSER,"threadDraft":entry_fixture::THREAD_COMPOSER,
        "label":if successor{entry_fixture::SUCCESSOR_OPEN}else{entry_fixture::OPEN}});
    wait_dom(
        desktop,
        &format!("({})({params})", include_str!("surfaces.js")),
    )
    .await
}

pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let scenario = case();
    let thread = scenario == "execution-thread-replacement";
    if !thread && scenario != "execution-pin-removal" {
        return Err("Wrong execution source-replacement case".into());
    }
    let readiness = readiness(name).await?;
    let (original_source, original_metadata) = capture_original(&state.peek(), thread)?;
    let mut stages = vec![];
    let mut captures = serde_json::Map::new();
    if thread {
        prepare_thread(state, desktop, false).await?;
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
    let dirty = source_snapshot(&state.peek());
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
    let before_source = source_snapshot(&state.peek());
    if documents(&before_source)? != &original_documents {
        return Err("Held entry replaced the dirty document".into());
    }
    let mutation = replace_source(&mut state.write(), thread)?;
    wait(state, "Original source read cancelled", |v| {
        v["entry"]["active"] == false && v["ledger"]["cancelled"] == 1 && request_count(v) == 3
    })
    .await?;
    let released = release(&state.peek())?;
    if released["delivered"] != false || released["cancelledBeforeReply"] != true {
        return Err("Retired source received its old reply".into());
    }
    let absent = wait_dom(desktop, r#"(()=>{const old=window.__executionSurfaceFixture;if(!old)return{ok:false};const count=[...document.querySelectorAll('[data-workflow-logs-source]')].filter(e=>e.dataset.workflowLogsSource===old.key).length;return{ok:count===0,originalActionKey:old.key,originalActionCount:count,originalActionConnected:old.action.isConnected}})()"#).await?;
    let stale = stale_callback(state, original_source, original_metadata)?;
    let successor = successor_witness(state, desktop, thread).await?;
    let after_model = snapshot(&state.peek());
    let after_source = source_snapshot(&state.peek());
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
    stage(state,desktop,name,"retired",json!({"fixtureDrivenSourceReplacement":true,
        "kind":if thread{"thread-root-replacement"}else{"originating-pin-removal"},
        "beforeSource":before_source,"afterSource":after_source,"beforeModel":before_model,"afterModel":after_model,
        "heldVisit":held_visit,"release":released,"retainedActivity":retained_activity,
        "mutation":mutation,"originalActionAbsent":absent,"staleCallback":stale,
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
        || documents(&source_snapshot(&state.peek()))? != &original_documents
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
