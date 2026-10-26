use super::*;

pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let pre_mount = {
        let mut shell = state.write();
        let view = presentation(&shell).ok_or("Missing section")?;
        let before = mentions::snapshot(&shell);
        let opening = rich::begin(&mut shell, view.visit, view.serial, "long", 8801)
            .map_err(|e| e.to_string())?;
        let team = shell.core.active_team.clone().ok_or("Missing team")?;
        crate::file_events::invalidate(&mut shell, &team, file::ID);
        let refused = !rich::finish(&mut shell, opening);
        let after = mentions::snapshot(&shell);
        if !refused
            || before["blocks"] != after["blocks"]
            || before["attempts"] != after["attempts"]
        {
            return Err("Stale file opening changed draft".into());
        }
        json!({"openingRefused":refused,"draftUnchanged":true,"noWrite":true})
    };
    unavailable(desktop).await?;
    shot(
        state,
        desktop,
        name,
        "pre-mount-retired",
        pre_mount,
        stages,
        captures,
    )
    .await?;
    click(desktop, "Cancel description edit", false).await?;
    no_editor(desktop).await?;
    source::reply(source::Reply::Private);
    file::mode(file::Mode::Held);
    click(desktop, "Edit description", false).await?;
    files_settled(2, true).await?;
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
    let socket = source::retire_socket(state)?;
    let release = file::release();
    files_settled(2, false).await?;
    unavailable(desktop).await?;
    if rich::files::fixture_status(&state.peek(), file::ID)["current"] == true {
        return Err("Retired socket installed status".into());
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
    file::mode(file::Mode::Normal);
    click(desktop, "Edit description", false).await?;
    files_settled(3, false).await?;
    editor_label(desktop, "Working with ", true, source::PRIVATE).await?;
    file::mode(file::Mode::Held);
    let (sender, receiver) = tokio::sync::oneshot::channel();
    dioxus::core::spawn_forever(async move {
        let _ = sender.send(rich::files::fixture_refresh(state).await.is_ok());
    });
    files_settled(4, true).await?;
    let token = token_view(desktop, true).await?;
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
    let release = file::release();
    let accepted = tokio::time::timeout(Duration::from_secs(5), receiver)
        .await
        .map_err(|_| "Retired read did not finish")?
        .map_err(|_| "Retired read result missing")?;
    files_settled(4, false).await?;
    let after = source::snapshot(&state.peek());
    if accepted
        || account["oldOpeningRefused"] != true
        || account["oldEditorHidden"] != true
        || !rich::files::fixture_status(&state.peek(), file::ID).is_null()
    {
        return Err("Retired account retained source/editor".into());
    }
    for key in ["account", "epoch", "socketGeneration"] {
        if before[key] != after[key] {
            return Err("Late file carrier changed replacement owner".into());
        }
    }
    wait_dom(
        desktop,
        "({ok:!document.querySelector('.profile-edit')&&!!document.querySelector('.sidebar')})",
    )
    .await?;
    shot(state,desktop,name,"account-replaced",json!({"account":account,"release":release,"accepted":accepted,"replacementBefore":before,"replacementAfter":after}),stages,captures).await
}
