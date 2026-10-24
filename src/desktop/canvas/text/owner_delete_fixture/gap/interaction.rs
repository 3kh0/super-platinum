//! Real owned controls and canonical comparisons for independent gap operations.
use super::*;
async fn open(
    desktop: &dioxus::desktop::DesktopContext,
    id: &str,
    action: &str,
) -> Result<(), String> {
    row(desktop, id, false, true).await?;
    let button = if action == "edit" {
        ".canvas-text-open[aria-label='Edit paragraph']".into()
    } else {
        format!("[data-structure-action={action}]")
    };
    click(
        desktop,
        &format!("[data-canvas-text-section={}] {button}", json!(id)),
    )
    .await?;
    check::editor_ready(desktop, action == "delete").await?;
    editor_layout(desktop).await?;
    Ok(())
}
async fn pending(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    text: bool,
) -> Result<(), String> {
    wait(desktop,r#"({ok:document.getElementById('canvas-text-save')?.disabled&&document.getElementById('canvas-text-cancel')?.disabled&&document.getElementById('canvas-text-save')?.textContent.includes('Saving')})"#).await?;
    for _ in 0..600 {
        let ready = {
            let mut shell = state.write();
            let f = fixture(&mut shell);
            if text {
                f.pending_text.is_some()
            } else {
                f.pending.is_some()
            }
        };
        if ready {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err("Owned preparation did not dispatch".into())
}
async fn save(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    text: bool,
) -> Result<(), String> {
    let (writes, reservations) = {
        let mut shell = state.write();
        let f = fixture(&mut shell);
        (f.writes.len(), f.reservations)
    };
    checked(desktop,r#"(()=>{const b=document.getElementById('canvas-text-save');if(!b||b.disabled)return {ok:false};b.click();b.click();return {ok:true};})()"#).await?;
    pending(state, desktop, text).await?;
    let correct = {
        let mut shell = state.write();
        let f = fixture(&mut shell);
        f.writes.len() == writes + 1 && f.reservations == reservations + usize::from(!text)
    };
    if !correct || (!text && recovery::stored().await?.is_none()) {
        return Err("Duplicate save/custody guard failed".into());
    }
    Ok(())
}
async fn finish(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    text: bool,
) -> Result<(), String> {
    if text {
        completion::complete_text(state).await?;
    } else {
        completion::complete(state, false).await?;
    }
    wait(
        desktop,
        r#"({ok:!document.getElementById('canvas-text-editor')})"#,
    )
    .await?;
    if recovery::stored().await?.is_some() {
        return Err("Confirmed structural receipt not retired".into());
    }
    Ok(())
}
async fn edit(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    id: &str,
    name: &str,
    text: &str,
    kind: &str,
    actual_bold: bool,
    ordinal: &mut usize,
) -> Result<(), String> {
    let before = snapshot(&state.peek());
    open(desktop, id, "edit").await?;
    let rtml = markup(text, kind);
    set_draft(
        desktop,
        &format!(
            "<div>{}</div>",
            if actual_bold {
                escaped(text)
            } else {
                rtml.clone()
            }
        ),
    )
    .await?;
    if actual_bold {
        check::bold(desktop, text).await?;
    }
    save(state, desktop, true).await?;
    capture(
        state,
        &format!("{name}-saving"),
        ordinal,
        editor_layout(desktop).await?,
    )
    .await?;
    finish(state, desktop, true).await?;
    let after = snapshot(&state.peek());
    unchanged(&before, &after, id)?;
    canonical(&after, id, &rtml)?;
    capture(
        state,
        &format!("{name}-confirmed"),
        ordinal,
        row(desktop, id, true, true).await?,
    )
    .await
}
async fn delete(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    id: &str,
    name: &str,
    next: &str,
    next_delete: bool,
    ordinal: &mut usize,
) -> Result<(), String> {
    let before = snapshot(&state.peek());
    open(desktop, id, "delete").await?;
    save(state, desktop, false).await?;
    capture(
        state,
        &format!("{name}-delete-saving"),
        ordinal,
        editor_layout(desktop).await?,
    )
    .await?;
    finish(state, desktop, false).await?;
    let after = snapshot(&state.peek());
    unchanged(&before, &after, id)?;
    if after.section(id).and_then(|s| s.deleted().ok().flatten()) != Some(true) {
        return Err("Deleted row tombstone missing".into());
    }
    capture(
        state,
        &format!("{name}-deleted"),
        ordinal,
        row(desktop, next, false, next_delete).await?,
    )
    .await
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    if !matches!(side().as_str(), "after-owner" | "before-child")
        || !matches!(count(), 1 | 3)
        || !matches!(rich().as_str(), "none" | "bold" | "link")
        || !matches!(
            phase().as_str(),
            "success" | "stale" | "unknown" | "restart"
        )
        || !matches!(
            env("SUPER_PLATINUM_CANVAS_SPLIT_GAP_OWNER_DELETE", "0").as_str(),
            "0" | "1"
        )
        || (owner_delete() && count() != 3)
    {
        return Err("Unsupported gap fixture configuration".into());
    }
    if phase() == "restart" {
        return review::run(state, desktop).await;
    }
    let owner = parent(&mode());
    wait(desktop,&format!(r#"({{ok:!!document.querySelector('[data-canvas-text-section="{owner}"] [data-structure-action=after]:not(:disabled)')}})"#)).await?;
    let initial = snapshot(&state.peek());
    let child = seed::child(&initial, owner);
    let mut ordinal = 0;
    capture(
        state,
        "initial",
        &mut ordinal,
        row(desktop, owner, false, true).await?,
    )
    .await?;
    let (anchor, action) = if side() == "after-owner" {
        (owner, "after")
    } else {
        (child.as_str(), "before")
    };
    open(desktop, anchor, action).await?;
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
        return Err("Cancel changed source/custody".into());
    }
    let mut gaps = Vec::new();
    for i in 1..=count() {
        let before = snapshot(&state.peek());
        open(desktop, anchor, action).await?;
        set_draft(desktop, &format!("<div>{}</div>", escaped(&text(i)))).await?;
        capture(
            state,
            &format!("gap-{i}-draft"),
            &mut ordinal,
            editor_layout(desktop).await?,
        )
        .await?;
        if phase() == "stale" {
            checked(desktop,r#"(()=>{const b=document.getElementById('canvas-text-save');b.click();b.click();return {ok:true};})()"#).await?;
            wait(desktop,r#"(()=>{const e=document.querySelector('#canvas-text-editor > p[role=alert]'),r=e?.getBoundingClientRect(),c=document.getElementById('canvas-text-cancel');return {ok:!!r&&r.width>0&&r.height>0&&getComputedStyle(e).visibility==='visible'&&e.textContent==='This paragraph or your access changed. Cancel and refresh.'&&!!c&&!c.disabled};})()"#).await?;
            if !fixture(&mut state.write()).writes.is_empty()
                || recovery::stored().await?.is_some()
                || !Arc::ptr_eq(&initial, &snapshot(&state.peek()))
            {
                return Err("Stale source dispatched".into());
            }
            capture(state, "stale", &mut ordinal, editor_layout(desktop).await?).await?;
            return terminal(state, desktop, ordinal).await;
        }
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
        capture(
            state,
            &format!("gap-{i}-saving"),
            &mut ordinal,
            editor_layout(desktop).await?,
        )
        .await?;
        if phase() == "unknown" {
            completion::complete(state, true).await?;
            wait(desktop,r#"({ok:!!document.getElementById('canvas-text-editor')&&document.body.textContent.includes('Could not confirm')})"#).await?;
            if recovery::stored().await?.is_none() {
                return Err("Unknown insertion lost durable lock".into());
            }
            capture(
                state,
                "unknown",
                &mut ordinal,
                editor_layout(desktop).await?,
            )
            .await?;
            return terminal(state, desktop, ordinal).await;
        }
        finish(state, desktop, false).await?;
        let after = snapshot(&state.peek());
        unchanged(&before, &after, &id)?;
        canonical(&after, &id, &escaped(&text(i)))?;
        wait(desktop,&format!(r#"(()=>{{const e=[...document.querySelectorAll('[data-canvas-text-section]')].find(e=>e.getAttribute('data-canvas-text-section')==={});return {{ok:!!e&&e.contains(document.activeElement)}};}})()"#,json!(anchor))).await?;
        let mut geometry = row(desktop, &id, false, true).await?;
        geometry["returnFocusId"] = json!(anchor);
        geometry["returnFocus"] = json!(true);
        capture(state, &format!("gap-{i}-confirmed"), &mut ordinal, geometry).await?;
        gaps.push(id);
    }
    capture(
        state,
        "owner-mapped",
        &mut ordinal,
        row(desktop, owner, false, true).await?,
    )
    .await?;
    capture(
        state,
        "child-mapped",
        &mut ordinal,
        row(desktop, &child, false, true).await?,
    )
    .await?;
    if count() == 3 {
        edit(
            state,
            desktop,
            &gaps[1],
            "gap-2-edit",
            &format!("{} edited", text(2)),
            "bold",
            true,
            &mut ordinal,
        )
        .await?;
        delete(
            state,
            desktop,
            &gaps[1],
            "gap-2",
            &gaps[0],
            true,
            &mut ordinal,
        )
        .await?;
    }
    edit(
        state,
        desktop,
        owner,
        "owner-edit",
        &format!("{} edited", texts(&corpus()).0),
        "none",
        false,
        &mut ordinal,
    )
    .await?;
    edit(
        state,
        desktop,
        &child,
        "child-edit",
        &format!("{} edited", texts(&corpus()).1),
        &rich(),
        false,
        &mut ordinal,
    )
    .await?;
    edit(
        state,
        desktop,
        &gaps[0],
        "gap-1-edit",
        &format!("{} edited", text(1)),
        "none",
        false,
        &mut ordinal,
    )
    .await?;
    if owner_delete() {
        delete(state, desktop, owner, "owner", &child, true, &mut ordinal).await?;
    }
    terminal(state, desktop, ordinal).await
}
