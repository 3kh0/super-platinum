//! Finite native Home routing checks; all pointer/scroll guards are shared.
use super::*;
use dioxus::desktop::DesktopContext;
use external::{Reply, capture};
mod helpers;
use helpers::*;

pub(crate) fn start(state: Signal<ShellState>, desktop: DesktopContext) {
    if !named() {
        return;
    }
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap_or_default();
    dioxus::core::spawn_forever(async move {
        let result = match tokio::time::timeout(
            Duration::from_secs(150),
            run(state, &desktop, &name),
        )
        .await
        {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => {
                json!({"ok":false,"fixture":name,"case":case(),"error":e,"source":snapshot(&state.peek())})
            }
            Err(_) => {
                json!({"ok":false,"fixture":name,"case":case(),"error":"Unified Home timeout"})
            }
        };
        let _ = tokio::task::spawn_blocking(move || {
            eprintln!("app-home-unified-context-check: {result}")
        })
        .await;
    });
}
async fn run(
    mut state: Signal<ShellState>,
    d: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let ready = capture::readiness(name).await?;
    for _ in 0..150 {
        if inputs::ready(&state.peek()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    if !inputs::ready(&state.peek()) {
        return Err("Initial native Home form not ready".into());
    }
    let mut stages = Vec::new();
    let mut captures = serde_json::Map::new();
    let (ingress, open) = if case() == "failure" {
        capture::wait(
            d,
            "!!document.querySelector('#app-home .app-home-toolbar > button')",
        )
        .await?;
        capture::click("#app-home .app-home-toolbar > button").await?;
        for _ in 0..100 {
            if state.peek().app_home.context.fixture_source_reads.len() == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        if state.peek().app_home.context.fixture_source_reads.len() != 1
            || !state.peek().app_home.context.fixture_opens.is_empty()
        {
            return Err("Failed context Refresh published or opened Home".into());
        }
        shot(
            state,
            d,
            name,
            "unavailable",
            json!({"sourceAbsent":true,"actualRefresh":true}),
            &mut stages,
            &mut captures,
        )
        .await?;
        state.write().app_home.context.fixture_source_ready = true;
        capture::click("#app-home .app-home-toolbar > button").await?;
        for _ in 0..150 {
            if state.peek().app_home.context.fixture_opens.len() == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let s = state.peek();
        if s.app_home.context.fixture_source_reads.len() != 2
            || s.app_home.context.fixture_opens.len() != 1
        {
            return Err("Explicit Refresh did not recover source/open exactly once".into());
        }
        (
            s.app_home.context.fixture_source_reads[1]["ingress"].clone(),
            s.app_home.context.fixture_opens[0].clone(),
        )
    } else {
        let ingress = publish(state, case() == "empty", TWO).await?;
        (ingress, opened(state).await?)
    };
    ready_inputs(state, d).await?;
    shot(
        state,
        d,
        name,
        "initial",
        json!({"ingress":ingress,"open":open}),
        &mut stages,
        &mut captures,
    )
    .await?;
    match case().as_str() {
        "selection" => selection(state, d, name, &mut stages, &mut captures).await?,
        "retirement" => retirement(state, d, name, &mut stages, &mut captures).await?,
        "failure" => failure(state, d, name, &mut stages, &mut captures).await?,
        "empty" => empty(state, d, name, &mut stages, &mut captures).await?,
        _ => return Err("Unknown unified Home case".into()),
    }
    Ok(
        json!({"ok":true,"fixture":name,"case":case(),"readiness":ready,"viewport":stages[0]["geometry"]["viewport"],
        "stages":stages,"captures":captures,"source":snapshot(&state.peek()),"captureChecksPassed":true,"networkCalls":0,
        "visualReview":"Individual original PNG review required"}),
    )
}
async fn selection(
    mut state: Signal<ShellState>,
    d: &DesktopContext,
    name: &str,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    query(state, d, "release", 1, false).await?;
    shot(state, d, name, "options-two", Value::Null, stages, captures).await?;
    select_beta(d).await?;
    ready_inputs(state, d).await?;
    shot(
        state,
        d,
        name,
        "selected-two",
        Value::Null,
        stages,
        captures,
    )
    .await?;
    let before = snapshot(&state.peek());
    let buffer = native_buffer(None).await?;
    let native_before = native_identity().await?;
    choose_team(d, THREE).await?;
    route(state, Some(THREE)).await?;
    let after = snapshot(&state.peek());
    let native_after = native_identity().await?;
    unchanged_form(&before, &after)?;
    if before["opens"] != after["opens"] || native_before != native_after {
        return Err("Context change replayed open or remounted native input".into());
    }
    let key = {
        let s = state.peek();
        let h = s.app_home.surface.as_ref().unwrap();
        actions::Key {
            visit: h.id,
            revision: h.revision,
        }
    };
    if let Some(lease) = inputs::native::lock(state, key).await {
        let _ = lease.send(());
        return Err("Unacknowledged retained native buffer passed final action lock".into());
    }
    native_buffer(buffer["before"].as_str()).await?;
    shot(state,d,name,"context-three",json!({"before":before,"after":after,"nativeBefore":native_before,"nativeAfter":native_after,"nativeBuffer":buffer,"unacknowledgedLockRefused":true,"restored":native_identity().await?}),stages,captures).await?;
    query(state, d, "release", 2, false).await?;
    shot(
        state,
        d,
        name,
        "options-three",
        Value::Null,
        stages,
        captures,
    )
    .await?;
    capture::click("[data-home-external] [data-external-done]").await?;
    capture::wait(d, "!document.querySelector('[data-external-search]')").await?;
    let detached = external::depart(&mut state.write());
    capture::wait(d, "!document.querySelector('#app-home')").await?;
    let open = return_home(state, detached).await?;
    ready_inputs(state, d).await?;
    shot(
        state,
        d,
        name,
        "returned-three",
        json!({"open":open}),
        stages,
        captures,
    )
    .await?;
    action(d).await?;
    shot(
        state,
        d,
        name,
        "action-three",
        Value::Null,
        stages,
        captures,
    )
    .await
}
async fn empty(
    state: Signal<ShellState>,
    d: &DesktopContext,
    name: &str,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    if Witness::capture(&state.peek())
        .map_err(|e| e.to_string())?
        .team_id()
        .is_some()
    {
        return Err("Observed empty context did not remain empty".into());
    }
    query(state, d, "release", 1, false).await?;
    shot(
        state,
        d,
        name,
        "options-installed",
        Value::Null,
        stages,
        captures,
    )
    .await?;
    select_beta(d).await?;
    ready_inputs(state, d).await?;
    shot(
        state,
        d,
        name,
        "selected-installed",
        Value::Null,
        stages,
        captures,
    )
    .await?;
    action(d).await?;
    shot(
        state,
        d,
        name,
        "action-installed",
        Value::Null,
        stages,
        captures,
    )
    .await
}
async fn failure(
    mut state: Signal<ShellState>,
    d: &DesktopContext,
    name: &str,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    query(state, d, "release", 1, false).await?;
    select_beta(d).await?;
    ready_inputs(state, d).await?;
    shot(state, d, name, "selected", Value::Null, stages, captures).await?;
    external::configure(&mut state.write(), Reply::Offline, false);
    query(state, d, "offline", 2, false).await?;
    capture::wait(d, "!!document.querySelector('[data-external-retry]')").await?;
    shot(state, d, name, "failed", Value::Null, stages, captures).await?;
    external::configure(&mut state.write(), Reply::Options, true);
    capture::click("[data-external-retry]").await?;
    settle(state, 3, true).await?;
    shot(state, d, name, "retry-held", Value::Null, stages, captures).await?;
    let release = external::release(&mut state.write());
    settle(state, 3, false).await?;
    shot(
        state,
        d,
        name,
        "recovered",
        json!({"release":release}),
        stages,
        captures,
    )
    .await?;
    capture::click("[data-home-external] [data-external-done]").await?;
    capture::wait(d, "!document.querySelector('[data-external-search]')").await?;
    let before = snapshot(&state.peek());
    let (team, flags) = {
        let mut s = state.write();
        let team = s.core.active_team.clone().unwrap();
        let flags =
            std::mem::take(&mut s.core.session.as_mut().unwrap().unified_user_client_enabled);
        (team, flags)
    };
    capture::wait(
        d,
        "document.querySelector('[data-external-open]')?.disabled===true",
    )
    .await?;
    shot(
        state,
        d,
        name,
        "configuration-unavailable",
        json!({"before":before,"team":team}),
        stages,
        captures,
    )
    .await?;
    state
        .write()
        .core
        .session
        .as_mut()
        .unwrap()
        .unified_user_client_enabled = flags;
    ready_inputs(state, d).await?;
    shot(
        state,
        d,
        name,
        "configuration-restored",
        Value::Null,
        stages,
        captures,
    )
    .await?;
    action(d).await?;
    shot(
        state,
        d,
        name,
        "action-recovered",
        Value::Null,
        stages,
        captures,
    )
    .await
}
async fn retirement(
    mut state: Signal<ShellState>,
    d: &DesktopContext,
    name: &str,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    query(state, d, "release", 1, false).await?;
    select_beta(d).await?;
    ready_inputs(state, d).await?;
    external::configure(&mut state.write(), Reply::Options, true);
    query(state, d, "held", 2, true).await?;
    let old = Witness::capture(&state.peek()).map_err(|e| e.to_string())?;
    let old_choice = Choice::capture(&state.peek())
        .map_err(|e| e.to_string())?
        .ok_or("No chooser")?;
    shot(state, d, name, "query-held", Value::Null, stages, captures).await?;
    let ingress = publish(state, false, THREE).await?;
    if old.current(&state.peek()) || choose(&mut state.write(), &old_choice, TWO) {
        return Err("Old metadata retained route/chooser authority".into());
    }
    retired(state).await?;
    let release = external::release(&mut state.write());
    route(state, Some(THREE)).await?;
    shot(state,d,name,"source-retired",json!({"ingress":ingress,"release":release,"oldWitnessCurrent":false,"oldChoiceRefused":true}),stages,captures).await?;
    hold_action(&mut state.write());
    click_action().await?;
    for _ in 0..150 {
        if snapshot(&state.peek())["actionGateWaiting"] == true {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    if snapshot(&state.peek())["actionGateWaiting"] != true {
        return Err("Native action lease not acquired".into());
    }
    choose_team(d, TWO).await?;
    route(state, Some(TWO)).await?;
    if !release_action(&mut state.write()) {
        return Err("Native action lease release failed".into());
    }
    for _ in 0..150 {
        if snapshot(&state.peek())["actionResults"]
            .as_array()
            .is_some_and(|v| v.len() == 1)
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    if snapshot(&state.peek())["actionResults"]
        != json!([{"admitted":false,"afterNativeLock":true}])
        || !state.peek().app_home.fixture_forms.is_empty()
    {
        return Err("Retired native action dispatched".into());
    }
    ready_inputs(state, d).await?;
    shot(
        state,
        d,
        name,
        "native-action-retired",
        json!({"postLockRouteRefused":true}),
        stages,
        captures,
    )
    .await?;
    external::configure(&mut state.write(), Reply::Options, true);
    query(state, d, "socket", 3, true).await?;
    shot(state, d, name, "socket-held", Value::Null, stages, captures).await?;
    {
        let mut s = state.write();
        let team = s.core.active_team.clone().unwrap();
        s.core.workspaces.get_mut(&team).unwrap().rt_generation += 1;
    }
    retired(state).await?;
    let release = external::release(&mut state.write());
    shot(
        state,
        d,
        name,
        "socket-retired",
        json!({"release":release}),
        stages,
        captures,
    )
    .await?;
    // Cached view return is a fixture navigation adapter; the open itself uses
    // production notify_opened and its current source/override witness.
    let detached = external::depart(&mut state.write());
    let open = return_home(state, detached).await?;
    ready_inputs(state, d).await?;
    external::configure(&mut state.write(), Reply::Options, true);
    query(state, d, "account", 4, true).await?;
    shot(
        state,
        d,
        name,
        "account-held",
        json!({"open":open}),
        stages,
        captures,
    )
    .await?;
    {
        let mut s = state.write();
        s.core.account_epoch += 1;
        s.core.active_account = Some("00000000-0000-4000-8000-000000000099".into());
    }
    retired(state).await?;
    let release = external::release(&mut state.write());
    shot(
        state,
        d,
        name,
        "account-retired",
        json!({"release":release}),
        stages,
        captures,
    )
    .await
}
