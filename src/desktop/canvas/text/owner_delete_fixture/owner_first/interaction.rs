//! Actual remaining-child deletion and unrelated surviving paragraph edit.
use super::*;
pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    if !valid() {
        return Err("Unsupported owner-first pair deletion configuration".into());
    }
    if phase() == "restart" {
        return review::run(state, desktop).await;
    }
    let owner = parent(&mode());
    // Installation seeds the canonical source asynchronously. The target child
    // ID is not known yet, but both approved fixtures retain this ordinary row.
    // Its mapped Edit is published only after the owned seed/read is installed.
    wait(desktop, r#"({ok:!!document.querySelector('[data-canvas-text-section="neighbor"] .canvas-text-open:not(:disabled)')})"#).await?;
    if !enabled(&state.peek()) {
        return Err("Owned fixture changed before initial snapshot".into());
    }
    let initial = snapshot(&state.peek());
    let child = seed::child(&initial, owner);
    wait(desktop,&format!(r#"({{ok:!!document.querySelector('[data-canvas-text-section="{child}"] button:not(:disabled)')}})"#)).await?;
    let mut ordinal = 0;
    if initial
        .section(owner)
        .and_then(|s| s.deleted().ok().flatten())
        != Some(true)
    {
        return Err("Seed owner tombstone missing".into());
    }
    capture(
        state,
        "initial",
        &mut ordinal,
        row(desktop, &child, false, true).await?,
    )
    .await?;
    open(desktop, &child, true).await?;
    capture(
        state,
        "delete-confirmation",
        &mut ordinal,
        editor_layout(desktop).await?,
    )
    .await?;
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
        return Err("Cancel changed source or custody".into());
    }
    capture(
        state,
        "cancelled",
        &mut ordinal,
        row(desktop, &child, true, true).await?,
    )
    .await?;
    open(desktop, &child, true).await?;
    if phase() == "stale" {
        checked(desktop,r#"(()=>{const b=document.getElementById('canvas-text-save');b.click();b.click();return {ok:true};})()"#).await?;
        wait(desktop,r#"(()=>{const root=document.getElementById('canvas-text-editor'),e=root?.querySelector(':scope > p[role="alert"]'),c=root?.querySelector('#canvas-text-cancel'),r=e?.getBoundingClientRect();return {ok:!!r&&r.width>0&&r.height>0&&getComputedStyle(e).visibility==='visible'&&e.textContent==='This paragraph or your access changed. Cancel and refresh.'&&!!c&&!c.disabled};})()"#).await?;
        if !fixture(&mut state.write()).writes.is_empty()
            || !Arc::ptr_eq(&initial, &snapshot(&state.peek()))
            || recovery::stored().await?.is_some()
        {
            return Err("Stale remaining-child deletion dispatched".into());
        }
        capture(state, "stale", &mut ordinal, editor_layout(desktop).await?).await?;
        return terminal(state, desktop, ordinal).await;
    }
    save(state, desktop, false).await?;
    if recovery::stored().await?.is_none() || fixture(&mut state.write()).reservations != 1 {
        return Err("Remaining-child deletion did not reserve exact custody".into());
    }
    capture(
        state,
        "delete-saving",
        &mut ordinal,
        editor_layout(desktop).await?,
    )
    .await?;
    if matches!(phase().as_str(), "unknown" | "omitted") {
        let omitted = phase() == "omitted";
        completion::complete(state, !omitted).await?;
        let current = snapshot(&state.peek());
        if initial
            .sections
            .iter()
            .filter(|s| s.id() != child)
            .any(|s| current.section(s.id()) != Some(s))
            || recovery::stored().await?.is_none()
            || checklist::presentation(&state.peek())
                .and_then(|v| v.operation)
                .is_none()
        {
            return Err("Unconfirmed deletion changed owner/neighbor or lost lock".into());
        }
        if omitted {
            if current.section(&child).is_some() {
                return Err("Omitted child unexpectedly retained".into());
            }
            wait(desktop,r#"({ok:!!document.getElementById('canvas-text-editor')&&document.getElementById('canvas-text-save')?.disabled&&document.getElementById('canvas-text-cancel')?.disabled})"#).await?;
        } else {
            if current
                .section(&child)
                .and_then(|s| s.deleted().ok().flatten())
                != Some(true)
            {
                return Err("Unknown child tombstone missing".into());
            }
            wait(desktop,r#"({ok:!!document.getElementById('canvas-text-editor')&&document.body.textContent.includes('Could not confirm')})"#).await?;
        }
        capture(
            state,
            if omitted { "omitted" } else { "unknown" },
            &mut ordinal,
            editor_layout(desktop).await?,
        )
        .await?;
        return terminal(state, desktop, ordinal).await;
    }
    finish(state, desktop, false).await?;
    let deleted = snapshot(&state.peek());
    unchanged(&initial, &deleted, &child)?;
    if [owner, child.as_str()]
        .iter()
        .any(|id| deleted.section(id).and_then(|s| s.deleted().ok().flatten()) != Some(true))
    {
        return Err("Both exact tombstones must remain".into());
    }
    wait(desktop,&format!(r#"({{ok:!document.querySelector('[data-canvas-text-section="{owner}"]')&&!document.querySelector('[data-canvas-text-section="{child}"]')}})"#)).await?;
    // Committed removal returns focus to the first remaining mapped row. The
    // rich case has an independent gap before the fixed ordinary neighbor.
    let next = if rich_gap() {
        initial
            .sections
            .iter()
            .find(|s| !matches!(s.id(), "title" | "neighbor") && s.id() != owner && s.id() != child)
            .ok_or("Independent gap missing")?
            .id()
            .to_owned()
    } else {
        "neighbor".into()
    };
    capture(
        state,
        "pair-deleted",
        &mut ordinal,
        row(desktop, &next, true, rich_gap()).await?,
    )
    .await?;
    open(desktop, "neighbor", false).await?;
    set_draft(desktop, "<div>Another paragraph edited</div>").await?;
    capture(
        state,
        "remaining-draft",
        &mut ordinal,
        editor_layout(desktop).await?,
    )
    .await?;
    save(state, desktop, true).await?;
    capture(
        state,
        "remaining-saving",
        &mut ordinal,
        editor_layout(desktop).await?,
    )
    .await?;
    finish(state, desktop, true).await?;
    let fresh = snapshot(&state.peek());
    unchanged(&deleted, &fresh, "neighbor")?;
    canonical(&fresh, "neighbor", "Another paragraph edited")?;
    capture(
        state,
        "remaining-confirmed",
        &mut ordinal,
        row(desktop, "neighbor", true, rich_gap()).await?,
    )
    .await?;
    terminal(state, desktop, ordinal).await
}
