//! Four owned processes: paired UI + SQLite/ACK, inert reopen, human local review,
//! archived reopen. Synthetic canonical data/input/ACKs, never Slack transport.
use super::{
    check::{checked, editor_layout, set_draft, wait},
    checklist,
    continuation_check::click,
    continuation_fixture, replacement_fixture as fixture, structure_fixture,
    structure_recovery_check::{reveal_review_controls, review_controls_script},
};
use crate::state::ShellState;
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use super_platinum_core::canvas::edit::structure::recovery::{
    Scope,
    journal::{Journal, Receipt},
};
static STARTED: AtomicBool = AtomicBool::new(false);
static RETIRED: AtomicUsize = AtomicUsize::new(0);
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !fixture::requested()
        || !matches!(
            std::env::var("SUPER_PLATINUM_CANVAS_REPLACEMENT_CHECK").as_deref(),
            Ok("seed" | "restart" | "review" | "reviewed")
        )
        || STARTED.swap(true, Ordering::Relaxed)
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = match run(state, &desktop).await {
            Ok(value) => value,
            Err(error) => json!({"ok":false,"error":error}),
        };
        eprintln!("canvas-replacement-check: {result}");
    });
}
fn path() -> Result<std::path::PathBuf, String> {
    let path = std::path::PathBuf::from(
        std::env::var_os("SUPER_PLATINUM_CANVAS_RECOVERY_DB").ok_or("Missing isolated journal")?,
    );
    if !path.is_absolute() {
        return Err("Journal path must be absolute".into());
    }
    Ok(path)
}
fn scope() -> Scope {
    Scope {
        workspace: "T1".into(),
        user: "U1".into(),
        file: "Ffixture".into(),
    }
}
async fn stored() -> Result<Option<Receipt>, String> {
    let path = path()?;
    tokio::task::spawn_blocking(move || {
        Journal::open(path, fixture::ACCOUNT).and_then(|j| j.get(&scope()))
    })
    .await
    .map_err(|_| "Journal worker failed")?
    .map_err(|_| "Journal read failed".into())
}
fn counts(state: &Signal<ShellState>) -> (usize, usize, usize, usize) {
    let shell = state.peek();
    let f = shell.canvas_checklist.structure_fixture.as_ref().unwrap();
    (
        f.attempts,
        f.pair_reservations,
        RETIRED.load(Ordering::Relaxed),
        f.text_attempts,
    )
}
async fn controls(
    desktop: &dioxus::desktop::DesktopContext,
    locked: bool,
) -> Result<Value, String> {
    wait(desktop,&format!(r#"(()=>{{const d=document.querySelector('.canvas-dialog'),b=[...document.querySelectorAll('.canvas-paragraph-actions button')];return {{ok:!!d&&b.length>0&&b.every(e=>e.disabled==={locked}),viewport:[innerWidth,innerHeight]}};}})()"#)).await
}
async fn capture(
    state: Signal<ShellState>,
    name: &str,
    ordinal: usize,
    geometry: Option<Value>,
) -> Result<(), String> {
    if !fixture::enabled(&state.peek()) {
        return Err("Connected capture refused".into());
    }
    let directory = std::path::PathBuf::from(
        std::env::var_os("SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR")
            .ok_or("Missing capture directory")?,
    );
    if !directory.is_absolute() {
        return Err("Capture directory must be absolute".into());
    }
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    let (dispatches, reservations, retirements, text_attempts) = counts(&state);
    eprintln!(
        "canvas-replacement-stage: {}",
        json!({"ok":true,"stage":name,"ordinal":ordinal,"noSession":true,"noTransport":true,"dispatches":dispatches,"reservations":reservations,"retirements":retirements,"textAttempts":text_attempts,"reviewGeometry":geometry})
    );
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let p = directory.join(format!("{ordinal}.ack"));
        match tokio::task::spawn_blocking(move || std::fs::read(p))
            .await
            .map_err(|_| "Capture worker failed")?
        {
            Ok(bytes) if bytes == b"captured\n" => return Ok(()),
            Ok(_) => return Err("Malformed capture acknowledgment".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err("Capture acknowledgment unreadable".into()),
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("Capture acknowledgment timeout".into());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}
async fn reserved(
    state: &Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if state
            .peek()
            .canvas_checklist
            .structure_fixture
            .as_ref()
            .is_some_and(|f| f.reserved_replacement.is_some())
        {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("Pair was not reserved".into());
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    wait(desktop,r#"({ok:document.querySelector('#canvas-text-save')?.disabled&&!document.querySelector('#canvas-text-cancel')?.disabled})"#).await?;
    let receipt = stored().await?.ok_or("Missing pre-dispatch receipt")?;
    if !receipt.checkpoint().is_replacement() {
        return Err("Saved request is not a complete pair".into());
    }
    Ok(())
}
fn final_value(state: Signal<ShellState>, mode: &str, layout: Value, flags: &[&str]) -> Value {
    let (dispatches, reservations, retirements, text_attempts) = counts(&state);
    let mut result = json!({"ok":true,"mode":mode,"layout":layout,"noSession":true,"noTransport":true,"dispatches":dispatches,"reservations":reservations,"retirements":retirements,"textAttempts":text_attempts});
    for flag in flags {
        result[*flag] = json!(true);
    }
    result
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let mode = std::env::var("SUPER_PLATINUM_CANVAS_REPLACEMENT_CHECK").unwrap();
    path()?;
    wait(desktop,r#"({ok:!!document.querySelector('[data-canvas-text-section="paragraph"] [data-structure-action="delete"]')})"#).await?;
    if !fixture::enabled(&state.peek()) {
        return Err("Missing disconnected pair authority".into());
    }
    fixture::read(state).await?;
    if mode == "review" {
        return review(state, desktop).await;
    }
    if mode == "reviewed" {
        let path = path()?;
        let count = tokio::task::spawn_blocking(move || {
            Journal::open(path, fixture::ACCOUNT).and_then(|j| j.local_review_count(&scope()))
        })
        .await
        .map_err(|_| "Archive worker")?
        .map_err(|_| "Archive read")?;
        if count != 1 || stored().await?.is_some() {
            return Err("Archived pair did not survive reopen".into());
        }
        let layout = controls(desktop, false).await?;
        capture(state, "review-decision-retained", 0, None).await?;
        return Ok(final_value(
            state,
            &mode,
            layout,
            &["archiveRetained", "noActiveLock", "noReplay"],
        ));
    }
    if mode == "restart" {
        let before = stored().await?.ok_or("Restart lost pair receipt")?;
        controls(desktop, true).await?;
        checked(desktop,r#"(()=>{for(const b of document.querySelectorAll('.canvas-paragraph-actions button'))b.click();return {ok:!document.getElementById('canvas-text-editor')};})()"#).await?;
        fixture::read(state).await?;
        let target = state.peek().canvas.clone().unwrap();
        state.write().dismiss_overlay();
        wait(
            desktop,
            r#"({ok:!document.querySelector('.canvas-dialog')})"#,
        )
        .await?;
        crate::canvas::open_target(state, target);
        wait(
            desktop,
            r#"({ok:!!document.querySelector('[data-canvas-text-section="paragraph"]')})"#,
        )
        .await?;
        fixture::read(state).await?;
        let layout = controls(desktop, true).await?;
        if stored().await?.as_ref() != Some(&before)
            || checklist::presentation(&state.peek()).is_none_or(|v| v.operation.is_some())
        {
            return Err("Reopen changed or revived pair".into());
        }
        capture(state, "restored-locked", 0, None).await?;
        return Ok(final_value(
            state,
            &mode,
            layout,
            &[
                "inertLock",
                "noReplay",
                "refreshPreserved",
                "reopenPreserved",
            ],
        ));
    }
    controls(desktop, false).await?;
    let original = fixture::snapshot();
    capture(state, "initial", 0, None).await?;
    click(
        desktop,
        r#"[data-canvas-text-section="paragraph"] [data-structure-action="delete"]"#,
    )
    .await?;
    wait(desktop,r#"({ok:document.querySelector('.canvas-text-hint')?.textContent.includes('leave an empty paragraph')})"#).await?;
    confirmation(desktop).await?;
    click(desktop, "#canvas-text-cancel").await?;
    wait(desktop,r#"({ok:!document.querySelector('#canvas-text-editor')&&document.activeElement?.closest('[data-canvas-text-section]')?.dataset.canvasTextSection==='paragraph'})"#).await?;
    if stored().await?.is_some() || counts(&state) != (0, 0, 0, 0) {
        return Err("Cancel reserved or sent a pair".into());
    }
    click(
        desktop,
        r#"[data-canvas-text-section="paragraph"] [data-structure-action="delete"]"#,
    )
    .await?;
    let geometry = confirmation(desktop).await?;
    let first_actions = action_geometry(desktop, "Delete paragraph", None).await?;
    let mut action_geometries = vec![first_actions.clone()];
    capture(state, "confirmation", 1, Some(geometry)).await?;
    wait(desktop, r#"({ok:!!document.querySelector('#canvas-text-editor')&&!document.querySelector('#canvas-text-save')?.disabled})"#).await?;
    confirmation(desktop).await?;
    click(desktop, "#canvas-text-save").await?;
    reserved(&state, desktop).await?;
    action_geometries.push(action_geometry(desktop, "Saving\u{2026}", Some(&first_actions)).await?);
    capture(state, "reserved", 2, None).await?;
    if !fixture::release(state).await {
        return Err("Reserved pair lost dispatch ownership".into());
    }
    wait(desktop,r#"({ok:document.querySelector('#canvas-text-save')?.disabled&&document.querySelector('#canvas-text-cancel')?.disabled})"#).await?;
    checked(
        desktop,
        r#"(()=>{document.querySelector('#canvas-text-save').click();return {ok:true};})()"#,
    )
    .await?;
    if counts(&state) != (1, 1, 0, 0) {
        return Err("Duplicate pair dispatch".into());
    }
    action_geometries.push(action_geometry(desktop, "Saving\u{2026}", Some(&first_actions)).await?);
    capture(state, "saving", 3, None).await?;
    let attempt = fixture::accept(&mut state.write()).ok_or("Missing pair ACK")?;
    let read = fixture::read(state).await?;
    if checklist::text_ops::settled(&state.peek(), &attempt) || stored().await?.is_none() {
        return Err("Pair unlocked before disk retirement".into());
    }
    controls(desktop, true).await?;
    action_geometries
        .push(action_geometry(desktop, "Checking\u{2026}", Some(&first_actions)).await?);
    capture(state, "accepted-locked", 4, None).await?;
    checklist::recovery::settle(state, &read).await;
    if !checklist::text_ops::settled(&state.peek(), &attempt) || stored().await?.is_some() {
        return Err("Pair did not retire its exact receipt".into());
    }
    RETIRED.fetch_add(1, Ordering::Relaxed);
    let id = checklist::replacement_ops::replacement(&attempt).unwrap();
    wait(desktop,&format!(r#"({{ok:!document.querySelector('#canvas-text-editor')&&!document.querySelector('[data-canvas-text-section="paragraph"]')&&document.activeElement?.closest('[data-canvas-text-section]')?.dataset.canvasTextSection==={}}})"#,json!(id))).await?;
    let current = checklist::presentation(&state.peek())
        .and_then(|v| v.snapshot)
        .ok_or("No replaced snapshot")?;
    if current.sections.len() != 2
        || current.section("title") != original.section("title")
        || current.section(id).is_none()
    {
        return Err("Replacement changed untouched inventory".into());
    }
    controls(desktop, false).await?;
    capture(state, "replaced", 5, None).await?;
    let open = format!("[data-canvas-text-section={}] .canvas-text-open", json!(id));
    click(desktop, &open).await?;
    wait(desktop,r#"(()=>{const e=document.getElementById('canvas-text-draft');let ready=false;try{ready=!!window.superPlatinumRichEditor.read(e.id)}catch{}return {ok:ready&&e.isContentEditable&&document.activeElement===e};})()"#).await?;
    set_draft(desktop, "<div>Replacement <strong>edited</strong></div>").await?;
    editor_layout(desktop).await?;
    capture(state, "edit-draft", 6, None).await?;
    click(desktop, "#canvas-text-save").await?;
    wait(
        desktop,
        r#"({ok:document.querySelector('#canvas-text-cancel')?.disabled})"#,
    )
    .await?;
    if !continuation_fixture::complete(&mut state.write()) {
        return Err("Replacement text ACK failed".into());
    }
    wait(desktop,&format!(r#"(()=>{{const e=document.querySelector('[data-canvas-text-section='+{}+']');return {{ok:!document.querySelector('#canvas-text-editor')&&e?.textContent.includes('Replacement edited')}};}})()"#,json!(json!(id).to_string()))).await?;
    controls(desktop, false).await?;
    capture(state, "edited", 7, None).await?;
    click(
        desktop,
        &format!(
            "[data-canvas-text-section={}] [data-structure-action=delete]",
            json!(id)
        ),
    )
    .await?;
    wait(desktop, r#"({ok:!!document.querySelector('#canvas-text-editor')&&!document.querySelector('#canvas-text-save')?.disabled})"#).await?;
    confirmation(desktop).await?;
    let second_actions = action_geometry(desktop, "Delete paragraph", None).await?;
    action_geometries.push(second_actions.clone());
    click(desktop, "#canvas-text-save").await?;
    reserved(&state, desktop).await?;
    if !fixture::release(state).await || !fixture::uncertain(&mut state.write()) {
        return Err("Uncertain pair unavailable".into());
    }
    fixture::read(state).await?;
    let layout = controls(desktop, true).await?;
    if counts(&state) != (2, 2, 1, 1) {
        return Err("Unexpected paired workflow counts".into());
    }
    action_geometries.push(action_geometry(desktop, "Unconfirmed", Some(&second_actions)).await?);
    capture(state, "uncertain-before-exit", 8, None).await?;
    let mut result = final_value(
        state,
        &mode,
        layout,
        &[
            "cancelSafe",
            "persistedBeforeDispatch",
            "sealedReadback",
            "retiredBeforeUnlock",
            "replacementFocused",
            "subsequentEdit",
            "duplicateSafe",
            "unknownRetained",
            "stableActions",
        ],
    );
    result["actionGeometry"] = json!(action_geometries);
    Ok(result)
}

// Measure real labels/states; do not overwrite text or styles to simulate them.
const ACTION_GEOMETRY: &str = r#"(expected,baseline)=>{
 const editor=document.getElementById('canvas-text-editor'),group=editor?.querySelector('.canvas-text-actions');
 const cancel=group?.querySelector('#canvas-text-cancel'),save=group?.querySelector('#canvas-text-save');
 if(!editor?.isConnected||!group||!cancel||!save||save.textContent!==expected)return {ok:false};
 const g=group.getBoundingClientRect();
 const rect=e=>{const r=e.getBoundingClientRect();return [r.left-g.left,r.top-g.top,r.width,r.height]};
 const current={label:save.textContent,generation:editor.dataset.editorGeneration,visit:editor.dataset.canvasVisit,groupWidth:g.width,cancel:rect(cancel),save:rect(save)};
 const valid=Number.isFinite(g.width)&&g.width>0&&[...current.cancel,...current.save].every(Number.isFinite)&&current.cancel[2]>0&&current.cancel[3]>0&&current.save[2]>0&&current.save[3]>0;
 const same=!baseline||(baseline.generation===current.generation&&baseline.visit===current.visit&&Math.abs(baseline.groupWidth-current.groupWidth)<=.5&&['cancel','save'].every(k=>baseline[k]?.length===4&&current[k].every((v,i)=>Math.abs(v-baseline[k][i])<=.5)));
 return {ok:valid&&same,...current};
}"#;
async fn action_geometry(
    desktop: &dioxus::desktop::DesktopContext,
    expected: &str,
    baseline: Option<&Value>,
) -> Result<Value, String> {
    wait(
        desktop,
        &format!(
            "({ACTION_GEOMETRY})({},{})",
            json!(expected),
            json!(baseline)
        ),
    )
    .await
}

async fn confirmation(desktop: &dioxus::desktop::DesktopContext) -> Result<Value, String> {
    wait(desktop,r#"({ok:!!document.querySelector('.canvas-text-hint')&&!document.querySelector('#canvas-text-save')?.disabled&&!!document.querySelector('#canvas-text-cancel')})"#).await?;
    super::continuation_check::visible(desktop, "#canvas-text-save").await?;
    let hit = super::continuation_check::HIT;
    checked(desktop,&format!(r#"(()=>{{const hint=({hit})('.canvas-text-hint',false),cancel=({hit})('#canvas-text-cancel',false),save=({hit})('#canvas-text-save',false);const copy=document.querySelector('.canvas-text-hint')?.textContent;return {{ok:hint.ok&&cancel.ok&&save.ok&&copy?.includes('Delete this paragraph and leave an empty paragraph')&&copy.includes('both changes stay locked for review'),hint,cancel,save,viewport:[innerWidth,innerHeight]}};}})()"#)).await
}

async fn review(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let before = stored().await?.ok_or("Missing review pair")?;
    controls(desktop, true).await?;
    let open = r#"(()=>{const b=[...document.querySelectorAll('.canvas-toolbar button')].find(b=>b.textContent==='Review saved change');if(!b||b.disabled)return {ok:false};b.id='pair-review-open';return {ok:true};})()"#;
    checked(desktop, open).await?;
    click(desktop, "#pair-review-open").await?;
    let ready = r#"(()=>{const p=document.querySelector('.canvas-local-review'),c=p?.querySelector('input');return {ok:!!c&&!c.disabled&&!c.checked&&p.textContent.includes('Delete a paragraph and create an empty replacement')&&p.textContent.includes('Either part, both parts, or neither part')&&p.textContent.includes('Replacement edited')};})()"#;
    wait(desktop, ready).await?;
    capture(state, "review-unchecked", 0, None).await?;
    reveal_review_controls(desktop).await?;
    let g = checked(desktop, &review_controls_script("inspect", false)).await?;
    capture(state, "review-controls-unchecked", 1, Some(g)).await?;
    checked(desktop, &review_controls_script("cancel", false)).await?;
    wait(
        desktop,
        r#"({ok:!document.querySelector('.canvas-local-review')})"#,
    )
    .await?;
    if stored().await?.as_ref() != Some(&before) {
        return Err("Review Cancel changed pair receipt".into());
    }
    checked(desktop, open).await?;
    click(desktop, "#pair-review-open").await?;
    wait(desktop, ready).await?;
    reveal_review_controls(desktop).await?;
    checked(desktop, &review_controls_script("check", false)).await?;
    wait(desktop, &review_controls_script("inspect", true)).await?;
    let g = checked(desktop, &review_controls_script("inspect", true)).await?;
    capture(state, "review-confirmation", 2, Some(g)).await?;
    checked(desktop, &review_controls_script("remove", true)).await?;
    wait(
        desktop,
        r#"({ok:!document.querySelector('.canvas-local-review')})"#,
    )
    .await?;
    let p = path()?;
    if stored().await?.is_some()
        || !tokio::task::spawn_blocking(move || {
            Journal::open(p, fixture::ACCOUNT).and_then(|j| j.has_local_review(&before))
        })
        .await
        .map_err(|_| "Archive worker failed")?
        .map_err(|_| "Archive read failed")?
    {
        return Err("Pair was not archived before local unlock".into());
    }
    if checklist::presentation(&state.peek()).is_none_or(|v| !v.stale || v.operation.is_some()) {
        return Err("Local review skipped refresh or revived pair".into());
    }
    capture(state, "review-removed", 3, None).await?;
    checked(desktop,r#"(()=>{const b=[...document.querySelectorAll('.canvas-toolbar button')].find(b=>b.textContent==='Refresh');if(!b||b.disabled)return {ok:false};b.id='pair-refresh';return {ok:true};})()"#).await?;
    click(desktop, "#pair-refresh").await?;
    let layout = controls(desktop, false).await?;
    let current = checklist::presentation(&state.peek())
        .and_then(|v| v.snapshot)
        .ok_or("Missing refreshed inventory")?;
    if current.sections != fixture::snapshot().sections {
        return Err("Local review rewrote canonical content".into());
    }
    capture(state, "review-refreshed", 4, None).await?;
    Ok(final_value(
        state,
        "review",
        layout,
        &[
            "manualAcknowledgement",
            "cancelSafe",
            "archiveBeforeUnlock",
            "refreshRequired",
            "canonicalUnchanged",
            "outcomeUnknown",
            "noReplay",
        ],
    ))
}
