use super::*;
use capture::{Record, click, model, open, row, save};
pub(super) mod failure;
pub(super) async fn reserved(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<(), String> {
    for _ in 0..500 {
        if state
            .peek()
            .canvas_checklist
            .structure_fixture
            .as_ref()
            .is_some_and(|f| f.reserved_replacement.is_some())
        {
            let receipt = recovery::stored()
                .await?
                .ok_or("Pair reservation missing journal")?;
            if !receipt.checkpoint().is_replacement() {
                return Err("Reservation is not paired".into());
            }
            wait(desktop,r#"({ok:document.getElementById('canvas-text-save')?.disabled&&!document.getElementById('canvas-text-cancel')?.disabled})"#).await?;
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    Err("Pair reservation timed out".into())
}
pub(super) async fn saving(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    wait(desktop,r#"({ok:document.getElementById('canvas-text-save')?.disabled&&document.getElementById('canvas-text-cancel')?.disabled})"#).await?;
    // Disabled replay is observed without invoking an alternate action API.
    checked(desktop,r#"(()=>{const s=document.getElementById('canvas-text-save');s.click();return{ok:s.disabled}})()"#).await?;
    Ok(())
}
fn unchanged_retained(
    before: &CanvasCollabSnapshot,
    after: &CanvasCollabSnapshot,
    target: &str,
) -> Result<(), String> {
    if after.sections.len() != before.sections.len() + 1
        || before
            .sections
            .iter()
            .filter(|s| s.id() != target)
            .any(|s| after.section(s.id()) != Some(s))
        || after
            .section(target)
            .and_then(|s| s.deleted().ok().flatten())
            != Some(true)
    {
        return Err("Pair replacement changed retained canonical bytes/inventory".into());
    }
    Ok(())
}
pub(super) async fn cycle(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    record: &mut Record,
    target: &str,
    repeat: bool,
) -> Result<String, String> {
    let source = snapshot(&state.peek());
    let prefix = if repeat { "repeat-" } else { "" };
    let action = open(desktop, target, true).await?;
    if repeat {
        record
            .shot(
                state,
                desktop,
                "repeat-confirmation",
                editor_layout(desktop).await?,
                action,
            )
            .await?;
    }
    let action = save(desktop).await?;
    reserved(state, desktop).await?;
    record
        .shot(
            state,
            desktop,
            &format!("{prefix}reserved"),
            editor_layout(desktop).await?,
            action,
        )
        .await?;
    let id = pair::release(state).await?;
    saving(desktop).await?;
    record
        .shot(
            state,
            desktop,
            &format!("{prefix}saving"),
            editor_layout(desktop).await?,
            json!({"replacement":id}),
        )
        .await?;
    let attempt = pair::complete(state, "success").await?;
    let read = pair::read_and_settle(state, false).await?;
    if checklist::text_ops::settled(&state.peek(), &attempt) || recovery::stored().await?.is_none()
    {
        return Err("Pair unlocked before durable retirement".into());
    }
    wait(desktop,r#"({ok:document.getElementById('canvas-text-save')?.textContent==='Checking…'&&document.getElementById('canvas-text-cancel')?.disabled})"#).await?;
    record
        .shot(
            state,
            desktop,
            &format!("{prefix}accepted-locked"),
            editor_layout(desktop).await?,
            Value::Null,
        )
        .await?;
    checklist::recovery::settle(state, &read).await;
    if !checklist::text_ops::settled(&state.peek(), &attempt) || recovery::stored().await?.is_some()
    {
        return Err("Confirmed pair did not retire exact journal".into());
    }
    pair::retain_confirmed(state).await?;
    wait(
        desktop,
        r#"({ok:!document.getElementById('canvas-text-editor')})"#,
    )
    .await?;
    let after = snapshot(&state.peek());
    unchanged_retained(&source, &after, target)?;
    owner_first::canonical(&after, &id, "")?;
    record
        .shot(
            state,
            desktop,
            if repeat { "repeated" } else { "replaced" },
            row(desktop, &id, true).await?,
            json!({"target":target,"replacement":id,"allOtherRowsExact":true}),
        )
        .await?;
    // The two programmatic click calls generated exactly one pair each.
    let mut shell = state.write();
    let f = fixture(&mut shell);
    if f.writes
        .iter()
        .filter(|w| w["kind"] == "replacement")
        .count()
        != if repeat && !confirmed::requested() {
            2
        } else {
            1
        }
    {
        return Err("Duplicate Save produced extra pair".into());
    }
    Ok(id)
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    record: &mut Record,
) -> Result<Value, String> {
    if !matches!(
        case().as_str(),
        "success" | "partial" | "missing" | "mutated" | "retired" | "unknown" | "restart"
    ) {
        return Err("Unknown retained replacement case".into());
    }
    record.ready().await?;
    wait(
        desktop,
        r#"({ok:!!document.querySelector('[data-canvas-text-section]')})"#,
    )
    .await?;
    for _ in 0..500 {
        if enabled(&state.peek())
            && checklist::presentation(&state.peek())
                .is_some_and(|v| v.snapshot.is_some() && !v.loading)
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    if case() == "restart" {
        return failure::restart(state, desktop, record).await;
    }
    let initial = snapshot(&state.peek());
    let child = seed::child(&initial, "paragraph");
    if initial.sections.len() != 4
        || ["paragraph", child.as_str()]
            .iter()
            .any(|id| initial.section(id).and_then(|s| s.deleted().ok().flatten()) != Some(true))
    {
        return Err("Initial two retained tombstones missing".into());
    }
    record
        .shot(
            state,
            desktop,
            "initial",
            row(desktop, "neighbor", false).await?,
            json!({"owner":"paragraph","child":child,"survivor":"neighbor"}),
        )
        .await?;
    let action = open(desktop, "neighbor", true).await?;
    record
        .shot(
            state,
            desktop,
            "confirmation",
            editor_layout(desktop).await?,
            action,
        )
        .await?;
    let before = model(state).await?;
    let action = click(desktop, "#canvas-text-cancel").await?;
    wait(
        desktop,
        r#"({ok:!document.getElementById('canvas-text-editor')})"#,
    )
    .await?;
    if model(state).await? != before
        || !same_source(&state.peek(), &initial)
        || recovery::stored().await?.is_some()
    {
        return Err("Cancel changed source/journal/request evidence".into());
    }
    record
        .shot(
            state,
            desktop,
            "cancelled",
            row(desktop, "neighbor", true).await?,
            action,
        )
        .await?;
    if case() != "success" {
        return failure::run(state, desktop, record).await;
    }
    let first = cycle(state, desktop, record, "neighbor", false).await?;
    edit(state, desktop, record, &first).await?;
    let second = cycle(state, desktop, record, &first, true).await?;
    if recovery::stored().await?.is_some() {
        return Err("Successful final edit retained journal".into());
    }
    let mut shell = state.write();
    let f = fixture(&mut shell);
    if f.writes.len() != 3 || f.acknowledgements.len() != 3 {
        return Err("Unexpected final pair/text counts".into());
    }
    Ok(
        json!({"cancelNoWrite":true,"duplicateGuard":true,"pairs":2,"textEdits":1,"allRetainedRowsExact":true,"firstReplacement":first,"finalReplacement":second}),
    )
}

pub(super) async fn edit(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    record: &mut Record,
    first: &str,
) -> Result<(), String> {
    let before_edit = snapshot(&state.peek());
    let action = open(desktop, &first, false).await?;
    let input = checked(desktop,&format!(r#"(()=>{{{}const e=document.getElementById('canvas-text-draft');if(!owned(e)||!e.isContentEditable)return{{ok:false}};e.focus();e.innerHTML='<div>Replacement <strong>edited</strong></div>';e.dispatchEvent(new InputEvent('input',{{bubbles:true,inputType:'insertText'}}));return{{ok:e.textContent==='Replacement edited',visibleHitTested:true,html:e.innerHTML}};}})()"#,crate::app_home::inputs::external::fixture::capture::ACTION_GUARD)).await?;
    record
        .shot(
            state,
            desktop,
            "edit-draft",
            editor_layout(desktop).await?,
            json!({"open":action,"input":input}),
        )
        .await?;
    let action = save(desktop).await?;
    for _ in 0..500 {
        if fixture(&mut state.write()).pending_text.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    if fixture(&mut state.write()).pending_text.is_none() {
        return Err("Text save missing real session worker slot".into());
    }
    saving(desktop).await?;
    record
        .shot(
            state,
            desktop,
            "edit-saving",
            editor_layout(desktop).await?,
            action,
        )
        .await?;
    completion::complete_text(state).await?;
    pair::read_and_settle(state, true).await?;
    pair::retain_confirmed(state).await?;
    wait(
        desktop,
        r#"({ok:!document.getElementById('canvas-text-editor')})"#,
    )
    .await?;
    let edited = snapshot(&state.peek());
    owner_first::unchanged(&before_edit, &edited, &first)?;
    owner_first::canonical(&edited, &first, "Replacement <b>edited</b>")?;
    record
        .shot(
            state,
            desktop,
            "edited",
            row(desktop, &first, true).await?,
            json!({"replacement":first,"allOtherRowsExact":true}),
        )
        .await?;
    Ok(())
}
