//! Restored pair review in owned offline processes. DOM clicks are synthetic;
//! journal reads and archive commits are real. No saved intent is resumed.
use super::{
    check::{checked, wait},
    checklist,
    continuation_check::click,
    split_recovery_fixture as fixture,
    structure_recovery_check::{reveal_review_controls, review_controls_script},
};
use crate::state::ShellState;
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, Ordering};
use super_platinum_core::canvas::edit::structure::recovery::journal::{Begin, Journal, Receipt};
static STARTED: AtomicBool = AtomicBool::new(false);
#[path = "split_recovery_parent_edit.rs"]
mod parent_edit;
#[path = "split_recovery_pre_enter.rs"]
mod pre_enter;
#[path = "split_recovery_second_middle.rs"]
mod second_middle;
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !fixture::requested() || STARTED.swap(true, Ordering::Relaxed) {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = match run(state, &desktop).await {
            Ok(v) => v,
            Err(error) => json!({"ok":false,"error":error}),
        };
        eprintln!("canvas-split-recovery-check: {result}");
    });
}
fn path() -> Result<std::path::PathBuf, String> {
    let p = std::path::PathBuf::from(
        std::env::var_os("SUPER_PLATINUM_CANVAS_RECOVERY_DB").ok_or("Missing isolated journal")?,
    );
    if !p.is_absolute() {
        return Err("Journal path must be absolute".into());
    }
    Ok(p)
}
async fn stored() -> Result<Option<Receipt>, String> {
    let p = path()?;
    tokio::task::spawn_blocking(move || {
        Journal::open(p, fixture::ACCOUNT).and_then(|j| j.get(&fixture::scope()))
    })
    .await
    .map_err(|_| "Journal worker failed")?
    .map_err(|_| "Journal read failed".into())
}
async fn journal(before: Option<&Receipt>) -> Result<Value, String> {
    let p = path()?;
    let before = before.cloned();
    tokio::task::spawn_blocking(move||{
        let j=Journal::open(p,fixture::ACCOUNT)?;let active=j.get(&fixture::scope())?;
        let archived=before.as_ref().map(|r|j.has_local_review(r)).transpose()?;
        Ok::<_,super_platinum_core::error::AppError>(json!({"active":active.map(|r|json!({"operation":r.operation_id(),"checkpoint":r.checkpoint()})),"archivedExact":archived,"archiveCount":j.local_review_count(&fixture::scope())?}))
    }).await.map_err(|_|"Journal observation worker failed")?.map_err(|_|"Journal observation failed".into())
}
async fn read(mut state: Signal<ShellState>) -> Result<(), String> {
    let snapshot = state
        .peek()
        .canvas_checklist
        .structure_fixture
        .as_ref()
        .unwrap()
        .canonical
        .clone()
        .unwrap();
    let read = checklist::begin_read(&mut state.write()).ok_or("Canonical read unavailable")?;
    checklist::recovery::load(state, &read).await;
    if !checklist::finish_read(&mut state.write(), &read, Ok(snapshot)) {
        return Err("Canonical read changed".into());
    }
    Ok(())
}
async fn controls(
    desktop: &dioxus::desktop::DesktopContext,
    locked: bool,
) -> Result<Value, String> {
    wait(desktop,&format!(r#"(()=>{{const d=document.querySelector('.canvas-dialog'),buttons=[...document.querySelectorAll('.canvas-paragraph-actions button')],r=d?.getBoundingClientRect();return {{ok:!!r&&buttons.length>0&&buttons.every(b=>b.disabled==={locked})&&r.left>=-1&&r.top>=-1&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1,viewport:[innerWidth,innerHeight]}};}})()"#)).await
}
const OPEN: &str = r#"(()=>{const b=[...document.querySelectorAll('.canvas-toolbar button')].find(b=>b.textContent==='Review saved change');if(!b||b.disabled)return {ok:false};b.id='split-review-open';return {ok:true};})()"#;
fn ready() -> String {
    if super::split_fixture::second_middle::enabled() {
        return second_middle::ready();
    }
    let corpus = super::split_fixture::corpus();
    if super::split_fixture::is_sequence() {
        let finals = (0..super::split_fixture::sequence::count())
            .map(super::split_fixture::sequence::final_text)
            .collect::<Vec<_>>();
        return format!(
            r#"(()=>{{const p=document.querySelector('.canvas-local-review'),c=p?.querySelector('input');return {{ok:!!c&&!c.checked&&!c.disabled&&p.querySelector('[data-review-sequence=original] .canvas-review-preview')?.textContent==={}&&p.querySelector('[data-review-sequence=parent] .canvas-review-preview')?.textContent==={}&&JSON.stringify([...p.querySelectorAll('[data-review-sequence=member] > .canvas-review-preview')].map(n=>n.textContent))===JSON.stringify({})&&p.querySelectorAll('[data-review-current]').length==={}}};}})()"#,
            json!(corpus.seed),
            json!(super::split_fixture::sequence::parent_final()),
            json!(finals),
            fixture::snapshot().sections.len()
        );
    }
    if super::split_fixture::is_start_typing() {
        return format!(
            r#"(()=>{{const p=document.querySelector('.canvas-local-review'),c=p?.querySelector('input'),n=p?.querySelector('[data-review-start-typing]');return {{ok:!!c&&!c.checked&&!c.disabled&&n?.querySelector('[data-review-start-before] .canvas-review-preview')?.textContent==={}&&n.querySelector('[data-review-start-after] .canvas-review-preview')?.textContent==={}&&n.querySelector('[data-review-start-new] .canvas-review-preview')?.textContent===''&&n.querySelector('[data-empty-paragraph]')?.textContent==='Empty paragraph'&&n.querySelector('[data-review-start-current] .canvas-review-preview')?.textContent==={}&&p.querySelectorAll('[data-review-split],[data-review-current],[data-review-insertion]').length===0}};}})()"#,
            json!(corpus.seed),
            json!(super::split_fixture::parent_text()),
            json!(corpus.seed)
        );
    }
    if super::split_fixture::is_start() {
        return format!(
            r#"(()=>{{const p=document.querySelector('.canvas-local-review'),c=p?.querySelector('input'),n=p?.querySelector('[data-review-insertion]');return {{ok:!!c&&!c.checked&&!c.disabled&&n?.dataset.anchor==='paragraph'&&n.dataset.side==='before'&&n.querySelector('[data-review-insert-draft] .canvas-review-preview')?.textContent===''&&n.querySelector('[data-empty-paragraph]')?.textContent==='Empty paragraph'&&n.querySelector('[data-review-insert-current="paragraph"] .canvas-review-preview')?.textContent==={}&&p.querySelectorAll('[data-review-split],[data-review-current]').length===0}};}})()"#,
            json!(corpus.seed)
        );
    }
    format!(
        r#"(()=>{{const p=document.querySelector('.canvas-local-review'),c=p?.querySelector('input');return {{ok:!!c&&!c.checked&&!c.disabled&&p.querySelector('[data-review-split=original] .canvas-review-preview')?.textContent==={}&&p.querySelector('[data-review-split=parent] .canvas-review-preview')?.textContent==={}&&p.querySelector('[data-review-split=child] .canvas-review-preview')?.textContent==={}&&p.querySelectorAll('[data-review-current]').length==={}&&(!{}||p.querySelector('[data-review-split=secondChild] .canvas-review-preview')?.textContent==={})}};}})()"#,
        json!(corpus.seed),
        json!(super::split_fixture::parent_text()),
        json!(super::split_fixture::child_text()),
        if super::split_fixture::is_empty() || super::split_fixture::is_repeat() {
            fixture::snapshot().sections.len()
        } else if super::split_fixture::is_reuse() {
            4
        } else {
            3
        },
        super::split_fixture::is_repeat(),
        json!(super::split_fixture::second_child_text())
    )
}
const PREVIEW: &str = r#"(()=>{const p=document.querySelector('.canvas-local-review'),c=p?.querySelector('input'),buttons=[...(p?.querySelectorAll('button')||[])],cancel=buttons.find(b=>b.textContent==='Cancel'),remove=buttons.find(b=>b.textContent==='Remove local edit lock'),scroll=p?.closest('.canvas-scroll');return {ok:!!document.querySelector('.canvas-dialog')&&!document.getElementById('canvas-text-editor'),panel:!!p,reviewActionPresent:[...document.querySelectorAll('.canvas-toolbar button')].some(b=>b.textContent==='Review saved change'),editorAbsent:!document.getElementById('canvas-text-editor'),startTyping:(()=>{const n=p?.querySelector('[data-review-start-typing]');return n?{originalBeforeText:n.querySelector('[data-review-start-before] .canvas-review-preview')?.textContent??null,originalAfterText:n.querySelector('[data-review-start-after] .canvas-review-preview')?.textContent??null,newText:n.querySelector('[data-review-start-new] .canvas-review-preview')?.textContent??null,emptyLabel:n.querySelector('[data-empty-paragraph]')?.textContent??null,currentText:n.querySelector('[data-review-start-current] .canvas-review-preview')?.textContent??null}:null})(),insertion:(()=>{const n=p?.querySelector("[data-review-insertion]");return n?{anchor:n.dataset.anchor,side:n.dataset.side,draftText:n.querySelector("[data-review-insert-draft] .canvas-review-preview")?.textContent??null,emptyLabel:n.querySelector("[data-empty-paragraph]")?.textContent??null,currentText:n.querySelector("[data-review-insert-current] .canvas-review-preview")?.textContent??null}:null})(),secondMiddle:(()=>{const n=p?.querySelector('[data-review-second-middle]');return n?{target:n.dataset.target,before:n.querySelector('[data-second-before] .canvas-review-preview').textContent,left:n.querySelector('[data-second-left] .canvas-review-preview').textContent,right:n.querySelector('[data-second-right] .canvas-review-preview').textContent,creation:[...p.querySelectorAll('[data-second-member]')].sort((a,b)=>Number(a.dataset.creationIndex)-Number(b.dataset.creationIndex)).map(n=>({kind:n.dataset.secondMember,text:n.querySelector('.canvas-review-preview').textContent,emptyLabel:n.querySelector('[data-member-empty]')?.textContent??null})),visualOrder:[...p.querySelectorAll('[data-second-member]')].map(n=>n.dataset.secondMember),historyNotice:n.querySelector('[data-second-history]').textContent}:null})(),parentHistory:(()=>{const n=p?.querySelector('[data-review-parent-history]');return n?{text:n.querySelector('.canvas-review-preview').textContent,historyNotice:n.querySelector('p').textContent,emptyLabel:p.querySelector('[data-parent-empty]')?.textContent??null}:null})(),preEnter:(()=>{const n=p?.querySelector('[data-review-pre-enter]');return n?{text:n.querySelector('.canvas-review-preview').textContent,historyNotice:n.querySelector('p').textContent}:null})(),initialTail:(()=>{const n=p?.querySelector('[data-review-initial-child]');return n?{text:n.querySelector('.canvas-review-preview').textContent}:null})(),originalChild:(()=>{const n=p?.querySelector('[data-review-original-child]');return n?{text:n.querySelector('.canvas-review-preview').textContent,kind:n.dataset.contentKind,label:n.querySelector('[data-content-label]').textContent}:null})(),emptyLabels:[...(p?.querySelectorAll('[data-empty-enter-label]')||[])].map(n=>({kind:n.dataset.emptyEnterLabel,text:n.textContent})),sequence:p?.querySelector('[data-review-sequence="original"]')?{originalText:p.querySelector('[data-review-sequence="original"] .canvas-review-preview').textContent,parentText:p.querySelector('[data-review-sequence="parent"] .canvas-review-preview').textContent,parentReportCount:Number(p.querySelector('[data-review-sequence="parent"]').dataset.parentReports),parentEmptyLabel:p.querySelector('[data-parent-empty]')?.textContent??null,historyNotice:!!p.querySelector('[data-review-sequence-history]'),members:[...p.querySelectorAll('[data-review-sequence="member"]')].map(n=>({id:n.dataset.memberId,index:Number(n.dataset.memberIndex),text:n.querySelector(':scope > .canvas-review-preview').textContent,reused:n.dataset.memberReused==='true',sourceText:n.querySelector('.canvas-review-source')?.textContent??null,reportCount:Number(n.dataset.memberReports),emptyLabel:n.querySelector('[data-member-empty]')?.textContent??null}))}:null,split:[...(p?.querySelectorAll('[data-review-split]')||[])].map(n=>({kind:n.dataset.reviewSplit,text:n.querySelector('.canvas-review-preview').textContent})),current:[...(p?.querySelectorAll('[data-review-current]')||[])].map(n=>({id:n.dataset.reviewCurrent,deleted:n.dataset.reviewDeleted==='true',text:n.querySelector('.canvas-review-preview').textContent,...(n.querySelector('[data-content-label]')?{contentKind:n.querySelector('[data-content-label]').dataset.contentKind,contentLabel:n.querySelector('[data-content-label]').textContent}:{})})),missing:[...(p?.querySelectorAll('[data-review-missing]')||[])].map(n=>n.dataset.reviewMissing),warning:p?.textContent??null,checked:c?.checked??null,checkboxDisabled:c?.disabled??null,cancelDisabled:cancel?.disabled??null,submitDisabled:remove?.disabled??null,error:p?.querySelector('[role=alert]')?.textContent??null,scroll:scroll?.scrollTop??null,viewport:[innerWidth,innerHeight]};})()"#;
async fn open(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    wait(desktop, OPEN)
        .await
        .map_err(|e| format!("Review open readiness: {e}"))?;
    click(desktop, "#split-review-open")
        .await
        .map_err(|e| format!("Review open click: {e}"))?;
    wait(desktop, &ready())
        .await
        .map_err(|e| format!("Review panel readiness: {e}"))?;
    Ok(())
}
async fn cancel(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    // The stale/failed panel intentionally disables its checkbox and submit.
    checked(desktop,r#"(()=>{const b=[...document.querySelectorAll('.canvas-local-review button')].find(b=>b.textContent==='Cancel');if(!b||b.disabled)return {ok:false};b.id='split-review-cancel';return {ok:true};})()"#).await?;
    super::continuation_check::visible(desktop, "#split-review-cancel")
        .await
        .map_err(|e| format!("Review Cancel visibility: {e}"))?;
    click(desktop, "#split-review-cancel")
        .await
        .map_err(|e| format!("Review Cancel click: {e}"))?;
    wait(
        desktop,
        r#"({ok:!document.querySelector('.canvas-local-review')})"#,
    )
    .await?;
    Ok(())
}
async fn refresh(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    locked: bool,
) -> Result<(), String> {
    let before = checklist::presentation(&state.peek()).ok_or("Refresh owner unavailable")?;
    checked(desktop,r#"(()=>{const b=[...document.querySelectorAll('.canvas-toolbar button')].find(b=>b.textContent==='Refresh');if(!b||b.disabled)return {ok:false};b.id='split-review-refresh';return {ok:true};})()"#).await?;
    click(desktop, "#split-review-refresh")
        .await
        .map_err(|e| format!("Review Refresh click: {e}"))?;
    // A DOM click returns before the Rust event begins its async journal read.
    // Wait for that new read to finish before selecting a toolbar node that the
    // loading render may remove and recreate.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let ready = {
            let shell = state.peek();
            fixture::enabled(&shell)
                && checklist::presentation(&shell).is_some_and(|view| {
                    view.visit == before.visit
                        && view.read != before.read
                        && !view.loading
                        && !view.stale
                        && view.error.is_none()
                        && if locked {
                            checklist::recovery::review::available(&shell)
                        } else {
                            view.capacity_reason.is_none()
                        }
                })
        };
        if ready {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("Review Refresh did not complete a new canonical and journal read".into());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}
async fn acknowledge(desktop: &dioxus::desktop::DesktopContext) -> Result<Value, String> {
    reveal_review_controls(desktop).await?;
    checked(desktop, &review_controls_script("check", false)).await?;
    wait(desktop, &review_controls_script("inspect", true)).await
}
async fn retained(before: &Receipt) -> Result<(), String> {
    if stored().await?.as_ref() != Some(before) {
        return Err("Whole split receipt changed".into());
    }
    let audit = journal(Some(before)).await?;
    if audit["archiveCount"] != 0 || audit["archivedExact"] != false {
        return Err("Undecided split was archived".into());
    }
    Ok(())
}
async fn capture(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    mode: &str,
    name: &str,
    ordinal: usize,
    before: Option<&Receipt>,
    geometry: Option<Value>,
) -> Result<(), String> {
    let (canonical, dispatches, owner_valid, owner) = {
        let shell = state.peek();
        if !fixture::requested() || shell.core.session.is_some() || shell.core.transport.is_some() {
            return Err("Connected capture refused".into());
        }
        let f = shell
            .canvas_checklist
            .structure_fixture
            .as_ref()
            .ok_or("Missing recovery scene")?;
        (
            super::split_fixture::inventory(f.canonical.as_ref().unwrap()),
            f.attempts + f.text_attempts,
            checklist::presentation(&shell).is_some(),
            json!({"account":shell.core.active_account,"epoch":shell.core.account_epoch,"workspace":shell.core.active_team,"user":shell.self_account.user_id}),
        )
    };
    if dispatches != 0 {
        return Err("Restored split created an edit attempt".into());
    }
    let preview = checked(desktop, PREVIEW).await?;
    let journal = journal(before).await?;
    let directory = std::path::PathBuf::from(
        std::env::var_os("SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR")
            .ok_or("Missing capture directory")?,
    );
    if !directory.is_absolute() {
        return Err("Capture directory must be absolute".into());
    }
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    eprintln!(
        "canvas-split-recovery-stage: {}",
        json!({"ok":true,"mode":mode,"corpus":super::split_fixture::corpus().name,"splitMode":super::split_fixture::mode(),"sequenceCount":super::split_fixture::sequence::count(),"sequenceHistory":super::split_fixture::sequence::history(),"sequenceLayout":super::split_fixture::sequence::layout(),"sequenceCadence":super::split_fixture::sequence::cadence(),"repeatHistory":super::split_fixture::repeat_history(),"repeatLayout":super::split_fixture::repeat_layout(),"emptyHistory":super::split_fixture::empty_history(),"emptyLayout":super::split_fixture::empty_layout(),"typing":super::split_fixture::typing(),"parentMode":super::split_fixture::parent_mode(),"preEnter":super::split_fixture::pre_enter::selector(),"middleParent":super::split_fixture::parent_edit::selector(),"middleCadence":super::split_fixture::parent_edit::cadence(),"secondMiddle":super::split_fixture::second_middle::selector(),"secondMiddleCadence":super::split_fixture::second_middle::cadence(),"parentId":super::split_fixture::parent_id(),"reuseBase":super::split_fixture::reuse_base(),"reuseContent":super::split_fixture::reuse_content(),"reuseChild":super::split_fixture::is_reuse().then(super::split_fixture::reuse_child),"stage":name,"ordinal":ordinal,"noSession":true,"noTransport":true,"dispatches":dispatches,"journal":journal,"canonical":canonical,"ownerValid":owner_valid,"owner":owner,"preview":preview,"reviewGeometry":geometry})
    );
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let p = directory.join(format!("{ordinal}.ack"));
        match tokio::task::spawn_blocking(move || std::fs::read(p))
            .await
            .map_err(|_| "Capture worker failed")?
        {
            Ok(bytes) if bytes == b"captured\n" => return Ok(()),
            Ok(_) => return Err("Malformed capture ACK".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err("Capture ACK unreadable".into()),
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!("Capture timed out: {name}"));
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    if !super::split_fixture::corpus_valid()
        || !super::split_fixture::mode_valid()
        || !super::split_fixture::typing_valid()
        || !super::split_fixture::parent_valid()
        || !super::split_fixture::pre_enter::valid()
        || !super::split_fixture::parent_edit::valid()
        || !super::split_fixture::second_middle::valid()
        || !super::split_fixture::empty_valid()
        || !super::split_fixture::repeat_valid()
        || !super::split_fixture::sequence::valid()
        || !super::split_fixture::reuse_valid()
    {
        return Err("Unknown split recovery corpus".into());
    }
    let mode = std::env::var("SUPER_PLATINUM_CANVAS_SPLIT_RECOVERY_CHECK").unwrap();
    path()?;
    wait(
        desktop,
        r#"({ok:!!document.querySelector('.canvas-dialog [data-structure-action]')})"#,
    )
    .await?;
    if !fixture::enabled(&state.peek()) {
        return Err("Expected disconnected split recovery owner".into());
    }
    read(state).await?;
    let original = fixture::snapshot();
    let mut account_guard = Value::Null;
    if mode == "seed" {
        if stored().await?.is_some() || journal(None).await?["archiveCount"] != 0 {
            return Err("Seed journal not empty".into());
        }
        let checkpoint = fixture::checkpoint();
        let p = path()?;
        let started = tokio::task::spawn_blocking(move || {
            Journal::open(p, fixture::ACCOUNT).and_then(|mut j| j.begin(checkpoint))
        })
        .await
        .map_err(|_| "Seed worker failed")?
        .map_err(|_| "Seed commit failed")?;
        let Begin::Started(receipt) = started else {
            return Err("Seed receipt not fresh".into());
        };
        read(state).await?;
        controls(desktop, true).await?;
        capture(state, desktop, &mode, "seeded", 0, Some(&receipt), None).await?;
        return terminal(state, desktop, &mode, Some(&receipt), true, Value::Null).await;
    }
    if mode == "reviewed" {
        let j = journal(None).await?;
        if j["active"] != Value::Null || j["archiveCount"] != 1 {
            return Err("Archived reopen lost whole decision".into());
        }
        controls(desktop, false).await?;
        capture(state, desktop, &mode, "archive-restored", 0, None, None).await?;
        return terminal(state, desktop, &mode, None, false, Value::Null).await;
    }
    let before = stored().await?.ok_or("Restored pair missing")?;
    if !fixture::matches_parent_mode(before.checkpoint())
        || !fixture::matches_typing(before.checkpoint())
        || !fixture::matches_reuse_base(before.checkpoint())
    {
        return Err("Restored pair has a different typing history".into());
    }
    if if super::split_fixture::is_reuse() {
        !before.checkpoint().is_end_reuse()
    } else if super::split_fixture::is_start_typing() {
        !before.checkpoint().is_start_typing()
    } else if super::split_fixture::is_start() {
        before.checkpoint().is_split()
            || before.checkpoint().is_end()
            || before.checkpoint().is_delete()
    } else if super::split_fixture::is_end() {
        !before.checkpoint().is_end() || before.checkpoint().is_end_reuse()
    } else {
        !before.checkpoint().is_split()
    } {
        return Err("Expected entire split checkpoint".into());
    }
    controls(desktop, true).await?;
    retained(&before).await?;
    if mode == "restart" {
        capture(state, desktop, &mode, "restored", 0, Some(&before), None).await?;
        read(state).await?;
        let target = state.peek().canvas.clone().unwrap();
        state.write().dismiss_overlay();
        wait(
            desktop,
            r#"({ok:!document.querySelector('.canvas-dialog')})"#,
        )
        .await?;
        crate::canvas::open_target(state, target);
        controls(desktop, true).await?;
        retained(&before).await?;
        capture(state, desktop, &mode, "reopened", 1, Some(&before), None).await?;
        return terminal(state, desktop, &mode, Some(&before), true, Value::Null).await;
    }
    open(desktop).await?;
    let mut ordinal = 0;
    if super::split_fixture::second_middle::enabled() {
        // A scrollable review can exceed a short viewport. Inspect every bounded
        // semantic section separately, without weakening its visibility bounds.
        let canonical = state
            .peek()
            .canvas_checklist
            .structure_fixture
            .as_ref()
            .and_then(|f| f.canonical.clone())
            .ok_or("Missing review canonical source")?;
        for (index, (name, _, _, _)) in second_middle::STAGES.iter().enumerate() {
            retained(&before).await?;
            let geometry = second_middle::reveal(desktop, index).await?;
            if state
                .peek()
                .canvas_checklist
                .structure_fixture
                .as_ref()
                .and_then(|f| f.canonical.as_ref())
                .is_none_or(|current| !std::sync::Arc::ptr_eq(current, &canonical))
            {
                return Err("Second-middle review canonical source changed".into());
            }
            capture(
                state,
                desktop,
                &mode,
                name,
                ordinal,
                Some(&before),
                Some(geometry),
            )
            .await?;
            retained(&before).await?;
            if state
                .peek()
                .canvas_checklist
                .structure_fixture
                .as_ref()
                .and_then(|f| f.canonical.as_ref())
                .is_none_or(|current| !std::sync::Arc::ptr_eq(current, &canonical))
            {
                return Err("Second-middle review canonical source changed during capture".into());
            }
            ordinal += 1;
        }
    } else {
        let pre_enter_geometry = if super::split_fixture::parent_edit::enabled() {
            Some(parent_edit::reveal(desktop).await?)
        } else if super::split_fixture::pre_enter::enabled() {
            Some(pre_enter::reveal(desktop).await?)
        } else {
            None
        };
        capture(
            state,
            desktop,
            &mode,
            "review-pair",
            ordinal,
            Some(&before),
            pre_enter_geometry,
        )
        .await?;
        ordinal += 1;
    }
    if mode == "review" {
        reveal_review_controls(desktop).await?;
        let geometry = checked(desktop, &review_controls_script("inspect", false)).await?;
        capture(
            state,
            desktop,
            &mode,
            "review-controls",
            ordinal,
            Some(&before),
            Some(geometry),
        )
        .await?;
        ordinal += 1;
        checked(desktop, &review_controls_script("cancel", false)).await?;
        wait(
            desktop,
            r#"({ok:!document.querySelector('.canvas-local-review')})"#,
        )
        .await?;
        retained(&before).await?;
        capture(
            state,
            desktop,
            &mode,
            "cancelled",
            ordinal,
            Some(&before),
            None,
        )
        .await?;
        ordinal += 1;
        open(desktop).await?;
        acknowledge(desktop).await?;
        fixture::set_snapshot(&mut state.write(), fixture::changed_snapshot());
        invalidated(desktop).await?;
        retained(&before).await?;
        capture(state, desktop, &mode, "stale", ordinal, Some(&before), None).await?;
        ordinal += 1;
        cancel(desktop).await?;
        fixture::set_snapshot(&mut state.write(), original.clone());
        refresh(state, desktop, true).await?;
        open(desktop).await?;
        acknowledge(desktop).await?;
        let epoch = state.peek().core.account_epoch;
        let before_read = checklist::presentation(&state.peek())
            .ok_or("Original review read missing")?
            .read;
        {
            let mut shell = state.write();
            shell.core.account_epoch = epoch.wrapping_add(1);
            shell.core.active_account = Some("00000000-0000-4000-8000-000000009993".into());
        }
        // CanvasDialog is keyed by account_epoch. Replacing it must destroy
        // the old acknowledgement, not retain a disabled old-account panel.
        wait(desktop, r#"(()=>{const p=document.querySelector('.canvas-local-review'),review=[...document.querySelectorAll('.canvas-toolbar button')].some(b=>b.textContent==='Review saved change');return {ok:!p&&!review&&!document.getElementById('canvas-text-editor')};})()"#)
            .await.map_err(|e|format!("Account replacement did not discard old review: {e}"))?;
        retained(&before).await?;
        {
            let shell = state.peek();
            if shell.core.account_epoch != epoch.wrapping_add(1)
                || shell.core.active_account.as_deref()
                    != Some("00000000-0000-4000-8000-000000009993")
                || checklist::presentation(&shell).is_some()
            {
                return Err(
                    "Account replacement was reseeded or adopted old read ownership".into(),
                );
            }
            account_guard = json!({"beforeEpoch":epoch,"beforeRead":before_read,"changedEpoch":shell.core.account_epoch,
                "changedAccount":shell.core.active_account,"oldReviewRemoved":true,
                "oldReviewActionAbsent":true,"receiptPreserved":true});
        }
        capture(
            state,
            desktop,
            &mode,
            "account-changed",
            ordinal,
            Some(&before),
            None,
        )
        .await?;
        ordinal += 1;
        {
            let mut shell = state.write();
            shell.core.account_epoch = epoch.wrapping_add(2);
            shell.core.active_account = Some(fixture::ACCOUNT.into());
        }
        // Returning to the original synthetic owner remounts a new reader and
        // reloads its durable lock; never restore an earlier scope generation.
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let ready = {
                let shell = state.peek();
                fixture::enabled(&shell)
                    && shell.core.account_epoch == epoch.wrapping_add(2)
                    && checklist::presentation(&shell).is_some_and(|view| view.read != before_read)
                    && checklist::recovery::review::available(&shell)
            };
            if ready {
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                return Err("Original account did not reload its retained pair".into());
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        retained(&before).await?;
        account_guard["restoredRead"] = json!(
            checklist::presentation(&state.peek())
                .ok_or("Restored review read missing")?
                .read
        );
        account_guard["restoredEpoch"] = json!(state.peek().core.account_epoch);
        account_guard["restoredAccount"] = json!(state.peek().core.active_account);
        open(desktop).await?;
    }
    let geometry = acknowledge(desktop).await?;
    capture(
        state,
        desktop,
        &mode,
        "review-confirmation",
        ordinal,
        Some(&before),
        Some(geometry),
    )
    .await?;
    ordinal += 1;
    checked(desktop, &review_controls_script("remove", true)).await?;
    if mode == "storage-failure" {
        wait(desktop,r#"(()=>{const p=document.querySelector('.canvas-local-review');return {ok:p?.querySelector('[role=alert]')?.textContent==='The local record could not be updated. It remains locked. Refresh to check its state.'&&p.querySelector('input').disabled};})()"#).await?;
        retained(&before).await?;
        capture(
            state,
            desktop,
            &mode,
            "storage-failed",
            ordinal,
            Some(&before),
            None,
        )
        .await?;
        ordinal += 1;
        cancel(desktop).await?;
        refresh(state, desktop, true).await?;
        open(desktop).await?;
        let geometry = acknowledge(desktop).await?;
        capture(
            state,
            desktop,
            &mode,
            "retry-confirmation",
            ordinal,
            Some(&before),
            Some(geometry),
        )
        .await?;
        ordinal += 1;
        checked(desktop, &review_controls_script("remove", true)).await?;
    }
    wait(
        desktop,
        r#"({ok:!document.querySelector('.canvas-local-review')})"#,
    )
    .await?;
    let j = journal(Some(&before)).await?;
    if j["active"] != Value::Null || j["archivedExact"] != true || j["archiveCount"] != 1 {
        return Err("Exact pair archive missing before unlock".into());
    }
    if checklist::presentation(&state.peek()).is_none_or(|v| !v.stale || v.operation.is_some()) {
        return Err("Local removal skipped refresh or recreated operation".into());
    }
    capture(
        state,
        desktop,
        &mode,
        "removed",
        ordinal,
        Some(&before),
        None,
    )
    .await?;
    ordinal += 1;
    refresh(state, desktop, false).await?;
    controls(desktop, false).await?;
    capture(
        state,
        desktop,
        &mode,
        "refreshed",
        ordinal,
        Some(&before),
        None,
    )
    .await?;
    terminal(state, desktop, &mode, Some(&before), false, account_guard).await
}
async fn invalidated(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    wait(desktop,r#"(()=>{const p=document.querySelector('.canvas-local-review'),c=p?.querySelector('input'),b=[...(p?.querySelectorAll('button')||[])].find(b=>b.textContent==='Remove local edit lock');return {ok:!!c&&c.disabled&&b?.disabled&&p.querySelector('[role=alert]')?.textContent==='This canvas changed. Cancel and refresh before reviewing again.'};})()"#).await?;
    checked(desktop,r#"(()=>{const b=[...document.querySelectorAll('.canvas-local-review button')].find(b=>b.textContent==='Remove local edit lock');b.click();return {ok:b.disabled};})()"#).await?;
    Ok(())
}
async fn terminal(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    mode: &str,
    before: Option<&Receipt>,
    locked: bool,
    account_guard: Value,
) -> Result<Value, String> {
    let layout = controls(desktop, locked).await?;
    let journal = journal(before).await?;
    let shell = state.peek();
    let f = shell.canvas_checklist.structure_fixture.as_ref().unwrap();
    let canonical = f.canonical.as_ref().unwrap();
    if canonical.sections != fixture::snapshot().sections
        || f.attempts != 0
        || f.text_attempts != 0
        || checklist::presentation(&shell).is_none_or(|v| v.operation.is_some())
    {
        return Err("Review changed canonical data or created an operation".into());
    }
    Ok(
        json!({"ok":true,"mode":mode,"corpus":super::split_fixture::corpus().name,"splitMode":super::split_fixture::mode(),"sequenceCount":super::split_fixture::sequence::count(),"sequenceHistory":super::split_fixture::sequence::history(),"sequenceLayout":super::split_fixture::sequence::layout(),"sequenceCadence":super::split_fixture::sequence::cadence(),"repeatHistory":super::split_fixture::repeat_history(),"repeatLayout":super::split_fixture::repeat_layout(),"emptyHistory":super::split_fixture::empty_history(),"emptyLayout":super::split_fixture::empty_layout(),"typing":super::split_fixture::typing(),"parentMode":super::split_fixture::parent_mode(),"preEnter":super::split_fixture::pre_enter::selector(),"middleParent":super::split_fixture::parent_edit::selector(),"middleCadence":super::split_fixture::parent_edit::cadence(),"secondMiddle":super::split_fixture::second_middle::selector(),"secondMiddleCadence":super::split_fixture::second_middle::cadence(),"parentId":super::split_fixture::parent_id(),"reuseBase":super::split_fixture::reuse_base(),"reuseContent":super::split_fixture::reuse_content(),"reuseChild":super::split_fixture::is_reuse().then(super::split_fixture::reuse_child),"noSession":true,"noTransport":true,"noReplay":true,"outcomeUnknown":true,"canonicalUnchanged":true,"accountGuard":account_guard,"dispatches":f.attempts+f.text_attempts,"reservations":usize::from(mode=="seed"),"retirements":0,"journal":journal,"canonical":super::split_fixture::inventory(canonical),"layout":layout}),
    )
}
