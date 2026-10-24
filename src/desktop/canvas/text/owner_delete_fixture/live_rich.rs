//! Named offline continuation while a saved split owner remains live.
use super::*;
use crate::canvas::text::check::{checked, click, editor_layout, set_draft, wait};
use std::sync::atomic::{AtomicBool, Ordering};
const DOM: &str = include_str!("live_rich/dom.js");
static STARTED: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
mod tests;
pub(super) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-live-split-rich" | "canvas-live-split-rich-narrow")
        )
}
pub(super) fn cadence() -> String {
    env("SUPER_PLATINUM_CANVAS_LIVE_SPLIT_CADENCE", "revert")
}
fn format() -> String {
    env("SUPER_PLATINUM_CANVAS_LIVE_SPLIT_FORMAT", "bold")
}
fn owner_action() -> String {
    env("SUPER_PLATINUM_CANVAS_LIVE_SPLIT_OWNER_ACTION", "none")
}
fn link_error() -> bool {
    env("SUPER_PLATINUM_CANVAS_LIVE_SPLIT_LINK_ERROR", "0") == "1"
}
fn config() -> Value {
    let mut value = json!({"corpus":corpus(),"history":mode(),"cadence":cadence(),"format":format(),"ownerAction":owner_action(),"phase":phase()});
    if link_error() {
        value["linkError"] = json!(true);
    }
    value
}
fn child_text() -> String {
    let tail = texts(&corpus()).1;
    if cadence() == "typed" {
        format!("{tail} typed")
    } else {
        tail.to_owned()
    }
}
fn script(call: &str) -> String {
    format!("{DOM}\nwindow.superPlatinumLiveRichCheck.{call}")
}
pub(super) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !requested()
        || env("SUPER_PLATINUM_CANVAS_LIVE_SPLIT_RICH_CHECK", "0") != "1"
        || STARTED.swap(true, Ordering::Relaxed)
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = match run(state, &desktop).await {
            Ok(v) => v,
            Err(error) => json!({"ok":false,"error":error}),
        };
        eprintln!("canvas-live-split-rich-check: {result}");
    });
}
pub(super) fn add_metadata(evidence: &mut Value) {
    evidence["liveRich"] = config();
}
async fn capture(
    state: Signal<ShellState>,
    name: &str,
    ordinal: &mut usize,
    geometry: Value,
) -> Result<(), String> {
    check::stage(state, name, *ordinal, geometry).await?;
    *ordinal += 1;
    Ok(())
}
async fn row(
    desktop: &dioxus::desktop::DesktopContext,
    id: &str,
    focus: bool,
    can_delete: bool,
) -> Result<Value, String> {
    wait(desktop, &script(&format!("show({})", json!(id)))).await?;
    check::row_actions(desktop, id, focus, can_delete).await
}
async fn open(desktop: &dioxus::desktop::DesktopContext, id: &str) -> Result<(), String> {
    click(
        desktop,
        &format!(
            "[data-canvas-text-section={}] .canvas-text-open[aria-label='Edit paragraph']",
            json!(id)
        ),
    )
    .await?;
    check::editor_ready(desktop, false).await?;
    editor_layout(desktop).await?;
    Ok(())
}
async fn format_child(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    remove: bool,
    ordinal: &mut usize,
) -> Result<(), String> {
    let kind = format();
    let text = child_text();
    checked(desktop, &script(&format!("select({})", json!(text)))).await?;
    checked(desktop, &script(&format!("toolbar({})", json!(kind)))).await?;
    if kind == "link" {
        let geometry = wait(desktop, &script(&format!("linkReady({remove})"))).await?;
        capture(
            state,
            if remove {
                "remove-link-form"
            } else {
                "link-form"
            },
            ordinal,
            geometry,
        )
        .await?;
        if link_error() {
            let geometry = checked(desktop, &script(&format!("invalidLink({remove})"))).await?;
            capture(
                state,
                if remove {
                    "remove-link-error"
                } else {
                    "link-error"
                },
                ordinal,
                geometry,
            )
            .await?;
        }
        checked(desktop, &script(&format!("applyLink({remove})"))).await?;
    }
    wait(
        desktop,
        &script(&format!(
            "formatted({},{remove},{})",
            json!(kind),
            json!(text)
        )),
    )
    .await?;
    Ok(())
}
async fn pending_text(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<(), String> {
    wait(desktop,r#"({ok:document.getElementById('canvas-text-save')?.disabled&&document.getElementById('canvas-text-cancel')?.disabled&&document.getElementById('canvas-text-save')?.textContent.includes('Saving')})"#).await?;
    for _ in 0..600 {
        if fixture(&mut state.write()).pending_text.is_some() {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err("Text preparation did not dispatch".into())
}
async fn finish_text(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<(), String> {
    completion::complete_text(state).await?;
    wait(
        desktop,
        r#"({ok:!document.getElementById('canvas-text-editor')})"#,
    )
    .await?;
    Ok(())
}
fn immutable_except(
    before: &CanvasCollabSnapshot,
    after: &CanvasCollabSnapshot,
    target: &str,
) -> Result<(), String> {
    if before.sections.len() != after.sections.len()
        || before
            .sections
            .iter()
            .any(|s| s.id() != target && after.section(s.id()) != Some(s))
    {
        return Err("Independent operation changed another raw row".into());
    }
    Ok(())
}
fn canonical(snapshot: &CanvasCollabSnapshot, id: &str, expected: &str) -> Result<(), String> {
    let body = snapshot
        .section(id)
        .ok_or("Canonical row absent")?
        .raw()
        .message(12)
        .map_err(|_| "Invalid canonical body")?
        .ok_or("Missing canonical body")?
        .message(1)
        .map_err(|_| "Invalid canonical text")?
        .ok_or("Missing canonical text")?;
    if body.string(1).map_err(|_| "Invalid canonical RTML")? != Some(expected) {
        return Err("Canonical rich text differs".into());
    }
    Ok(())
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    if (link_error() && format() != "link")
        || !matches!(
            env("SUPER_PLATINUM_CANVAS_LIVE_SPLIT_LINK_ERROR", "0").as_str(),
            "0" | "1"
        )
        || !matches!(format().as_str(), "bold" | "link")
        || !matches!(cadence().as_str(), "typed" | "revert")
        || !matches!(owner_action().as_str(), "none" | "edit" | "delete")
        || !matches!(phase().as_str(), "success" | "stale")
    {
        return Err("Unsupported live rich fixture configuration".into());
    }
    let owner = parent(&mode());
    wait(desktop,&format!(r#"({{ok:!!document.querySelector('[data-canvas-text-section="{owner}"] .canvas-text-open:not(:disabled)')}})"#)).await?;
    let initial = snapshot(&state.peek());
    let child = seed::child(&initial, owner);
    let text = child_text();
    let plain = escaped(&text);
    let rich = if format() == "bold" {
        format!("<b>{plain}</b>")
    } else {
        format!("<a href=\"https://example.test/\">{plain}</a>")
    };
    canonical(&initial, &child, &plain)?;
    let mut ordinal = 0;
    capture(
        state,
        "initial",
        &mut ordinal,
        row(desktop, &child, false, true).await?,
    )
    .await?;
    // One real Cancel proves opening the restored live child is side-effect free.
    open(desktop, &child).await?;
    click(desktop, "#canvas-text-cancel").await?;
    wait(
        desktop,
        r#"({ok:!document.getElementById('canvas-text-editor')})"#,
    )
    .await?;
    if !fixture(&mut state.write()).writes.is_empty()
        || !Arc::ptr_eq(&initial, &snapshot(&state.peek()))
    {
        return Err("Cancel changed saved pair".into());
    }
    open(desktop, &child).await?;
    format_child(state, desktop, false, &mut ordinal).await?;
    capture(
        state,
        "child-draft",
        &mut ordinal,
        editor_layout(desktop).await?,
    )
    .await?;
    checked(desktop,r#"(()=>{const b=document.getElementById('canvas-text-save');b.click();b.click();return {ok:true};})()"#).await?;
    if phase() == "stale" {
        wait(desktop,r#"(()=>{const root=document.getElementById('canvas-text-editor'),e=root?.querySelector(':scope > p[role="alert"]'),c=root?.querySelector('#canvas-text-cancel'),r=e?.getBoundingClientRect();return {ok:!!r&&r.width>0&&r.height>0&&getComputedStyle(e).visibility==='visible'&&e.textContent==='This text changed before saving.'&&!!c&&!c.disabled};})()"#).await?;
        if !fixture(&mut state.write()).writes.is_empty()
            || !Arc::ptr_eq(&initial, &snapshot(&state.peek()))
            || recovery::stored().await?.is_some()
        {
            return Err("Stale rich edit changed source/custody".into());
        }
        capture(state, "stale", &mut ordinal, editor_layout(desktop).await?).await?;
    } else {
        pending_text(state, desktop).await?;
        if fixture(&mut state.write()).writes.len() != 1 {
            return Err("Duplicate rich save dispatched".into());
        }
        capture(
            state,
            "child-saving",
            &mut ordinal,
            editor_layout(desktop).await?,
        )
        .await?;
        finish_text(state, desktop).await?;
        let rich_source = snapshot(&state.peek());
        immutable_except(&initial, &rich_source, &child)?;
        canonical(&rich_source, &child, &rich)?;
        capture(
            state,
            "child-confirmed",
            &mut ordinal,
            row(desktop, &child, true, true).await?,
        )
        .await?;
        capture(
            state,
            "owner-mapped",
            &mut ordinal,
            row(desktop, owner, false, true).await?,
        )
        .await?;
        if owner_action() == "delete" {
            let selector = format!(
                "[data-canvas-text-section={}] [data-structure-action=delete]",
                json!(owner)
            );
            click(desktop, &selector).await?;
            check::editor_ready(desktop, true).await?;
            capture(
                state,
                "owner-confirmation",
                &mut ordinal,
                editor_layout(desktop).await?,
            )
            .await?;
            checked(desktop,r#"(()=>{const b=document.getElementById('canvas-text-save');b.click();b.click();return {ok:true};})()"#).await?;
            for _ in 0..600 {
                if fixture(&mut state.write()).pending.is_some() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            if fixture(&mut state.write()).pending.is_none()
                || fixture(&mut state.write()).reservations != 1
                || fixture(&mut state.write()).writes.len() != 2
                || recovery::stored().await?.is_none()
            {
                return Err("Owner deletion did not retain one dispatched receipt".into());
            }
            capture(
                state,
                "owner-saving",
                &mut ordinal,
                editor_layout(desktop).await?,
            )
            .await?;
            completion::complete(state, false).await?;
            wait(
                desktop,
                r#"({ok:!document.getElementById('canvas-text-editor')})"#,
            )
            .await?;
            let deleted = snapshot(&state.peek());
            immutable_except(&rich_source, &deleted, owner)?;
            canonical(&deleted, &child, &rich)?;
            if deleted.section(owner).unwrap().deleted().unwrap() != Some(true)
                || recovery::stored().await?.is_some()
            {
                return Err("Owner tombstone/custody not confirmed".into());
            }
            capture(
                state,
                "owner-deleted",
                &mut ordinal,
                row(desktop, &child, true, true).await?,
            )
            .await?;
        } else {
            if owner_action() == "edit" {
                open(desktop, owner).await?;
                let desired = format!("{} edited", texts(&corpus()).0);
                set_draft(desktop, &format!("<div>{}</div>", escaped(&desired))).await?;
                capture(
                    state,
                    "owner-draft",
                    &mut ordinal,
                    editor_layout(desktop).await?,
                )
                .await?;
                click(desktop, "#canvas-text-save").await?;
                pending_text(state, desktop).await?;
                capture(
                    state,
                    "owner-saving",
                    &mut ordinal,
                    editor_layout(desktop).await?,
                )
                .await?;
                finish_text(state, desktop).await?;
                let edited = snapshot(&state.peek());
                immutable_except(&rich_source, &edited, owner)?;
                canonical(&edited, owner, &escaped(&desired))?;
                capture(
                    state,
                    "owner-confirmed",
                    &mut ordinal,
                    row(desktop, owner, true, true).await?,
                )
                .await?;
            }
            let before_remove = snapshot(&state.peek());
            open(desktop, &child).await?;
            format_child(state, desktop, true, &mut ordinal).await?;
            capture(
                state,
                "remove-draft",
                &mut ordinal,
                editor_layout(desktop).await?,
            )
            .await?;
            click(desktop, "#canvas-text-save").await?;
            pending_text(state, desktop).await?;
            capture(
                state,
                "remove-saving",
                &mut ordinal,
                editor_layout(desktop).await?,
            )
            .await?;
            finish_text(state, desktop).await?;
            let removed = snapshot(&state.peek());
            immutable_except(&before_remove, &removed, &child)?;
            canonical(&removed, &child, &plain)?;
            capture(
                state,
                "remove-confirmed",
                &mut ordinal,
                row(desktop, &child, true, true).await?,
            )
            .await?;
        }
    }
    let source = snapshot(&state.peek());
    let canonical = tokio::task::spawn_blocking(move || seed::saved(&source))
        .await
        .map_err(|_| "Evidence worker failed")?;
    Ok(
        json!({"ok":true,"noSession":true,"noTransport":true,"noHttp":true,"liveRich":config(),"stages":ordinal,"cancelNoWrite":true,"duplicateGuard":true,"layout":check::layout(desktop).await?,"canonical":canonical,"journal":recovery::observe().await?}),
    )
}
