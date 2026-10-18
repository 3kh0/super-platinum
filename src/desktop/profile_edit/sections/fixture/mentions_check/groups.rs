//! Extra witnessed group labels, within the same closed save/readback case.
use super::*;
pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    click(desktop, "Cancel description edit", false).await?;
    mentions::group_event(&mut state.write(), mentions::group_raw(""))?;
    click(desktop, "Edit description", false).await?;
    editor_label(desktop, "Saved with ", true, "").await?;
    let limit = "x".repeat(5000);
    let excess = "x".repeat(5001);
    edit_label(desktop, &limit, "").await?;
    use_description(state, desktop, &limit).await?;
    if mentions::snapshot(&state.peek())["canSave"] != true {
        return Err("Empty group counted toward source length".into());
    }
    click(desktop, "Edit description", false).await?;
    editor_label(desktop, &limit, true, "").await?;
    // Prove the adjacent empty insert contributes zero, but cannot make text
    // beyond the field limit writable. Restore the accepted 5000 before pixels.
    edit_label(desktop, &excess, "").await?;
    use_description(state, desktop, &excess).await?;
    if mentions::snapshot(&state.peek())["canSave"] != false {
        return Err("Empty group allowed 5001 text".into());
    }
    wait_dom(desktop,r#"({ok:document.querySelector('.profile-edit-field-error')?.textContent==="Check this field's value or length."})"#).await?;
    click(desktop, "Edit description", false).await?;
    editor_label(desktop, &excess, true, "").await?;
    edit_label(desktop, &limit, "").await?;
    use_description(state, desktop, &limit).await?;
    click(desktop, "Edit description", false).await?;
    editor_label(desktop, &limit, true, "").await?;
    capture(state,desktop,name,"group-empty",json!({"observedLabel":"","emptyContributesZero":true,"text5000Accepted":true,"text5001Refused":true,"rawTokenCount":2}),stages,captures).await?;
    edit_label(desktop, "Saved with ", "").await?;
    use_description(state, desktop, "Saved with ").await?;
    restore(state, desktop).await?;
    for (stage, label, raw) in [
        ("group-unknown", "@unknown-group", {
            let mut r = mentions::group_raw("ignored-handle");
            r["isUnknown"] = json!(true);
            r
        }),
        ("group-deactivated", mentions::GROUP_HANDLE, {
            let mut r = mentions::group_raw(mentions::GROUP_HANDLE);
            r["date_delete"] = json!(123);
            r["is_visible"] = json!(false);
            r
        }),
    ] {
        click(desktop, "Cancel description edit", false).await?;
        mentions::group_event(&mut state.write(), raw.clone())?;
        let observed = mentions::group_row(&state.peek())
            .and_then(|g| g.label_source.clone())
            .ok_or("Missing group observation")?;
        if observed != raw {
            return Err("Group event lost raw fields".into());
        }
        click(desktop, "Edit description", false).await?;
        editor_label(desktop, "Saved with ", true, label).await?;
        capture(state,desktop,name,stage,json!({"observedLabel":label,"observedRow":observed,"rawTokenCount":2,"originalDraft":true}),stages,captures).await?;
        click(desktop, "Cancel description edit", false).await?;
        restore(state, desktop).await?;
    }
    Ok(())
}
async fn restore(mut state: Signal<ShellState>, desktop: &DesktopContext) -> Result<(), String> {
    mentions::group_event(
        &mut state.write(),
        mentions::group_raw(mentions::GROUP_HANDLE),
    )?;
    click(desktop, "Edit description", false).await?;
    editor(desktop, "Saved with ", true).await
}
