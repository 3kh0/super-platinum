use super::*;
pub(super) fn plural(mut state: Signal<ShellState>) -> Result<Value, String> {
    let mut shell = state.write();
    let second = json!({"ok":true,"file":{"id":source::SECOND,"mode":"quip","filetype":"quip","title":"Second known canvas","updated":101}});
    rich::files::fixture_observe(&mut shell, &second).map_err(|e| e.to_string())?;
    let owner = source::owner(&shell)?;
    let empty = source::event(
        &mut shell,
        &owner,
        json!({"type":"file_deleted","file_ids":[],"file_id":file::ID}),
    )?;
    if empty["before"] != empty["after"] {
        return Err("Empty plural fell back to singular".into());
    }
    let nested = source::event(
        &mut shell,
        &owner,
        json!({"type":"file_deleted","file":{"id":file::ID}}),
    )?;
    if nested["before"] != nested["after"] {
        return Err("Nested legacy file invented deletion".into());
    }
    let event = source::event(
        &mut shell,
        &owner,
        json!({"type":"file_deleted","file_ids":[file::ID,null,false,"",file::ID,source::SECOND,source::ABSENT],"file_id":"FIGNORED01"}),
    )?;
    if event["after"]["rows"][0]["current"] != true
        || event["after"]["rows"][1]["current"] != true
        || !event["after"]["rows"][2]["source"].is_null()
    {
        return Err("Plural known-only publication wrong".into());
    }
    Ok(json!({"secondReceipt":second,"empty":empty,"nested":nested,"event":event}))
}
pub(super) async fn retirement(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let old_owner = source::owner(&state.peek())?;
    let socket = source::retire_socket(state)?;
    let stale = source::event(&mut state.write(), &old_owner, deletion())?;
    if stale["acceptedOwner"] != false || stale["before"] != stale["after"] {
        return Err("Old socket event changed source".into());
    }
    shot(
        state,
        desktop,
        name,
        "old-socket-refused",
        json!({"socket":socket,"event":stale,"token":token_view(desktop,false).await?}),
        stages,
        captures,
    )
    .await?;
    let before = mentions::snapshot(&state.peek());
    let event = current_event(state, deletion())?;
    // begin now uses the current event fact; a second exact event retires this
    // pre-mount observation while preserving the same visible tombstone.
    let refusal = {
        let mut shell = state.write();
        let view = presentation(&shell).ok_or("Missing pane")?;
        let opening = rich::begin(&mut shell, view.visit, view.serial, "long", 8802)
            .map_err(|e| e.to_string())?;
        let owner = source::owner(&shell)?;
        let successor = source::event(&mut shell, &owner, deletion())?;
        let refused = !rich::finish(&mut shell, opening);
        if !refused || before["blocks"] != mentions::snapshot(&shell)["blocks"] {
            return Err("Pre-mount event failed to retire old opening".into());
        }
        json!({"openingRefused":true,"draftUnchanged":true,"event":event,"successor":successor})
    };
    unavailable(desktop).await?;
    shot(
        state,
        desktop,
        name,
        "pre-mount-retired",
        refusal,
        stages,
        captures,
    )
    .await?;
    click(desktop, "Cancel description edit", false).await?;
    no_editor(desktop).await?;
    click(desktop, "Edit description", false).await?;
    editor_label(desktop, "Working with ", true, source::DELETED).await?;
    shot(
        state,
        desktop,
        name,
        "current-deleted",
        json!({"token":token_view(desktop,true).await?}),
        stages,
        captures,
    )
    .await?;
    let owner = source::owner(&state.peek())?;
    let account = mentions::replace_account(state)?;
    let foreign = source::event(&mut state.write(), &owner, deletion())?;
    if foreign["acceptedOwner"] != false
        || foreign["before"] != foreign["after"]
        || account["oldOpeningRefused"] != true
        || account["oldEditorHidden"] != true
    {
        return Err("Foreign event gained replacement source".into());
    }
    wait_dom(
        desktop,
        "({ok:!document.querySelector('.profile-edit')&&!!document.querySelector('.sidebar')})",
    )
    .await?;
    shot(
        state,
        desktop,
        name,
        "account-replaced",
        json!({"account":account,"event":foreign}),
        stages,
        captures,
    )
    .await
}
