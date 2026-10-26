use super::*;
async fn worker_closed(worker: &std::sync::Weak<()>) -> Result<(), String> {
    for _ in 0..100 {
        if worker.strong_count() == 0 && transport::pending().is_empty() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err("Retired missing-group worker did not close".into())
}
fn worker(state: Signal<ShellState>) -> Result<std::sync::Weak<()>, String> {
    let s = state.peek();
    crate::usergroups::unknown::fixture_worker(
        &s,
        s.core.active_team.as_deref().ok_or("Missing team")?,
    )
    .ok_or("Missing owned worker".into())
}
fn no_group(state: Signal<ShellState>) -> Result<(), String> {
    let s = source::snapshot(&state.peek());
    if !s["group"].is_null() {
        return Err("Retired missing-group response published".into());
    }
    Ok(())
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    first: u64,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    click(desktop, "Cancel description edit", false).await?;
    no_editor(desktop).await?;
    let opening = {
        let mut s = state.write();
        let v = presentation(&s).ok_or("Missing section")?;
        rich::begin(&mut s, v.visit, v.serial, "long", 9001).map_err(|e| e.to_string())?
    };
    let first_worker = worker(state)?;
    let socket = source::reconnect(&mut state.write())?;
    let release = transport::release_observed(first, Ok(source::response()))?;
    worker_closed(&first_worker).await?;
    no_group(state)?;
    if rich::finish(&mut state.write(), opening) {
        return Err("Captured opening survived socket retirement".into());
    }
    if mentions::snapshot(&state.peek())["blocks"] != json!(file::blocks("Working with ")) {
        return Err("Retirement changed raw draft".into());
    }
    shot(
        state,
        desktop,
        name,
        "pre-mount-retired",
        json!({"socket":socket,"release":release,"oldOpeningRefused":true,"draftUnchanged":true}),
        stages,
        captures,
    )
    .await?;
    click(desktop, "Edit description", false).await?;
    editor_label(desktop, "Working with ", true, source::ORIGINAL).await?;
    let second = request(state, "directory").await?;
    let second_worker = worker(state)?;
    shot(
        state,
        desktop,
        name,
        "socket-held",
        json!({"request":second}),
        stages,
        captures,
    )
    .await?;
    let socket = source::reconnect(&mut state.write())?;
    let release = transport::release_observed(second, Ok(source::response()))?;
    worker_closed(&second_worker).await?;
    no_group(state)?;
    editor_label(desktop, "Working with ", true, source::ORIGINAL).await?;
    shot(state,desktop,name,"socket-retired",json!({"socket":socket,"release":release,"latePublicationRefused":true,"mountedLabelRetained":true}),stages,captures).await?;
    click(desktop, "Cancel description edit", false).await?;
    click(desktop, "Edit description", false).await?;
    editor_label(desktop, "Working with ", true, source::ORIGINAL).await?;
    let last = request(state, "directory").await?;
    let last_worker = worker(state)?;
    shot(
        state,
        desktop,
        name,
        "account-held",
        json!({"request":last}),
        stages,
        captures,
    )
    .await?;
    let account = mentions::replace_account(state)?;
    let before = source::snapshot(&state.peek());
    let release = transport::release_observed(last, Ok(source::response()))?;
    worker_closed(&last_worker).await?;
    let after = source::snapshot(&state.peek());
    for field in [
        "account",
        "epoch",
        "group",
        "source",
        "selfRestricted",
        "socketGeneration",
        "unknown",
        "shared",
    ] {
        if before[field] != after[field] {
            return Err(format!("Retired callback changed replacement {field}"));
        }
    }
    if account["oldOpeningRefused"] != true
        || account["oldEditorHidden"] != true
        || !after["group"].is_null()
    {
        return Err("Account retirement retained original editor/group".into());
    }
    wait_dom(
        desktop,
        "({ok:!document.querySelector('.profile-edit')&&!!document.querySelector('.sidebar')})",
    )
    .await?;
    shot(state,desktop,name,"account-replaced",json!({"account":account,"release":release,"before":before,"after":after,"originalWorkerFinished":true,"successorUnchanged":true}),stages,captures).await
}
