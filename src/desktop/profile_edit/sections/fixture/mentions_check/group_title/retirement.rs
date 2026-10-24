use super::*;
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
    transport::release(first, Ok(source::response(true)))?;
    settled(state, 1).await?;
    if rich::finish(&mut state.write(), opening) {
        return Err("Old nested group opening survived source change".into());
    }
    if mentions::snapshot(&state.peek())["blocks"] != json!(file::blocks("Working with ")) {
        return Err("Stale nested opening changed original form".into());
    }
    shot(
        state,
        desktop,
        name,
        "pre-mount-retired",
        json!({"oldOpeningRefused":true,"draftUnchanged":true}),
        stages,
        captures,
    )
    .await?;
    click(desktop, "Edit description", false).await?;
    editor_label(desktop, "Working with ", true, source::RENAMED).await?;
    let reset = reset_open(state, desktop, "Working with ", source::RENAMED).await?;
    let second = request(state, "directory").await?;
    let socket = source::reconnect(&mut state.write())?;
    let mut poison = source::raw(true);
    poison["handle"] = json!("retired-socket-name");
    poison["date_update"] = json!(999);
    let release = transport::release_observed(second, Ok(json!({"ok":true,"results":[poison]})))?;
    let successor = request(state, "directory").await?;
    transport::release(successor, Ok(source::response(true)))?;
    settled(state, 3).await?;
    require_label(state, "@renamed")?;
    editor_label(desktop, "Working with ", true, source::RENAMED).await?;
    shot(state,desktop,name,"socket-retired",json!({"reset":reset,"socket":socket,"release":release,"successor":successor,"latePublicationRefused":true}),stages,captures).await?;
    // Refresh the file receipt under the current socket before the next lifecycle reset.
    click(desktop, "Cancel description edit", false).await?;
    click(desktop, "Edit description", false).await?;
    editor_label(desktop, "Working with ", true, source::RENAMED).await?;
    let reset = reset_open(state, desktop, "Working with ", source::RENAMED).await?;
    let last = request(state, "directory").await?;
    let worker = {
        let s = state.peek();
        crate::usergroups::freshness::fixture_worker(
            &s,
            s.core.active_team.as_deref().ok_or("Missing team")?,
        )
        .ok_or("Missing original group worker")?
    };
    shot(
        state,
        desktop,
        name,
        "account-held",
        json!({"reset":reset,"request":last}),
        stages,
        captures,
    )
    .await?;
    let account = mentions::replace_account(state)?;
    if account["oldOpeningRefused"] != true || account["oldEditorHidden"] != true {
        return Err("Account replacement retained nested opening".into());
    }
    let before = source::snapshot(&state.peek());
    let release = transport::release_observed(last, Ok(source::response(true)))?;
    for _ in 0..100 {
        if worker.strong_count() == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    if worker.strong_count() != 0 || !transport::pending().is_empty() {
        return Err("Retired group worker did not close".into());
    }
    let after = source::snapshot(&state.peek());
    for field in [
        "account",
        "epoch",
        "group",
        "label",
        "shared",
        "socketGeneration",
    ] {
        if before[field] != after[field] {
            return Err(format!(
                "Retired group callback mutated replacement {field}"
            ));
        }
    }
    if !after["group"].is_null() {
        return Err("Retired group leaked into replacement source".into());
    }
    wait_dom(
        desktop,
        "({ok:!document.querySelector('.profile-edit')&&!!document.querySelector('.sidebar')})",
    )
    .await?;
    shot(state,desktop,name,"account-replaced",json!({"account":account,"release":release,"before":before,"after":after,"originalWorkerFinished":true,"successorUnchanged":true}),stages,captures).await
}
