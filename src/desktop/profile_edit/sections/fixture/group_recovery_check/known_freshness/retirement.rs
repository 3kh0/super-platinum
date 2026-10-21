use super::*;

pub(super) async fn run(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    first: u64,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let before = recovery::snapshot(&state.peek());
    click(desktop, "Cancel description edit", false).await?;
    form(desktop).await?;
    let cancelled = recovery::snapshot(&state.peek());
    if cancelled["profile"]["blocks"] != before["profile"]["blocks"]
        || recovery::receiver_retired(first)
        || cancelled["knownFreshness"]["opening"]["mounted"] != false
    {
        return Err("Cancel consumed account-owned known work or changed original draft".into());
    }
    capture(
        state,
        desktop,
        name,
        "cancelled",
        json!({"before":before,"after":cancelled,"accountOwnedReadRetained":true}),
        stages,
        captures,
    )
    .await?;
    let response = known::response(true, false);
    let release = recovery::release_observed(first, Ok(response.clone()))?;
    let completed = settled(state, 1).await?;
    source(state, true)?;
    form(desktop).await?;
    if recovery::snapshot(&state.peek())["profile"]["blocks"]
        != json!(recovery::blocks(recovery::DRAFT))
    {
        return Err("Background known result rewrote cancelled description".into());
    }
    capture(
        state,
        desktop,
        name,
        "late-settled",
        json!({"response":response,"release":release,"settled":completed,
            "cancelledConsumerNotRemounted":true}),
        stages,
        captures,
    )
    .await?;
    let count = recovery::requests();
    click(desktop, "Edit description", false).await?;
    editor(desktop, recovery::DRAFT, true).await?;
    let observed = quiet(state, desktop, recovery::DRAFT, true).await?;
    if recovery::requests() != count {
        return Err("Fresh opening ignored account-owned settled source".into());
    }
    capture(
        state,
        desktop,
        name,
        "reopened",
        json!({"newLabels":true,"noRedundantRead":true,"quiet":observed}),
        stages,
        captures,
    )
    .await?;
    let (second, held) = reset_open(state, desktop, recovery::DRAFT, true).await?;
    capture(state, desktop, name, "account-held", held, stages, captures).await?;
    let worker = {
        let shell = state.peek();
        let team = shell
            .core
            .active_team
            .as_deref()
            .ok_or("Missing original team")?;
        crate::usergroups::freshness::fixture_worker(&shell, team)
            .ok_or("Missing original held worker identity")?
    };
    let mut replaced = recovery::replace_account(state);
    wait_dom(desktop,"({ok:!document.getElementById('profile-section-rich')&&!document.getElementById('profile-section-rich-pane')})").await?;
    // Replacement retires publication authority, not the already awaited
    // closed transport. Observe the exact original worker through its own end.
    if worker.strong_count() == 0 || recovery::receiver_retired(second) {
        return Err("Original held worker ended before its late response".into());
    }
    replaced["originalWorkerAlive"] = json!(true);
    capture(
        state,
        desktop,
        name,
        "account-replaced",
        replaced,
        stages,
        captures,
    )
    .await?;
    let before = recovery::snapshot(&state.peek());
    let response = known::response(true, true);
    let release = recovery::release_observed(second, Ok(response.clone()))?;
    if release != json!({"serial":second,"receiverRetired":false,"delivered":true}) {
        return Err("Original late transport response was not delivered".into());
    }
    let requests = recovery::requests();
    let mut finished = false;
    for _ in 0..100 {
        if worker.strong_count() == 0 {
            finished = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    if !finished {
        return Err("Original late worker did not finish after response delivery".into());
    }
    let after = recovery::snapshot(&state.peek());
    if before["account"] != after["account"]
        || before["epoch"] != after["epoch"]
        || before["groups"] != after["groups"]
        || before["profile"] != after["profile"]
        || before["knownFreshness"] != after["knownFreshness"]
        || recovery::requests() != requests
        || !recovery::pending().is_empty()
    {
        return Err("Old known response changed successor account/source/UI".into());
    }
    capture(
        state,
        desktop,
        name,
        "late-refused",
        json!({"before":before,"after":after,"response":response,"release":release,
            "successorUnchanged":true,"originalWorkerFinished":true}),
        stages,
        captures,
    )
    .await
}
