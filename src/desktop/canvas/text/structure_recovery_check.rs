//! Real SQLite workers + native DOM, with controlled exit/reopen. Synthetic
//! canonical snapshots/ACKs only; no transport, Slack request, or crash claim.
use super::{
    check::{checked, wait},
    checklist, structure_fixture,
};
use crate::state::ShellState;
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use super_platinum_core::{
    canvas::edit::{
        structure::{
            PreparedStructureEdit,
            recovery::{
                Scope,
                journal::{Journal, Receipt},
            },
        },
        text::TextDraft,
    },
    slack::api::CanvasCollabSnapshot,
};
const ACCOUNT: &str = "00000000-0000-4000-8000-000000009998";
static STARTED: AtomicBool = AtomicBool::new(false);
static DISPATCHES: AtomicUsize = AtomicUsize::new(0);
static RESERVATIONS: AtomicUsize = AtomicUsize::new(0);
static RETIREMENTS: AtomicUsize = AtomicUsize::new(0);
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !structure_fixture::requested()
        || !matches!(
            std::env::var("SUPER_PLATINUM_CANVAS_STRUCTURE_RECOVERY_CHECK").as_deref(),
            Ok("seed" | "restart" | "review" | "reviewed")
        )
    {
        return;
    }
    if STARTED.swap(true, Ordering::Relaxed) {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = match run(state, &desktop).await {
            Ok(value) => value,
            Err(error) => json!({"ok":false,"error":error}),
        };
        eprintln!("canvas-structure-recovery-check: {result}");
    });
}
fn path() -> Result<std::path::PathBuf, String> {
    let path = std::path::PathBuf::from(
        std::env::var_os("SUPER_PLATINUM_CANVAS_RECOVERY_DB")
            .ok_or("Missing isolated recovery DB")?,
    );
    if !path.is_absolute() {
        return Err("Recovery DB must be an absolute isolated path".into());
    }
    Ok(path)
}
async fn stored(scope: Scope) -> Result<Option<Receipt>, String> {
    let path = path()?;
    tokio::task::spawn_blocking(move || Journal::open(path, ACCOUNT).and_then(|j| j.get(&scope)))
        .await
        .map_err(|_| "Recovery read worker failed")?
        .map_err(|_| "Recovery storage read failed".into())
}
async fn read(
    mut state: Signal<ShellState>,
    snapshot: Arc<CanvasCollabSnapshot>,
) -> Result<checklist::Read, String> {
    let read = checklist::begin_read(&mut state.write()).ok_or("Canonical read unavailable")?;
    checklist::recovery::load(state, &read).await;
    if !checklist::finish_read(&mut state.write(), &read, Ok(snapshot)) {
        return Err("Canonical read ownership changed".into());
    }
    Ok(read)
}
fn prepare(
    mut state: Signal<ShellState>,
    anchor: &str,
    action: checklist::structure_ops::Action,
    text: &str,
) -> Result<(checklist::Attempt, PreparedStructureEdit), String> {
    let view = checklist::presentation(&state.peek()).ok_or("Missing canonical view")?;
    let (attempt, intent) = checklist::structure_ops::begin_structure(
        &mut state.write(),
        view.visit,
        view.read,
        anchor,
        action,
        &TextDraft::from_rtml(text).map_err(|_| "Invalid fixture draft")?,
        checklist::save::now_usec().ok_or("Missing clock")?,
    )
    .map_err(str::to_owned)?;
    let prepared = intent
        .prepare(&attempt.snapshot, checklist::save::now_usec().unwrap())
        .map_err(|_| "Fixture preparation failed")?;
    Ok((attempt, prepared))
}
async fn controls(
    desktop: &dioxus::desktop::DesktopContext,
    locked: bool,
) -> Result<Value, String> {
    wait(desktop,&format!(r#"(()=>{{const dialog=document.querySelector('.canvas-dialog'),a=[...document.querySelectorAll('[data-structure-action]')];const r=dialog?.getBoundingClientRect();return {{ok:!!r&&a.length>0&&a.every(b=>b.disabled==={locked})&&r.left>=-1&&r.right<=innerWidth+1&&r.top>=-1&&r.bottom<=innerHeight+1,viewport:[innerWidth,innerHeight],buttons:a.length}}}})()"#)).await
}
async fn inventory(
    desktop: &dioxus::desktop::DesktopContext,
    inserted: bool,
) -> Result<(), String> {
    wait(desktop, &format!(r#"(()=>{{const rows=[...document.querySelectorAll('[data-canvas-text-section]')];const ids=rows.map(r=>r.getAttribute('data-canvas-text-section'));return {{ok:rows.length==={}&&ids.includes('paragraph')&&ids.includes('neighbor')&&rows.some(r=>r.textContent.includes('Durable paragraph'))==={inserted}&&rows.some(r=>r.textContent.includes('First paragraph'))&&rows.some(r=>r.textContent.includes('Another paragraph'))}}}})()"#, if inserted { 3 } else { 2 })).await?;
    Ok(())
}
async fn reserve(
    state: Signal<ShellState>,
    attempt: &checklist::Attempt,
    prepared: &PreparedStructureEdit,
) -> Result<(), String> {
    let _ticket = checklist::recovery::reserve(state, attempt, prepared)
        .await
        .map_err(str::to_owned)?;
    RESERVATIONS.fetch_add(1, Ordering::Relaxed);
    Ok(())
}
async fn accepted_roundtrip(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    attempt: checklist::Attempt,
    prepared: PreparedStructureEdit,
    scope: &Scope,
    capture: bool,
) -> Result<Arc<CanvasCollabSnapshot>, String> {
    if stored(scope.clone()).await?.is_none() {
        return Err("Dispatch preceded durable reservation".into());
    }
    if !checklist::structure_ops::mark_dispatched(&mut state.write(), &attempt, &prepared) {
        return Err("Fixture dispatch ownership changed".into());
    }
    DISPATCHES.fetch_add(1, Ordering::Relaxed);
    let (accepted, current) = structure_fixture::accepted(
        prepared,
        &attempt.snapshot,
        checklist::save::now_usec().unwrap(),
    );
    if !checklist::structure_ops::finish_structure(&mut state.write(), &attempt, accepted) {
        return Err("Sealed ACK was not retained".into());
    }
    let read = read(state, current.clone()).await?;
    if checklist::confirm_readback(&mut state.write(), &attempt, &read) {
        return Err("Readback unlocked before durable retirement".into());
    }
    if checklist::presentation(&state.peek())
        .and_then(|v| v.operation)
        .is_none()
        || stored(scope.clone()).await?.is_none()
    {
        return Err("Accepted receipt was lost before retirement".into());
    }
    inventory(desktop, capture).await?;
    if capture {
        controls(desktop, true).await?;
        capture_stage(state, "accepted-locked", 2).await?;
    }
    checklist::recovery::settle(state, &read).await;
    if checklist::presentation(&state.peek())
        .and_then(|v| v.operation)
        .is_some()
        || stored(scope.clone()).await?.is_some()
    {
        return Err("Sealed readback did not durably retire exact receipt".into());
    }
    RETIREMENTS.fetch_add(1, Ordering::Relaxed);
    Ok(current)
}
// This isolated checker uses the canonical fixture's controller owner. The
// general shell scene starts as U0 while the retained Canvas fixture is U1.
// Align both synthetic viewer projections before any journal load or attempt;
// production reserve must continue rejecting differing checkpoint/disk owners.
fn install_fixture_owner(
    shell: &mut ShellState,
    snapshot: &CanvasCollabSnapshot,
) -> Result<(), String> {
    if shell.core.session.is_some()
        || shell.core.transport.is_some()
        || shell.core.active_team.as_deref() != Some(snapshot.init.workspace_id())
    {
        return Err("Recovery owner requires the disconnected canonical workspace".into());
    }
    let user = snapshot
        .init
        .options()
        .string(33)
        .map_err(|_| "Invalid fixture owner")?
        .ok_or("Missing fixture owner")?;
    let workspace = shell
        .core
        .workspaces
        .get_mut(snapshot.init.workspace_id())
        .ok_or("Missing fixture workspace")?;
    shell.core.active_account = Some(ACCOUNT.into());
    workspace.self_user_id = user.into();
    shell.self_account.user_id = user.into();
    Ok(())
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let mode = std::env::var("SUPER_PLATINUM_CANVAS_STRUCTURE_RECOVERY_CHECK").unwrap();
    path()?;
    wait(
        desktop,
        r#"({ok:!!document.querySelector('.canvas-dialog [data-structure-action]')})"#,
    )
    .await?;
    if !structure_fixture::enabled(&state.peek()) {
        return Err("Recovery check requires disconnected fixture".into());
    }
    let original = structure_fixture::snapshot();
    {
        let mut shell = state.write();
        install_fixture_owner(&mut shell, &original)?;
        checklist::open_visit(&mut shell);
    }
    read(state, original.clone()).await?;
    let scope = Scope {
        workspace: original.init.workspace_id().into(),
        user: original
            .init
            .options()
            .string(33)
            .map_err(|_| "Invalid owner")?
            .ok_or("Missing owner")?
            .into(),
        file: original.file_id().into(),
    };
    if mode == "reviewed" {
        return reviewed_check(state, desktop, scope, original).await;
    }
    if mode == "review" {
        return local_review_check(state, desktop, scope, original).await;
    }
    if mode == "restart" {
        let before = stored(scope.clone())
            .await?
            .ok_or("Restart lost durable receipt")?;
        let layout = controls(desktop, true).await?;
        inventory(desktop, false).await?;
        if prepare(
            state,
            "paragraph",
            checklist::structure_ops::Action::After,
            "No replay",
        )
        .is_ok()
        {
            return Err("Restored receipt permitted another intent".into());
        }
        checked(desktop,r#"(()=>{for(const b of document.querySelectorAll('[data-structure-action]'))b.click();return {ok:!document.getElementById('canvas-text-editor')};})()"#).await?;
        read(state, original.clone()).await?; // A matching read cannot settle uncertainty.
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
            r#"({ok:!!document.querySelector('.canvas-dialog')})"#,
        )
        .await?;
        // Action nodes require the mounted fixture's canonical projection.
        controls(desktop, true).await?;
        read(state, original).await?;
        controls(desktop, true).await?;
        let after = stored(scope)
            .await?
            .ok_or("Refresh or reopen retired uncertainty")?;
        if before.operation_id() != after.operation_id()
            || checklist::presentation(&state.peek())
                .and_then(|v| v.operation)
                .is_some()
        {
            return Err("Restart recreated an in-memory dispatch".into());
        }
        capture_stage(state, "restored-locked", 0).await?;
        return Ok(
            json!({"ok":true,"mode":mode,"dispatches":DISPATCHES.load(Ordering::Relaxed),"reservations":RESERVATIONS.load(Ordering::Relaxed),"retirements":RETIREMENTS.load(Ordering::Relaxed),"inertLock":true,"noReplay":true,"refreshPreserved":true,"reopenPreserved":true,"noSession":state.peek().core.session.is_none(),"noTransport":state.peek().core.transport.is_none(),"layout":layout}),
        );
    }
    if stored(scope.clone()).await?.is_some() {
        return Err("Seed storage was not empty".into());
    }
    controls(desktop, false).await?;
    inventory(desktop, false).await?;
    capture_stage(state, "initial", 0).await?;
    let (insert, prepared) = prepare(
        state,
        "paragraph",
        checklist::structure_ops::Action::After,
        "Durable paragraph",
    )?;
    let inserted = prepared.target_section().to_owned();
    reserve(state, &insert, &prepared).await?;
    if stored(scope.clone()).await?.is_none() {
        return Err("Reservation was not committed".into());
    }
    controls(desktop, true).await?;
    capture_stage(state, "reserved", 1).await?;
    let current = accepted_roundtrip(state, desktop, insert, prepared, &scope, true).await?;
    controls(desktop, false).await?;
    capture_stage(state, "retired", 3).await?;
    let (delete, prepared) = prepare(
        state,
        &inserted,
        checklist::structure_ops::Action::Delete,
        "",
    )?;
    reserve(state, &delete, &prepared).await?;
    let restored = accepted_roundtrip(state, desktop, delete, prepared, &scope, false).await?;
    if restored.sections != original.sections
        || current.sections.len() != original.sections.len() + 1
    {
        return Err("Normal workflow did not preserve exact inventory".into());
    }
    let (pending, prepared) = prepare(
        state,
        "paragraph",
        checklist::structure_ops::Action::After,
        "Retain after restart",
    )?;
    reserve(state, &pending, &prepared).await?;
    // Deliberately stop BEFORE mark_dispatched. The owned runner exits this
    // process after saving PNG/SQL evidence; restarting never resumes this value.
    drop(prepared);
    let layout = controls(desktop, true).await?;
    capture_stage(state, "reserved-before-exit", 4).await?;
    Ok(
        json!({"ok":true,"mode":mode,"dispatches":DISPATCHES.load(Ordering::Relaxed),"reservations":RESERVATIONS.load(Ordering::Relaxed),"retirements":RETIREMENTS.load(Ordering::Relaxed),"persistedBeforeDispatch":true,"sealedReadback":true,"retiredBeforeUnlock":true,"originalRestored":true,"pendingNotDispatched":true,"noSession":state.peek().core.session.is_none(),"noTransport":state.peek().core.transport.is_none(),"layout":layout}),
    )
}

async fn capture_stage(
    state: Signal<ShellState>,
    stage: &str,
    ordinal: usize,
) -> Result<(), String> {
    capture_stage_geometry(state, stage, ordinal, None).await
}
async fn capture_stage_geometry(
    state: Signal<ShellState>,
    stage: &str,
    ordinal: usize,
    geometry: Option<Value>,
) -> Result<(), String> {
    if !structure_fixture::enabled(&state.peek()) {
        return Err("Connected capture refused".into());
    }
    let directory = std::path::PathBuf::from(
        std::env::var_os("SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR")
            .ok_or("Missing capture acknowledgments")?,
    );
    if !directory.is_absolute() {
        return Err("Capture directory must be absolute".into());
    }
    let ack = directory.join(format!("{ordinal}.ack"));
    if geometry.is_none() {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    eprintln!(
        "canvas-structure-recovery-stage: {}",
        json!({"ok":true,"stage":stage,"ordinal":ordinal,"reviewGeometry":geometry,"noSession":state.peek().core.session.is_none(),"noTransport":state.peek().core.transport.is_none(),"dispatches":DISPATCHES.load(Ordering::Relaxed),"reservations":RESERVATIONS.load(Ordering::Relaxed),"retirements":RETIREMENTS.load(Ordering::Relaxed)})
    );
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let path = ack.clone();
        match tokio::task::spawn_blocking(move || std::fs::read(path))
            .await
            .map_err(|_| "Capture worker failed")?
        {
            Ok(bytes) if bytes == b"captured\n" => return Ok(()),
            Ok(_) => return Err("Malformed capture acknowledgment".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err("Capture acknowledgment unreadable".into()),
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("Capture acknowledgment timed out".into());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

async fn reviewed_check(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    scope: Scope,
    original: Arc<CanvasCollabSnapshot>,
) -> Result<Value, String> {
    if stored(scope.clone()).await?.is_some() {
        return Err("Reopened review retained an active lock".into());
    }
    let path = path()?;
    let count = tokio::task::spawn_blocking(move || {
        Journal::open(path, ACCOUNT).and_then(|j| j.local_review_count(&scope))
    })
    .await
    .map_err(|_| "Review audit worker failed")?
    .map_err(|_| "Review archive unavailable")?;
    if count != 1 {
        return Err("Reopened review lost the unknown-outcome archive".into());
    }
    let layout = controls(desktop, false).await?;
    inventory(desktop, false).await?;
    let view = checklist::presentation(&state.peek()).ok_or("Missing reopened view")?;
    if view.operation.is_some()
        || view.stale
        || view
            .snapshot
            .is_none_or(|s| s.sections != original.sections)
    {
        return Err("Reopened review recreated an operation or changed canonical content".into());
    }
    capture_stage(state, "review-decision-retained", 0).await?;
    Ok(
        json!({"ok":true,"mode":"reviewed","archiveRetained":true,"noActiveLock":true,"canonicalUnchanged":true,"noReplay":true,
        "dispatches":DISPATCHES.load(Ordering::Relaxed),"reservations":RESERVATIONS.load(Ordering::Relaxed),"retirements":RETIREMENTS.load(Ordering::Relaxed),
        "noSession":state.peek().core.session.is_none(),"noTransport":state.peek().core.transport.is_none(),"layout":layout}),
    )
}

// Geometry is measured before every action; programmatic DOM clicks below do
// not constitute trusted OS-pointer input. The owned runner separately parks
// its pointer and captures the actual native window.
const REVIEW_CONTROLS: &str = r#"(action, expectedChecked) => {
    const panel=document.querySelector('.canvas-local-review');
    const checkbox=panel?.querySelector('input[type="checkbox"]');
    const buttons=[...(panel?.querySelectorAll('button')||[])];
    const cancel=buttons.find(b=>b.textContent==='Cancel');
    const remove=buttons.find(b=>b.textContent==='Remove local edit lock');
    const label=checkbox?.closest('label');
    if(!panel||!checkbox||!label||buttons.length!==2||!cancel||!remove||cancel.tagName!=='BUTTON'||remove.tagName!=='BUTTON')return {ok:false};
    if(action==='reveal') {
        const scroll=panel.closest('.canvas-scroll');
        if(!scroll)return {ok:false};
        const viewport=scroll.getBoundingClientRect(),bottom=viewport.top+scroll.clientTop+scroll.clientHeight;
        scroll.scrollTop+=remove.getBoundingClientRect().bottom-bottom+8;
        const ticket={ready:false,checkbox,cancel,remove};panel.__reviewScroll=ticket;
        requestAnimationFrame(()=>requestAnimationFrame(()=>{if(panel.isConnected&&panel.__reviewScroll===ticket)ticket.ready=true;}));
        return {ok:true};
    }
    const ticket=panel.__reviewScroll;
    if(!ticket?.ready||ticket.checkbox!==checkbox||ticket.cancel!==cancel||ticket.remove!==remove)return {ok:false};
    const bounds=e=>{const r=e.getBoundingClientRect();return [r.left,r.top,r.right,r.bottom];};
    const measure=(e,hitTest)=>{
        const rect=bounds(e),clip=[0,0,innerWidth,innerHeight];
        let shown=e.isConnected&&!e.closest('[hidden],[inert]');
        for(let a=e;a;a=a.parentElement){
            const style=getComputedStyle(a);
            shown=shown&&style.display!=='none'&&style.visibility!=='hidden'&&style.visibility!=='collapse'&&Number(style.opacity)>0;
            if(a===e)continue;
            const r=a.getBoundingClientRect();
            if(/auto|scroll|hidden|clip/.test(style.overflowX)){clip[0]=Math.max(clip[0],r.left+a.clientLeft);clip[2]=Math.min(clip[2],r.left+a.clientLeft+a.clientWidth);}
            if(/auto|scroll|hidden|clip/.test(style.overflowY)){clip[1]=Math.max(clip[1],r.top+a.clientTop);clip[3]=Math.min(clip[3],r.top+a.clientTop+a.clientHeight);}
        }
        const point=[(rect[0]+rect[2])/2,(rect[1]+rect[3])/2];
        const hit=!hitTest||e.contains(document.elementFromPoint(...point));
        const visible=shown&&rect.every(Number.isFinite)&&rect[2]>rect[0]&&rect[3]>rect[1]&&rect[0]>=clip[0]&&rect[1]>=clip[1]&&rect[2]<=clip[2]&&rect[3]<=clip[3];
        return {rect,clip,point,visible,hit,hitTested:hitTest};
    };
    const proof={checked:checkbox.checked,submitDisabled:remove.disabled,cancelDisabled:cancel.disabled,checkboxDisabled:checkbox.disabled,
        checkbox:measure(checkbox,true),label:measure(label,false),cancel:measure(cancel,true),remove:measure(remove,true),viewport:[innerWidth,innerHeight]};
    proof.ok=checkbox.checked===expectedChecked&&!checkbox.disabled&&!cancel.disabled&&remove.disabled===!expectedChecked&&['checkbox','label','cancel','remove'].every(k=>proof[k].visible&&proof[k].hit);
    if(!proof.ok||action==='inspect')return proof;
    const target=action==='cancel'?cancel:action==='check'&&!expectedChecked?checkbox:action==='remove'&&expectedChecked?remove:null;
    if(!target||target.disabled)return {...proof,ok:false};
    target.focus({preventScroll:true});
    if(document.activeElement!==target)return {...proof,ok:false};
    target.click();
    return {...proof,action};
}"#;
pub(super) fn review_controls_script(action: &str, expected_checked: bool) -> String {
    format!("({REVIEW_CONTROLS})({}, {expected_checked})", json!(action))
}
pub(super) async fn reveal_review_controls(
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<(), String> {
    checked(desktop, &review_controls_script("reveal", false)).await?;
    wait(desktop, &review_controls_script("inspect", false)).await?;
    Ok(())
}
async fn capture_review_controls(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    stage: &str,
    ordinal: usize,
    checked_state: bool,
) -> Result<(), String> {
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    let geometry = checked(desktop, &review_controls_script("inspect", checked_state)).await?;
    capture_stage_geometry(state, stage, ordinal, Some(geometry)).await
}

async fn local_review_check(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    scope: Scope,
    original: Arc<CanvasCollabSnapshot>,
) -> Result<Value, String> {
    let before = stored(scope.clone())
        .await?
        .ok_or("Review requires a retained seed receipt")?;
    controls(desktop, true).await?;
    let open = r#"(()=>{const b=[...document.querySelectorAll('.canvas-toolbar button')].find(b=>b.textContent==='Review saved change');if(b&&!b.disabled)b.click();return {ok:!!b&&!b.disabled};})()"#;
    let ready = r#"(()=>{const p=document.querySelector('.canvas-local-review'),c=p?.querySelector('input'),b=[...(p?.querySelectorAll('button')||[])].find(b=>b.textContent==='Remove local edit lock');return {ok:!!c&&!c.disabled&&!c.checked&&b?.disabled&&p.textContent.includes('unknown outcome')&&p.textContent.includes('Retain after restart')};})()"#;
    checked(desktop, open).await?;
    wait(desktop, ready).await?;
    capture_stage(state, "review-unchecked", 0).await?;
    reveal_review_controls(desktop).await?;
    capture_review_controls(state, desktop, "review-controls-unchecked", 1, false).await?;
    checked(desktop, &review_controls_script("cancel", false)).await?;
    wait(
        desktop,
        r#"({ok:!document.querySelector('.canvas-local-review')})"#,
    )
    .await?;
    if stored(scope.clone()).await?.as_ref() != Some(&before) {
        return Err("Cancel changed the durable receipt".into());
    }
    controls(desktop, true).await?;
    checked(desktop, open).await?;
    wait(desktop, ready).await?;
    reveal_review_controls(desktop).await?;
    checked(desktop, &review_controls_script("check", false)).await?;
    wait(desktop,r#"(()=>{const b=[...document.querySelectorAll('.canvas-local-review button')].find(b=>b.textContent==='Remove local edit lock');return {ok:!!b&&!b.disabled};})()"#).await?;
    capture_review_controls(state, desktop, "review-confirmation", 2, true).await?;
    checked(desktop, &review_controls_script("remove", true)).await?;
    wait(
        desktop,
        r#"({ok:!document.querySelector('.canvas-local-review')})"#,
    )
    .await?;
    if stored(scope.clone()).await?.is_some() {
        return Err("Explicit local decision left the active row".into());
    }
    let path = path()?;
    if !tokio::task::spawn_blocking(move || {
        Journal::open(path, ACCOUNT).and_then(|j| j.has_local_review(&before))
    })
    .await
    .map_err(|_| "Review audit worker failed")?
    .map_err(|_| "Review archive unavailable")?
    {
        return Err("Active removal lacked an exact unknown-outcome archive".into());
    }
    if checklist::presentation(&state.peek()).is_none_or(|v| !v.stale || v.operation.is_some()) {
        return Err("Local unlock skipped mandatory refresh or recreated an operation".into());
    }
    capture_stage(state, "review-removed", 3).await?;
    checked(desktop,r#"(()=>{const b=[...document.querySelectorAll('.canvas-toolbar button')].find(b=>b.textContent==='Refresh');b?.click();return {ok:!!b&&!b.disabled};})()"#).await?;
    let layout = controls(desktop, false).await?;
    let current = checklist::presentation(&state.peek())
        .and_then(|v| v.snapshot)
        .ok_or("Missing canonical read")?;
    if current.sections != original.sections {
        return Err("Local review changed canonical content".into());
    }
    capture_stage(state, "review-refreshed", 4).await?;
    Ok(
        json!({"ok":true,"mode":"review","manualAcknowledgement":true,"cancelSafe":true,"archiveBeforeUnlock":true,"refreshRequired":true,"canonicalUnchanged":true,"outcomeUnknown":true,"noReplay":true,
        "dispatches":DISPATCHES.load(Ordering::Relaxed),"reservations":RESERVATIONS.load(Ordering::Relaxed),"retirements":RETIREMENTS.load(Ordering::Relaxed),
        "noSession":state.peek().core.session.is_none(),"noTransport":state.peek().core.transport.is_none(),"layout":layout}),
    )
}

#[cfg(test)]
mod owner_tests {
    use super::*;

    #[test]
    fn native_recovery_scene_uses_one_exact_controller_and_shell_owner() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        let snapshot = structure_fixture::snapshot();
        let user = snapshot.init.options().string(33).unwrap().unwrap();
        assert_ne!(
            shell.self_account.user_id, user,
            "original scene reproduces the mismatch"
        );
        install_fixture_owner(&mut shell, &snapshot).unwrap();
        assert_eq!(shell.core.active_account.as_deref(), Some(ACCOUNT));
        assert_eq!(shell.self_account.user_id, user);
        assert_eq!(
            shell
                .core
                .workspaces
                .get(snapshot.init.workspace_id())
                .unwrap()
                .self_user_id,
            user
        );
        shell.canvas = Some(crate::state::CanvasTarget {
            id: snapshot.file_id().into(),
            title: "Synthetic".into(),
            permalink: None,
        });
        shell.overlay = Some(crate::state::Overlay::Canvas);
        checklist::open_visit(&mut shell);
        let read = checklist::begin_read(&mut shell).unwrap();
        assert!(checklist::finish_read(
            &mut shell,
            &read,
            Ok(snapshot.clone())
        ));
        let intent = checklist::structure_ops::Action::After
            .capture(
                &snapshot,
                "paragraph",
                &TextDraft::from_rtml("Durable paragraph").unwrap(),
                100,
            )
            .unwrap();
        let prepared = intent.prepare(&snapshot, 100).unwrap();
        let checkpoint = prepared.checkpoint().unwrap();
        assert_eq!(
            checkpoint.scope().workspace,
            shell.core.active_team.as_deref().unwrap()
        );
        assert_eq!(checkpoint.scope().user, shell.self_account.user_id);
        assert_eq!(checkpoint.scope().file, shell.canvas.as_ref().unwrap().id);
    }

    #[test]
    fn fixture_owner_alignment_refuses_connected_or_other_workspace_state() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        let snapshot = structure_fixture::snapshot();
        let user = shell.self_account.user_id.clone();
        shell.core.transport = Some(Arc::new(
            super_platinum_core::slack::Transport::new("synthetic").unwrap(),
        ));
        assert!(install_fixture_owner(&mut shell, &snapshot).is_err());
        assert_eq!(shell.self_account.user_id, user);
        shell.core.transport = None;
        shell.core.active_team = Some("OTHER".into());
        assert!(install_fixture_owner(&mut shell, &snapshot).is_err());
        assert_eq!(shell.self_account.user_id, user);
    }
}
