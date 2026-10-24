//! Actual Delete/Cancel/Save and surviving-owner rich toolbar interactions.
use super::*;
pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    if !valid() {
        return Err("Unsupported child deletion configuration".into());
    }
    if phase() == "restart" {
        return review::run(state, desktop).await;
    }
    let owner = parent(&mode());
    wait(desktop,&format!(r#"({{ok:!!document.querySelector('[data-canvas-text-section="{owner}"] button:not(:disabled)')}})"#)).await?;
    let initial = snapshot(&state.peek());
    let child = seed::child(&initial, owner);
    let mut ordinal = 0;
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
            return Err("Stale child deletion dispatched".into());
        }
        capture(state, "stale", &mut ordinal, editor_layout(desktop).await?).await?;
        return terminal(state, desktop, ordinal).await;
    }
    save(state, desktop, false).await?;
    if recovery::stored().await?.is_none() || fixture(&mut state.write()).reservations != 1 {
        return Err("Child deletion did not reserve exact custody".into());
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
    if deleted
        .section(&child)
        .and_then(|s| s.deleted().ok().flatten())
        != Some(true)
    {
        return Err("Child tombstone not retained".into());
    }
    wait(
        desktop,
        &format!(r#"({{ok:!document.querySelector('[data-canvas-text-section="{child}"]')}})"#),
    )
    .await?;
    capture(
        state,
        "deleted",
        &mut ordinal,
        row(desktop, owner, true, true).await?,
    )
    .await?;
    let before_owner = if variant() == "plain" {
        add_after(state, desktop, owner, &mut ordinal).await?;
        snapshot(&state.peek())
    } else {
        deleted.clone()
    };
    open(desktop, owner, false).await?;
    capture(
        state,
        "owner-editing",
        &mut ordinal,
        editor_layout(desktop).await?,
    )
    .await?;
    let edited = edited_text();
    set_draft(desktop, &format!("<div>{}</div>", escaped(&edited))).await?;
    capture(
        state,
        "owner-draft",
        &mut ordinal,
        editor_layout(desktop).await?,
    )
    .await?;
    save(state, desktop, true).await?;
    capture(
        state,
        "owner-saving",
        &mut ordinal,
        editor_layout(desktop).await?,
    )
    .await?;
    finish(state, desktop, true).await?;
    let plain = snapshot(&state.peek());
    unchanged(&before_owner, &plain, owner)?;
    canonical(&plain, owner, &escaped(&edited))?;
    capture(
        state,
        "owner-confirmed",
        &mut ordinal,
        row(desktop, owner, true, true).await?,
    )
    .await?;
    open(desktop, owner, false).await?;
    checked(desktop, &script(&format!("select({})", json!(edited)))).await?;
    checked(
        desktop,
        &script(&format!("toolbar({})", json!(owner_format()))),
    )
    .await?;
    if owner_format() == "link" {
        let geometry = wait(desktop, &script("linkReady(false)")).await?;
        capture(state, "link-form", &mut ordinal, geometry).await?;
        checked(desktop, &script("applyLink(false)")).await?;
    }
    wait(
        desktop,
        &script(&format!(
            "formatted({},false,{})",
            json!(owner_format()),
            json!(edited)
        )),
    )
    .await?;
    capture(
        state,
        "format-draft",
        &mut ordinal,
        editor_layout(desktop).await?,
    )
    .await?;
    save(state, desktop, true).await?;
    capture(
        state,
        "format-saving",
        &mut ordinal,
        editor_layout(desktop).await?,
    )
    .await?;
    finish(state, desktop, true).await?;
    let formatted = snapshot(&state.peek());
    unchanged(&plain, &formatted, owner)?;
    let rich = if owner_format() == "bold" {
        format!("<b>{}</b>", escaped(&edited))
    } else {
        format!("<a href=\"https://example.test/\">{}</a>", escaped(&edited))
    };
    canonical(&formatted, owner, &rich)?;
    capture(
        state,
        "format-confirmed",
        &mut ordinal,
        row(desktop, owner, true, true).await?,
    )
    .await?;
    terminal(state, desktop, ordinal).await
}

async fn add_after(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    owner: &str,
    ordinal: &mut usize,
) -> Result<(), String> {
    let before = snapshot(&state.peek());
    row(desktop, owner, false, true).await?;
    click(
        desktop,
        &format!(
            "[data-canvas-text-section={}] [data-structure-action=after]",
            json!(owner)
        ),
    )
    .await?;
    check::editor_ready(desktop, false).await?;
    set_draft(desktop, "<div>Independent paragraph</div>").await?;
    capture(state, "add-draft", ordinal, editor_layout(desktop).await?).await?;
    save(state, desktop, false).await?;
    let id = {
        let mut shell = state.write();
        fixture(&mut shell)
            .pending
            .as_ref()
            .unwrap()
            .1
            .target_section()
            .to_owned()
    };
    capture(state, "add-saving", ordinal, editor_layout(desktop).await?).await?;
    finish(state, desktop, false).await?;
    let after = snapshot(&state.peek());
    if after.sections.len() != before.sections.len() + 1
        || before
            .sections
            .iter()
            .any(|s| after.section(s.id()) != Some(s))
    {
        return Err("Insertion changed original owner or retained child".into());
    }
    canonical(&after, &id, "Independent paragraph")?;
    wait(desktop,&format!(r#"(()=>{{const e=[...document.querySelectorAll('[data-canvas-text-section]')].find(e=>e.getAttribute('data-canvas-text-section')==={});return {{ok:!!e&&e.contains(document.activeElement)}};}})()"#,json!(owner))).await?;
    let mut geometry = row(desktop, &id, false, true).await?;
    geometry["returnFocusId"] = json!(owner);
    geometry["returnFocus"] = json!(true);
    capture(state, "add-confirmed", ordinal, geometry).await
}
