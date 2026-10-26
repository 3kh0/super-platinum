use super::*;
use capture::{click, dom, shot, type_query, wait};
use dioxus::desktop::DesktopContext;
use std::time::Duration;
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
                json!({"ok":false,"fixture":name,"case":case(),"error":e,"model":snapshot(&state.peek())})
            }
            Err(_) => {
                json!({"ok":false,"fixture":name,"case":case(),"error":"Home external timeout"})
            }
        };
        let _ = tokio::task::spawn_blocking(move || eprintln!("app-home-external-check: {result}"))
            .await;
    });
}
async fn settle(state: Signal<ShellState>, count: usize, held: bool) -> Result<(), String> {
    for _ in 0..160 {
        let value = snapshot(&state.peek());
        if value["requests"].as_array().map(Vec::len) == Some(count)
            && value["pending"] == held
            && value["query"]["loading"] == held
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    Err(format!(
        "Home query settlement: {}",
        snapshot(&state.peek())
    ))
}
async fn cancelled(state: Signal<ShellState>) -> Result<(), String> {
    for _ in 0..100 {
        if state
            .peek()
            .app_home
            .external
            .fixture
            .as_ref()
            .is_some_and(|f| f.pending.as_ref().is_some_and(|p| p.is_closed()))
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Cancelled Home query still owns reply receiver".into())
}
// The rendered picker can disappear before the existing runtime tick has
// dropped its stale Query. Wait for that real retirement before read-only shots.
async fn retired(state: Signal<ShellState>, home_current: bool) -> Result<(), String> {
    let expected = snapshot(&state.peek());
    if expected["authority"]["homeCurrent"] != home_current {
        return Err("Unexpected Home owner at retirement boundary".into());
    }
    let retained = expected["retainedStates"]
        .as_array()
        .ok_or("Missing retained Home forms")?;
    if retained.len() != 1
        || retained[0]["state"]["values"]["home-choice"]["value"]["selected_option"]
            != option("Release Beta", "beta")
        || (home_current && expected["formState"] != retained[0]["state"])
        || (!home_current && !expected["formState"].is_null())
    {
        return Err("Retirement lost expected selected Home form".into());
    }
    for _ in 0..150 {
        let current = snapshot(&state.peek());
        if current["authority"] != expected["authority"]
            || current["formState"] != expected["formState"]
            || current["retainedStates"] != expected["retainedStates"]
        {
            return Err("Home owner/form changed during runtime retirement".into());
        }
        if current["query"].is_null() && current["pending"] == false {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Home runtime did not retire picker before capture".into())
}
async fn open(d: &DesktopContext) -> Result<(), String> {
    wait(
        d,
        "!!document.querySelector('[data-home-external] [data-external-open]')",
    )
    .await?;
    click("[data-home-external] [data-external-open]").await?;
    wait(
        d,
        "!!document.querySelector('[data-home-external] input[type=search]')",
    )
    .await?;
    Ok(())
}
async fn run(
    mut state: Signal<ShellState>,
    d: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let ready = capture::readiness(name).await?;
    wait(
        d,
        "!!document.querySelector('[data-home-external] [data-external-open]')",
    )
    .await?;
    // Wait for the real mounted native date input acknowledgement, never seed it.
    for _ in 0..100 {
        if super::super::super::ready(&state.peek()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    if !super::super::super::ready(&state.peek()) {
        return Err("Native Home inputs not ready".into());
    }
    let mut stages = vec![];
    let mut captures = serde_json::Map::new();
    shot(
        state,
        d,
        name,
        "initial",
        Value::Null,
        &mut stages,
        &mut captures,
    )
    .await?;
    match case().as_str() {
        "selection" => selection(state, d, name, &mut stages, &mut captures).await?,
        "failure" => failure(state, d, name, &mut stages, &mut captures).await?,
        "retirement" => retirement(state, d, name, &mut stages, &mut captures).await?,
        "rate-limit" => rate_limit(state, d, name, &mut stages, &mut captures).await?,
        _ => return Err("Unknown Home fixture case".into()),
    }
    Ok(
        json!({"ok":true,"fixture":name,"case":case(),"readiness":ready,"viewport":stages[0]["geometry"]["viewport"],"stages":stages,"captures":captures,"model":snapshot(&state.peek()),"captureChecksPassed":true,"visualReview":"Individual original PNG review required","networkCalls":0}),
    )
}
async fn selection(
    mut state: Signal<ShellState>,
    d: &DesktopContext,
    name: &str,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    open(d).await?;
    type_query("ab").await?;
    wait(
        d,
        "document.querySelector('[data-external-search]')?.textContent.includes('Type at least 3')",
    )
    .await?;
    if !state
        .peek()
        .app_home
        .external
        .fixture
        .as_ref()
        .unwrap()
        .requests
        .is_empty()
    {
        return Err("Below-minimum search dispatched".into());
    }
    shot(
        state,
        d,
        name,
        "minimum",
        json!({"requests":0}),
        stages,
        captures,
    )
    .await?;
    configure(&mut state.write(), Reply::Options, true);
    type_query("release").await?;
    settle(state, 1, true).await?;
    shot(state, d, name, "loading", Value::Null, stages, captures).await?;
    let released = release(&mut state.write());
    settle(state, 1, false).await?;
    wait(
        d,
        "document.querySelectorAll('[data-home-external] [data-option-index]').length===2",
    )
    .await?;
    shot(state, d, name, "options", released, stages, captures).await?;
    click("[data-home-external] [data-option-index='0']").await?;
    wait(d,"!document.querySelector('[data-external-search]')&&document.querySelector('[data-home-external]')?.textContent.includes('Release Alpha')").await?;
    if !state.peek().app_home.fixture_forms.is_empty() {
        return Err("Quiet selection sent action".into());
    }
    shot(state, d, name, "selected", Value::Null, stages, captures).await?;
    if !refresh(&mut state.write()) {
        return Err("Current Home refresh refused".into());
    }
    wait(
        d,
        "document.querySelector('[data-home-external]')?.textContent.includes('Release Alpha')",
    )
    .await?;
    shot(state, d, name, "refreshed", Value::Null, stages, captures).await?;
    let before_departure = snapshot(&state.peek())["formState"].clone();
    let detached = depart(&mut state.write());
    wait(d, "!document.getElementById('app-home')").await?;
    return_home(&mut state.write(), detached);
    wait(
        d,
        "document.querySelector('[data-home-external]')?.textContent.includes('Release Alpha')",
    )
    .await?;
    for _ in 0..100 {
        if super::super::super::ready(&state.peek()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    let after_return = snapshot(&state.peek())["formState"].clone();
    if before_departure != after_return || !super::super::super::ready(&state.peek()) {
        return Err("Home departure lost owned selected state or native readiness".into());
    }
    shot(state,d,name,"returned",json!({"before":before_departure,"after":after_return,"cachedViewAdapter":true,"actualLeave":true}),stages,captures).await?;
    // Diverge the native date value without emitting an input: final lock must refuse.
    let native=dom(r#"(()=>{const e=document.querySelector('#app-home input[type=date]');if(!e)return{ok:false};const before=e.value;e.value='2026-10-25';return{ok:true,before,after:e.value};})()"#).await?;
    let visit = {
        let s = state.peek();
        let h = s.app_home.surface.as_ref().unwrap();
        actions::Key {
            visit: h.id,
            revision: h.revision,
        }
    };
    if let Some(lease) = super::super::super::native::lock(state, visit).await {
        let _ = lease.send(());
        return Err("Unacknowledged native date bypassed final lock".into());
    }
    if !state.peek().app_home.fixture_forms.is_empty() {
        return Err("Native guard sent an app action".into());
    }
    dom(&format!(r#"(()=>{{const e=document.querySelector('#app-home input[type=date]');e.value={};e.dispatchEvent(new Event('input',{{bubbles:true}}));return{{ok:true}};}})()"#,native["before"])).await?;
    wait(d,"window.__spHomeCalendarNative&&[...window.__spHomeCalendarNative.entries.values()].every(e=>!e.pending&&e.acked===e.seq)").await?;
    shot(
        state,
        d,
        name,
        "native-guard",
        json!({"native":native,"unacknowledgedNativeLockRefused":true}),
        stages,
        captures,
    )
    .await?;
    click("#app-home [aria-label='Review release']").await?;
    wait(d, "!!document.querySelector('.app-modal')").await?;
    if state.peek().app_home.fixture_forms.len() != 1 {
        return Err("Explicit Home action count".into());
    }
    shot(
        state,
        d,
        name,
        "action",
        json!({"explicitAction":true}),
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
    configure(&mut state.write(), Reply::Options, false);
    open(d).await?;
    type_query("release").await?;
    settle(state, 1, false).await?;
    wait(
        d,
        "document.querySelectorAll('[data-option-index]').length===2",
    )
    .await?;
    shot(
        state,
        d,
        name,
        "prior-options",
        Value::Null,
        stages,
        captures,
    )
    .await?;
    type_query("ab").await?;
    wait(d,"document.querySelector('[data-external-search]')?.textContent.includes('Type at least 3')&&document.querySelectorAll('[data-option-index]').length===2").await?;
    if state
        .peek()
        .app_home
        .external
        .fixture
        .as_ref()
        .unwrap()
        .requests
        .len()
        != 1
    {
        return Err("Below-minimum retained search dispatched".into());
    }
    shot(
        state,
        d,
        name,
        "below-minimum",
        json!({"retainedRowsSelectable":true}),
        stages,
        captures,
    )
    .await?;
    click("[data-home-external] [data-option-index='0']").await?;
    wait(d, "!document.querySelector('[data-external-search]')").await?;
    shot(
        state,
        d,
        name,
        "prior-selected",
        Value::Null,
        stages,
        captures,
    )
    .await?;
    open(d).await?;
    type_query("release").await?;
    settle(state, 2, false).await?;
    configure(&mut state.write(), Reply::Offline, false);
    type_query("offline").await?;
    settle(state, 3, false).await?;
    wait(d,"document.querySelector('[data-external-search]')?.textContent.includes('disconnected')&&document.querySelectorAll('[data-option-index]').length===2").await?;
    shot(
        state,
        d,
        name,
        "failed",
        json!({"priorRowsRetained":true,"selectedDraftRetained":true}),
        stages,
        captures,
    )
    .await?;
    configure(&mut state.write(), Reply::Replacement, true);
    click("[data-external-retry]").await?;
    settle(state, 4, true).await?;
    shot(state, d, name, "retry-held", Value::Null, stages, captures).await?;
    let old_owner = super::super::owner(&state.peek(), &key()).ok_or("No held Home owner")?;
    let old_revision = old_owner.query(&state.peek()).unwrap().revision;
    let retained_selection = snapshot(&state.peek())["formState"].clone();
    let proof = release(&mut state.write());
    settle(state, 4, false).await?;
    if snapshot(&state.peek())["formState"] != retained_selection || !validated(&state.peek()) {
        return Err("Same-value replacement changed retained selection".into());
    }
    let mut stale_event = None;
    let chosen = super::super::choose_impl(
        &mut state.write(),
        old_owner,
        old_revision,
        0,
        &mut stale_event,
    );
    if chosen || stale_event.is_some() {
        return Err("Replaced row retained old click authority".into());
    }
    shot(
        state,
        d,
        name,
        "recovered",
        json!({"release":proof,"staleRowClickRefused":true,"sameValueOldSelectionRetained":true,"before":retained_selection,"after":snapshot(&state.peek())["formState"]}),
        stages,
        captures,
    )
    .await?;
    click("[data-home-external] [data-option-index='0']").await?;
    wait(d,"!document.querySelector('[data-external-search]')&&document.querySelector('[data-home-external]')?.textContent.includes('Release Gamma')").await?;
    shot(state, d, name, "selected", Value::Null, stages, captures).await?;
    click("[data-home-external] [aria-label='Clear selected option']").await?;
    wait(
        d,
        "!document.querySelector('[data-home-external] .message-multi-selected-option')",
    )
    .await?;
    shot(state, d, name, "cleared", Value::Null, stages, captures).await?;
    open(d).await?;
    click("[data-external-done]").await?;
    wait(d, "!document.querySelector('[data-external-search]')").await?;
    shot(state, d, name, "closed", Value::Null, stages, captures).await
}
async fn retirement(
    mut state: Signal<ShellState>,
    d: &DesktopContext,
    name: &str,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    configure(&mut state.write(), Reply::Options, false);
    open(d).await?;
    type_query("release").await?;
    settle(state, 1, false).await?;
    wait(
        d,
        "document.querySelectorAll('[data-option-index]').length===2",
    )
    .await?;
    shot(
        state,
        d,
        name,
        "prior-options",
        Value::Null,
        stages,
        captures,
    )
    .await?;
    configure(&mut state.write(), Reply::Options, true);
    type_query("next").await?;
    settle(state, 2, true).await?;
    shot(
        state,
        d,
        name,
        "held",
        json!({"priorRowsSelectable":true}),
        stages,
        captures,
    )
    .await?;
    click("[data-home-external] [data-option-index='1']").await?;
    wait(d,"!document.querySelector('[data-external-search]')&&document.querySelector('[data-home-external]')?.textContent.includes('Release Beta')").await?;
    cancelled(state).await?;
    let proof = release(&mut state.write());
    if proof["delivered"] != false {
        return Err("Selecting retained row failed to cancel newer request".into());
    }
    shot(
        state,
        d,
        name,
        "retained-choice",
        json!({"release":proof,"selectedWhileLoading":true}),
        stages,
        captures,
    )
    .await?;
    configure(&mut state.write(), Reply::Options, true);
    open(d).await?;
    type_query("release").await?;
    settle(state, 3, true).await?;
    type_query("ab").await?;
    wait(
        d,
        "document.querySelector('[data-external-search]')?.textContent.includes('Type at least 3')",
    )
    .await?;
    cancelled(state).await?;
    let cancellation = release(&mut state.write());
    if cancellation["delivered"] != false {
        return Err("Retired query receiver live".into());
    }
    shot(
        state,
        d,
        name,
        "query-retired",
        cancellation,
        stages,
        captures,
    )
    .await?;
    configure(&mut state.write(), Reply::Options, true);
    type_query("release").await?;
    settle(state, 4, true).await?;
    if !refresh(&mut state.write()) {
        return Err("View refresh refused".into());
    }
    cancelled(state).await?;
    let proof = release(&mut state.write());
    if proof["delivered"] != false {
        return Err("View-retired reply delivered".into());
    }
    wait(d, "!document.querySelector('[data-external-search]')").await?;
    retired(state, true).await?;
    shot(state, d, name, "view-retired", proof, stages, captures).await?;
    configure(&mut state.write(), Reply::Options, true);
    open(d).await?;
    type_query("release").await?;
    settle(state, 5, true).await?;
    shot(state, d, name, "socket-held", Value::Null, stages, captures).await?;
    {
        let mut s = state.write();
        let team = s.core.active_team.clone().unwrap();
        s.core.workspaces.get_mut(&team).unwrap().rt_generation += 1;
    }
    cancelled(state).await?;
    let proof = release(&mut state.write());
    if proof["delivered"] != false {
        return Err("Socket-retired reply delivered".into());
    }
    wait(d, "!document.querySelector('[data-external-search]')").await?;
    retired(state, false).await?;
    shot(state, d, name, "socket-retired", proof, stages, captures).await?;
    // Reopen the same fixture-owned cached Home as a new visit, preserving form values.
    {
        let mut s = state.write();
        let owner = super::super::super::super::Owner::capture(&s).ok_or("No new Home owner")?;
        let h = s.app_home.surface.as_mut().unwrap();
        h.id = owner.home_visit;
        h.owner = owner;
        h.revision = next();
    }
    configure(&mut state.write(), Reply::Options, true);
    open(d).await?;
    type_query("release").await?;
    settle(state, 6, true).await?;
    shot(
        state,
        d,
        name,
        "account-held",
        Value::Null,
        stages,
        captures,
    )
    .await?;
    {
        let mut s = state.write();
        s.core.account_epoch += 1;
    }
    cancelled(state).await?;
    let proof = release(&mut state.write());
    if proof["delivered"] != false {
        return Err("Account-retired reply delivered".into());
    }
    wait(d, "!document.querySelector('[data-external-search]')").await?;
    retired(state, false).await?;
    shot(state, d, name, "account-retired", proof, stages, captures).await
}

async fn rate_limit(
    mut state: Signal<ShellState>,
    d: &DesktopContext,
    name: &str,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    configure(&mut state.write(), Reply::RateLimited, false);
    open(d).await?;
    type_query("release").await?;
    settle(state, 1, false).await?;
    wait(
        d,
        "document.querySelector('[data-external-retry]')?.disabled===true",
    )
    .await?;
    shot(
        state,
        d,
        name,
        "rate-limited",
        json!({"retryAfterSeconds":8}),
        stages,
        captures,
    )
    .await?;
    click("[data-external-done]").await?;
    wait(d, "!document.querySelector('[data-external-search]')").await?;
    shot(state, d, name, "closed", Value::Null, stages, captures).await?;
    open(d).await?;
    type_query("release").await?;
    wait(d,"document.querySelector('[data-external-retry]')?.disabled===true&&document.querySelector('[data-external-search]')?.textContent.includes('wait')").await?;
    shot(
        state,
        d,
        name,
        "reopened-cooling",
        json!({"sameRouteCooldownRetained":true}),
        stages,
        captures,
    )
    .await?;
    for _ in 0..500 {
        if super::super::owner(&state.peek(), &key())
            .and_then(|o| o.query(&state.peek()).map(|q| q.retryable()))
            .unwrap_or(false)
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    wait(
        d,
        "document.querySelector('[data-external-retry]')?.disabled===false",
    )
    .await?;
    if state
        .peek()
        .app_home
        .external
        .fixture
        .as_ref()
        .unwrap()
        .requests
        .len()
        != 1
    {
        return Err("Rate limit automatically replayed".into());
    }
    shot(
        state,
        d,
        name,
        "ready-to-retry",
        json!({"noAutomaticReplay":true}),
        stages,
        captures,
    )
    .await?;
    configure(&mut state.write(), Reply::Replacement, true);
    click("[data-external-retry]").await?;
    settle(state, 2, true).await?;
    shot(state, d, name, "retry-held", Value::Null, stages, captures).await?;
    let proof = release(&mut state.write());
    settle(state, 2, false).await?;
    shot(state, d, name, "recovered", proof, stages, captures).await?;
    click("[data-home-external] [data-option-index='0']").await?;
    wait(d,"!document.querySelector('[data-external-search]')&&document.querySelector('[data-home-external]')?.textContent.includes('Release Gamma')").await?;
    shot(state, d, name, "selected", Value::Null, stages, captures).await
}
