//! Actual startup producer and first offline Share, with no observation injection.
use super::*;
use backend::startup as source;

fn acquisition(s: &ShellState) -> Value {
    crate::app_home::autocomplete::observations::snapshot(s)
}
fn connection(s: &ShellState) -> Value {
    json!({"routable":s.connection.routable,"routeObserved":s.connection.routed_at.is_some(),
        "realtimeConnected":s.core.workspaces["TWORKFLOW"].rt.is_connected(),
        "transportHealth":s.core.transport.as_ref().map(|t|format!("{:?}",t.health()).to_lowercase())})
}
pub(super) fn native(base: &str, mode: &str) -> String {
    if !source::requested() || source::case() != "socket-retired" {
        return base.into();
    }
    include_str!("startup-retired.js")
        .replace("MODE", &json!(mode).to_string())
        .replace("BASE", base)
}
fn retire(mut state: Signal<ShellState>, scenario: &str) -> Result<Value, String> {
    let mut s = state.write();
    if !source::enabled(&s) {
        return Err("Original startup fixture absent".into());
    }
    let generation = s.core.workspaces["TWORKFLOW"].rt_generation;
    let before = acquisition(&s);
    let connection_before = connection(&s);
    if connection_before["routable"] != true
        || connection_before["realtimeConnected"] != true
        || connection_before["transportHealth"] == "offline"
    {
        return Err("Retirement requires original still-online source".into());
    }
    let frame = if scenario == "prefs-retired" {
        let raw = json!({"type":"pref_change","name":"display_real_names_override","value":1});
        let event = super_platinum_core::slack::realtime::parse_event(&raw.to_string())
            .ok_or("Original preference event did not parse")?;
        if !matches!(
            event,
            super_platinum_core::slack::events::RtEvent::PreferencesChanged { .. }
        ) {
            return Err("Preference event changed type".into());
        }
        crate::app_home::realtime(&mut s, "TWORKFLOW", generation, &event);
        crate::realtime::apply_in_view(&mut s.core, "TWORKFLOW", generation, event, false);
        raw
    } else {
        let w = s
            .core
            .workspaces
            .get_mut("TWORKFLOW")
            .ok_or("Original workspace absent")?;
        w.rt_generation = w.rt_generation.wrapping_add(1);
        json!({"kind":"closed-fixture-socket-generation-retirement","before":generation,"after":w.rt_generation,"networkCalls":0})
    };
    let after = acquisition(&s);
    Ok(
        json!({"scenario":scenario,"generation":generation,"frame":frame,"before":before,"after":after,"connectionBefore":connection_before,"connectionAfter":connection(&s),"storeInjection":false}),
    )
}

pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    visit: u64,
    captures: &mut serde_json::Map<String, Value>,
    stages: &mut Vec<Value>,
) -> Result<Value, String> {
    let scenario = source::case();
    let held = matches!(
        scenario.as_str(),
        "success" | "prefs-retired" | "socket-retired"
    );
    until(state, visit, "original pre-Share acquisition", |p, s| {
        let d = &s["confirmationAdd"]["share"]["startup"];
        value(p)["active"] != true
            && if held {
                d["held"] == json!([2])
            } else {
                d["requests"].as_array().is_some_and(|r| r.len() == 2)
                    && d["settlements"].as_array().is_some_and(|r| r.len() == 2)
            }
    })
    .await?;
    let before = observe(state, desktop, visit).await?;
    if held {
        crate::app_home::autocomplete::startup::schedule(state, "TWORKFLOW");
        crate::app_home::autocomplete::startup::schedule(state, "TWORKFLOW");
    }
    let stable = quiet(state, desktop, visit).await?;
    if stable["before"] != stable["after"] {
        return Err("Repeated startup scheduling changed original held acquisition".into());
    }
    let binding = if scenario == "socket-retired" {
        let base = super::super::super::super::native_script("observe");
        evaluate(desktop, &native(&base, "arm")).await?
    } else {
        Value::Null
    };
    let after = observe(state, desktop, visit).await?;
    let original_attempt = last(&after["firstPublish"]).clone();
    let mut exercise =
        json!({"before":before,"after":after,"scheduleCalls":if held{2}else{0},"quiet":stable});
    if !binding.is_null() {
        exercise["retirementBinding"] = binding;
    }
    stage(
        state,
        desktop,
        name,
        "startup-before",
        visit,
        exercise,
        captures,
        stages,
    )
    .await?;
    let model = backend::capture(&state.peek())?;
    let mut release = Value::Null;
    let mut retirement = Value::Null;
    let transition;
    if scenario == "success" {
        release = source::release(&model, 2)?;
        until(
            state,
            visit,
            "complete original startup producer",
            |_, s| {
                let d = &s["confirmationAdd"]["share"]["startup"];
                d["settlements"].as_array().is_some_and(|r| r.len() == 3)
                    && acquisition(&state.peek())["currentComplete"] == true
            },
        )
        .await?;
        transition = backend::connectivity::offline(&mut state.write())?;
    } else {
        if held {
            retirement = retire(state, &scenario)?;
            release = source::release(&model, 2)?;
        }
        until(state, visit, "original startup cannot publish", |_, s| {
            let d = &s["confirmationAdd"]["share"]["startup"];
            let a = acquisition(&state.peek());
            d["settlements"].as_array().is_some_and(|r| r.len() == 2)
                && a["pending"] == false
                && a["currentComplete"] == false
        })
        .await?;
        if held {
            let s = state.peek();
            let key = if scenario == "socket-retired" {
                "settledOnline"
            } else {
                "settledBeforeOffline"
            };
            retirement[key] = json!({"observations":acquisition(&s),"connection":connection(&s)});
        }
        if scenario == "socket-retired" {
            until(state, visit, "original management lease retired", |p, _| {
                p["visible"] == false
                    && last(p)["current"] == false
                    && last(p)["confirmationShare"].is_null()
                    && [
                        "id",
                        "visit",
                        "document",
                        "revision",
                        "publication",
                        "metadata",
                        "readback",
                        "complete",
                        "finished",
                        "closed",
                    ]
                    .iter()
                    .all(|key| last(p)[*key] == original_attempt[*key])
            })
            .await?;
            let current = observe(state, desktop, visit).await?;
            let s = state.peek();
            if connection(&s) != retirement["connectionBefore"]
                || acquisition(&s)["currentComplete"] != false
            {
                return Err("Socket retirement was masked by link loss or late publication".into());
            }
            drop(s);
            stage(state,desktop,name,"startup-retired",visit,
                json!({"after":current,"transition":null,"release":release,"retirement":retirement,"quiet":quiet(state,desktop,visit).await?}),captures,stages).await?;
            return Ok(Value::Null);
        }
        transition = backend::connectivity::offline(&mut state.write())?;
    }
    let current = observe(state, desktop, visit).await?;
    stage(state,desktop,name,"startup-offline",visit,json!({"after":current,"transition":transition,"release":release,"retirement":retirement,"quiet":quiet(state,desktop,visit).await?}),captures,stages).await?;
    let activation = action(desktop, "click", "workflow-confirmation-share-open", "").await?;
    // Offline cache capture deliberately starts no model/offline request, so
    // use the same original DOM/model predicate without online cache readiness.
    let mut current = ready(state, desktop, visit).await?;
    stage(
        state,
        desktop,
        name,
        "share-open",
        visit,
        json!({"activation":activation,"after":current}),
        captures,
        stages,
    )
    .await?;
    let activation = action(desktop, "text", "workflow-share-message", backend::MESSAGE).await?;
    until(state, visit, "original startup optional text", |p, _| {
        value(p)["text"] == backend::MESSAGE && value(p)["bound"] == true
    })
    .await?;
    current = ready(state, desktop, visit).await?;
    stage(
        state,
        desktop,
        name,
        "text-retained",
        visit,
        json!({"activation":activation,"after":current}),
        captures,
        stages,
    )
    .await?;
    let activation = input(desktop, "Fixture").await?;
    until(state, visit, "first offline typed query", |p, _| {
        value(p)["search"]["query"] == "Fixture"
            && value(p)["search"]["status"] != "searching"
            && value(p)["bound"] == true
    })
    .await?;
    current = ready(state, desktop, visit).await?;
    stage(
        state,
        desktop,
        name,
        "offline-query",
        visit,
        json!({"activation":activation,"after":current}),
        captures,
        stages,
    )
    .await?;
    let activation = action(desktop, "click", "workflow-share-cancel", "").await?;
    until(state, visit, "startup original Cancel", |p, _| {
        last(p)["closed"] == true && value(p)["active"] == false
    })
    .await?;
    current = observe(state, desktop, visit).await?;
    stage(
        state,
        desktop,
        name,
        "share-cancelled",
        visit,
        json!({"activation":activation,"after":current,"quiet":quiet(state,desktop,visit).await?}),
        captures,
        stages,
    )
    .await?;
    Ok(Value::Null)
}
