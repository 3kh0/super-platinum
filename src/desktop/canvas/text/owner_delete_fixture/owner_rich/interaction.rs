//! Native controls apply, remove and independently continue the saved owner.
use super::*;
async fn later(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    id: &str,
    name: &str,
    value: &str,
    ordinal: &mut usize,
) -> Result<(), String> {
    let before = snapshot(&state.peek());
    open(desktop, id).await?;
    set_draft(desktop, &format!("<div>{}</div>", escaped(value))).await?;
    save(state, desktop, true).await?;
    capture(
        state,
        &format!("{name}-later-saving"),
        ordinal,
        editor_layout(desktop).await?,
    )
    .await?;
    finish(state, desktop).await?;
    let after = snapshot(&state.peek());
    unchanged(&before, &after, id)?;
    canonical(&after, id, &escaped(value))?;
    capture(
        state,
        &format!("{name}-later-confirmed"),
        ordinal,
        row(desktop, id, true, true).await?,
    )
    .await
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    if !matches!(cadence().as_str(), "correction" | "revert" | "erase")
        || !matches!(
            phase().as_str(),
            "success" | "stale" | "unknown" | "restart"
        )
        || !matches!(
            env("SUPER_PLATINUM_CANVAS_COALESCED_OWNER_GAP", "0").as_str(),
            "0" | "1"
        )
    {
        return Err("Unsupported owner rich configuration".into());
    }
    if phase() == "restart" {
        return review::run(state, desktop).await;
    }
    let owner = parent(&mode());
    wait(desktop,&format!(r#"({{ok:!!document.querySelector('[data-canvas-text-section="{owner}"] .canvas-text-open:not(:disabled)')}})"#)).await?;
    let initial = snapshot(&state.peek());
    let child = seed::child(&initial, owner);
    let gap = gap_id(&initial, owner, &child);
    if gap.is_some() != has_gap() {
        return Err("Independent gap seed differs".into());
    }
    let mut ordinal = 0;
    capture(
        state,
        "initial",
        &mut ordinal,
        initial_row(desktop, owner).await?,
    )
    .await?;
    open(desktop, owner).await?;
    click(desktop, "#canvas-text-cancel").await?;
    wait(
        desktop,
        r#"({ok:!document.getElementById('canvas-text-editor')})"#,
    )
    .await?;
    if !fixture(&mut state.write()).writes.is_empty()
        || !Arc::ptr_eq(&initial, &snapshot(&state.peek()))
        || recovery::stored().await?.is_some()
    {
        return Err("Cancel changed original source or custody".into());
    }
    open(desktop, owner).await?;
    apply_format(state, desktop, false, &mut ordinal).await?;
    capture(
        state,
        "owner-draft",
        &mut ordinal,
        editor_layout(desktop).await?,
    )
    .await?;
    if phase() == "stale" {
        checked(desktop,r#"(()=>{const b=document.getElementById('canvas-text-save');b.click();b.click();return {ok:true};})()"#).await?;
        wait(desktop,r#"(()=>{const root=document.getElementById('canvas-text-editor'),e=root?.querySelector(':scope > p[role="alert"]'),c=root?.querySelector('#canvas-text-cancel'),r=e?.getBoundingClientRect();return {ok:!!r&&r.width>0&&r.height>0&&getComputedStyle(e).visibility==='visible'&&e.textContent==='This text changed before saving.'&&!!c&&!c.disabled};})()"#).await?;
        if !fixture(&mut state.write()).writes.is_empty()
            || !Arc::ptr_eq(&initial, &snapshot(&state.peek()))
            || recovery::stored().await?.is_some()
        {
            return Err("Stale owner formatting dispatched".into());
        }
        capture(state, "stale", &mut ordinal, editor_layout(desktop).await?).await?;
        return terminal(state, desktop, ordinal).await;
    }
    save(state, desktop, true).await?;
    capture(
        state,
        "owner-saving",
        &mut ordinal,
        editor_layout(desktop).await?,
    )
    .await?;
    finish(state, desktop).await?;
    let formatted = snapshot(&state.peek());
    unchanged(&initial, &formatted, owner)?;
    canonical(&formatted, owner, &rich_text())?;
    capture(
        state,
        "owner-confirmed",
        &mut ordinal,
        row(desktop, owner, true, true).await?,
    )
    .await?;
    capture(
        state,
        "child-mapped",
        &mut ordinal,
        row(desktop, &child, false, true).await?,
    )
    .await?;
    if let Some(id) = &gap {
        capture(
            state,
            "gap-mapped",
            &mut ordinal,
            row(desktop, id, false, true).await?,
        )
        .await?;
    }
    if phase() == "unknown" {
        row(desktop, owner, false, true).await?;
        click(
            desktop,
            &format!(
                "[data-canvas-text-section={}] [data-structure-action=delete]",
                json!(owner)
            ),
        )
        .await?;
        check::editor_ready(desktop, true).await?;
        capture(
            state,
            "owner-delete-confirmation",
            &mut ordinal,
            editor_layout(desktop).await?,
        )
        .await?;
        save(state, desktop, false).await?;
        if recovery::stored().await?.is_none() || fixture(&mut state.write()).reservations != 1 {
            return Err("Unknown owner deletion did not reserve custody".into());
        }
        capture(
            state,
            "owner-delete-saving",
            &mut ordinal,
            editor_layout(desktop).await?,
        )
        .await?;
        completion::complete(state, true).await?;
        let current = snapshot(&state.peek());
        unchanged(&formatted, &current, owner)?;
        if current
            .section(owner)
            .and_then(|s| s.deleted().ok().flatten())
            != Some(true)
            || recovery::stored().await?.is_none()
        {
            return Err("Unknown deletion lost tombstone or receipt".into());
        }
        wait(desktop,r#"({ok:!!document.getElementById('canvas-text-editor')&&document.body.textContent.includes('Could not confirm')})"#).await?;
        capture(
            state,
            "unknown",
            &mut ordinal,
            editor_layout(desktop).await?,
        )
        .await?;
        return terminal(state, desktop, ordinal).await;
    }
    open(desktop, owner).await?;
    apply_format(state, desktop, true, &mut ordinal).await?;
    capture(
        state,
        "remove-draft",
        &mut ordinal,
        editor_layout(desktop).await?,
    )
    .await?;
    save(state, desktop, true).await?;
    capture(
        state,
        "remove-saving",
        &mut ordinal,
        editor_layout(desktop).await?,
    )
    .await?;
    finish(state, desktop).await?;
    let removed = snapshot(&state.peek());
    unchanged(&formatted, &removed, owner)?;
    canonical(&removed, owner, &escaped(&owner_text()))?;
    capture(
        state,
        "remove-confirmed",
        &mut ordinal,
        row(desktop, owner, true, true).await?,
    )
    .await?;
    later(
        state,
        desktop,
        owner,
        "owner",
        &format!("{} edited", owner_text()),
        &mut ordinal,
    )
    .await?;
    later(
        state,
        desktop,
        &child,
        "child",
        &format!("{} edited", texts(&corpus()).1),
        &mut ordinal,
    )
    .await?;
    if let Some(id) = &gap {
        later(
            state,
            desktop,
            id,
            "gap",
            &format!("{} edited", gap_text()),
            &mut ordinal,
        )
        .await?;
    }
    terminal(state, desktop, ordinal).await
}
