//! Nonmember placeholder and restored ordinary membership, with exact custody.
use super::*;
const RESTORED_NAME: &str = "member-only-harbor";
const RESTORED_LABEL: &str = "Notes for #member-only-harbor";

fn unchanged_draft(state: Signal<ShellState>) -> Result<(), String> {
    if mentions::snapshot(&state.peek())["blocks"] != json!(file::blocks("Working with ")) {
        return Err("Membership change rewrote the original description".into());
    }
    Ok(())
}
async fn placeholder_editor(desktop: &DesktopContext) -> Result<(), String> {
    editor_label(desktop, "Working with ", true, source::PLACEHOLDER).await?;
    // Only title/editor admission is asserted; synthetic raw source and the
    // independently rendered sidebar are not a secret-disclosure test.
    wait_dom(desktop,&format!("({{ok:!document.querySelector('#profile-section-rich-pane')?.textContent.includes({})}})",json!(RESTORED_NAME))).await.map(|_|())
}

pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    initial: u64,
    requests: &mut Vec<Value>,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    if !source::private() {
        return Err("Membership case requires private title flag".into());
    }
    click(desktop, "Cancel description edit", false).await?;
    no_editor(desktop).await?;
    let opening = {
        let mut shell = state.write();
        let v = presentation(&shell).ok_or("Missing private section")?;
        rich::begin(&mut shell, v.visit, v.serial, "long", 9011).map_err(|e| e.to_string())?
    };
    let mut revoked = source::row(RESTORED_NAME, 102);
    revoked["is_member"] = json!(false);
    release(
        state,
        initial,
        Ok(json!({"results":[revoked],"member_channels":[]})),
        requests,
    )
    .await?;
    idle(state).await?;
    if source::snapshot(&state.peek())["channel"]["is_member"] != false {
        return Err("Revoked private membership was not canonically published".into());
    }
    if rich::finish(&mut state.write(), opening) {
        return Err("Old private-member opening survived membership revocation".into());
    }
    unchanged_draft(state)?;
    shot(
        state,
        desktop,
        name,
        "pre-mount-revoked",
        json!({"oldOpeningRefused":true,"rawDraftRetained":true}),
        stages,
        captures,
    )
    .await?;
    click(desktop, "Edit description", false).await?;
    placeholder_editor(desktop).await?;
    unchanged_draft(state)?;
    if !transport::pending(&state.peek()).is_empty()
        || rich::files::fixture_status(&state.peek(), file::ID)["current"] != true
    {
        return Err(
            "Nonmember placeholder did not reuse current source/conditional freshness".into(),
        );
    }
    shot(
        state,
        desktop,
        name,
        "nonmember-placeholder",
        json!({"placeholderAdmitted":true,"suppliedNameNotUsed":true,"rawDraftRetained":true}),
        stages,
        captures,
    )
    .await?;

    let event = source::restore_membership(state)?;
    idle(state).await?;
    let restored = source::snapshot(&state.peek());
    if restored["channel"]["is_member"] != true
        || restored["channel"]["is_private"] != true
        || restored["channel"]["name"] != RESTORED_NAME
    {
        return Err("Restored private source not published".into());
    }
    // Source restoration cannot replace the already admitted local label.
    placeholder_editor(desktop).await?;
    unchanged_draft(state)?;
    shot(
        state,
        desktop,
        name,
        "restored-mounted",
        json!({"event":event,"mountedPlaceholderRetained":true,"rawDraftRetained":true}),
        stages,
        captures,
    )
    .await?;
    click(desktop, "Cancel description edit", false).await?;
    transport::resynced(state, vec![mentions::CHANNEL.into()]).map_err(|e| e.to_string())?;
    click(desktop, "Edit description", false).await?;
    editor_label(desktop, "Working with ", true, RESTORED_LABEL).await?;
    let retry = request(state, requests, "channels.info").await?;
    shot(
        state,
        desktop,
        name,
        "restored-opened",
        json!({"conditional":retry,"freshOpening":true}),
        stages,
        captures,
    )
    .await?;
    release(state, retry, Ok(unchanged()), requests).await?;
    idle(state).await?;
    edit_label(desktop, "Saved with ", RESTORED_LABEL).await?;
    use_description(state, desktop, "Saved with ").await?;
    shot(
        state,
        desktop,
        name,
        "used",
        json!({"rawTokensRetained":true}),
        stages,
        captures,
    )
    .await?;
    save(state, desktop, Outcome::Saved, 1, 1).await?;
    click(desktop, "Edit description", false).await?;
    editor_label(desktop, "Saved with ", true, RESTORED_LABEL).await?;
    shot(
        state,
        desktop,
        name,
        "confirmed",
        json!({"confirmedSave":true,"newOpeningLabel":true}),
        stages,
        captures,
    )
    .await
}
