use super::*;
pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let (opening, before) = {
        let mut shell = state.write();
        let v = presentation(&shell).ok_or("Missing section")?;
        let before = mentions::snapshot(&shell);
        let opening =
            rich::begin(&mut shell, v.visit, v.serial, "long", 8801).map_err(|e| e.to_string())?;
        (opening, before)
    };
    source::configure(source::Reply::Changed, false);
    rich::files::sections::fixture_refresh(state)
        .await
        .map_err(|e| e.to_string())?;
    settled(2, false).await?;
    let refused = !rich::finish(&mut state.write(), opening);
    let after = mentions::snapshot(&state.peek());
    if !refused || before["blocks"] != after["blocks"] || before["attempts"] != after["attempts"] {
        return Err("Stale section opening changed draft".into());
    }
    wait_dom(desktop,"({ok:!document.getElementById('profile-section-rich')&&!!document.getElementById('profile-section-rich-status')})").await?;
    shot(
        state,
        desktop,
        name,
        "pre-mount-retired",
        json!({"openingRefused":true,"draftUnchanged":true,"noWrite":true}),
        stages,
        captures,
    )
    .await?;
    // A current error clears the file. It is source acquisition, not a fixture
    // cache reset, and creates the next real missing-snippet opening.
    source::configure(source::Reply::Failure, false);
    rich::files::sections::fixture_refresh(state)
        .await
        .map_err(|e| e.to_string())?;
    settled(3, false).await?;
    click(desktop, "Cancel description edit", false).await?;
    no_editor(desktop).await?;
    source::configure(source::Reply::Present, true);
    click(desktop, "Edit description", false).await?;
    settled(4, true).await?;
    shot(
        state,
        desktop,
        name,
        "socket-held",
        json!({"held":true}),
        stages,
        captures,
    )
    .await?;
    let socket = {
        let mut shell = state.write();
        let team = shell.core.active_team.clone().ok_or("Missing team")?;
        let w = shell
            .core
            .workspaces
            .get_mut(&team)
            .ok_or("Missing workspace")?;
        let before = w.rt_generation;
        w.rt_generation += 1;
        json!({"before":before,"after":w.rt_generation})
    };
    let release = source::release();
    settled(4, false).await?;
    wait_dom(desktop,"({ok:!document.getElementById('profile-section-rich')&&!!document.getElementById('profile-section-rich-status')})").await?;
    let rows = rich::files::sections::fixture_status(&state.peek());
    if rows["rows"]
        .as_array()
        .is_none_or(|rows| rows.iter().any(|r| r["hasEntry"] == true))
    {
        return Err("Retired section response installed source".into());
    }
    shot(
        state,
        desktop,
        name,
        "socket-retired",
        json!({"socket":socket,"release":release,"latePublicationRefused":true}),
        stages,
        captures,
    )
    .await?;
    click(desktop, "Cancel description edit", false).await?;
    no_editor(desktop).await?;
    source::configure(source::Reply::Present, false);
    click(desktop, "Edit description", false).await?;
    settled(5, false).await?;
    let token = token_view(desktop, "Working with ", source::Reply::Present).await?;
    shot(
        state,
        desktop,
        name,
        "reopened",
        json!({"newSocket":true,"token":token}),
        stages,
        captures,
    )
    .await?;
    source::configure(source::Reply::Changed, true);
    let (tx, rx) = tokio::sync::oneshot::channel();
    dioxus::core::spawn_forever(async move {
        let _ = tx.send(rich::files::sections::fixture_refresh(state).await.is_ok());
    });
    settled(6, true).await?;
    let token = token_view(desktop, "Working with ", source::Reply::Present).await?;
    shot(
        state,
        desktop,
        name,
        "account-held",
        json!({"held":true,"token":token}),
        stages,
        captures,
    )
    .await?;
    let account = mentions::replace_account(state)?;
    let before = source::snapshot(&state.peek());
    let release = source::release();
    let accepted = tokio::time::timeout(Duration::from_secs(5), rx)
        .await
        .map_err(|_| "Retired section read did not finish")?
        .map_err(|_| "Missing section read result")?;
    settled(6, false).await?;
    let after = source::snapshot(&state.peek());
    if accepted
        || account["oldOpeningRefused"] != true
        || account["oldEditorHidden"] != true
        || !after["sectionSource"].is_null()
    {
        return Err("Retired account retained section source/editor".into());
    }
    for key in ["account", "epoch", "socketGeneration"] {
        if before[key] != after[key] {
            return Err("Late section reply changed replacement owner".into());
        }
    }
    wait_dom(
        desktop,
        "({ok:!document.querySelector('.profile-edit')&&!!document.querySelector('.sidebar')})",
    )
    .await?;
    shot(state,desktop,name,"account-replaced",json!({"account":account,"release":release,"accepted":accepted,"replacementBefore":before,"replacementAfter":after}),stages,captures).await
}
