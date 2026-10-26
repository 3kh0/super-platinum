//! Fresh processes import raw canonical bytes; only the production journal
//! restore worker can recover authority. Existing paired Save/capture paths stay shared.
use super::*;
use capture::{Record, click, model, open, row, save};

pub(in crate::canvas::text::owner_delete_fixture) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-confirmed-provenance" | "canvas-confirmed-provenance-narrow")
        )
}
pub(super) fn case() -> String {
    env("SUPER_PLATINUM_CANVAS_CONFIRMED_PROVENANCE_CASE", "seed")
}
pub(in crate::canvas::text::owner_delete_fixture) fn cold() -> bool {
    requested() && case() != "seed"
}
/// Called only inside the existing import worker, on the phase's private clone.
pub(in crate::canvas::text::owner_delete_fixture) fn imported(
    source: Arc<CanvasCollabSnapshot>,
) -> Arc<CanvasCollabSnapshot> {
    if case() != "changed" {
        return source;
    }
    let changed = pair::preflight_source(&source, "mutated");
    std::fs::write(
        snapshot_path(),
        serde_json::to_vec(&seed::saved(&changed)).unwrap(),
    )
    .unwrap();
    changed
}
pub(in crate::canvas::text::owner_delete_fixture) async fn reload(mut state: Signal<ShellState>) {
    let source = snapshot(&state.peek());
    let Some(mut read) = checklist::begin_read(&mut state.write()) else {
        return;
    };
    checklist::recovery::load(state, &read).await;
    let previous_was_absent =
        checklist::presentation(&state.peek()).is_some_and(|v| v.snapshot.is_none());
    let mut previous_raw = Value::Null;
    // The second cold reopen also proves Refresh from a previously published
    // raw server read. It never seeds a certificate; full Inventory rejects it.
    if case() == "reopen" && previous_was_absent {
        let raw = source.clone();
        let unproven = tokio::task::spawn_blocking(move || {
            let target = raw.sections.iter().find(|s| s.section_type().is_ok_and(|kind| kind.unwrap_or(0) == 0) && s.deleted().ok().flatten() != Some(true)).unwrap();
            json!({"canonical":seed::saved(&raw),"replacementRefused":ReplacementIntent::main_canvas(&raw,target.id(),checklist::save::now_usec().unwrap()).is_err()})
        }).await.unwrap();
        if unproven["replacementRefused"] != true {
            return;
        }
        if !checklist::finish_read(&mut state.write(), &read, Ok(source.clone())) {
            return;
        }
        previous_raw = json!({"read":checklist::presentation(&state.peek()).map(|v|v.read),"unproven":unproven});
        let Some(next) = checklist::begin_read(&mut state.write()) else {
            return;
        };
        read = next;
        checklist::recovery::load(state, &read).await;
    }
    let fresh = checklist::replacement_ops::readback::carry_for_read(state, &read, source).await;
    let error = fresh.as_ref().err().copied();
    let certified = fresh.as_ref().ok().cloned();
    if !checklist::finish_read(&mut state.write(), &read, fresh) {
        return;
    }
    let mut shell = state.write();
    if let Some(snapshot) = certified {
        shell
            .canvas_checklist
            .structure_fixture
            .as_mut()
            .unwrap()
            .canonical = Some(snapshot);
    }
    let read_serial = checklist::presentation(&shell).map(|v| v.read);
    fixture(&mut shell).last_live.restore = Some(
        json!({"case":case(),"previousWasAbsent":previous_was_absent,"previousRaw":previous_raw,"read":read_serial,"error":error,"sharedRestore":true}),
    );
}
async fn sole(state: Signal<ShellState>) -> Result<String, String> {
    let source = snapshot(&state.peek());
    tokio::task::spawn_blocking(move || {
        let ids: Vec<_> = source
            .sections
            .iter()
            .filter(|s| {
                s.section_type().is_ok_and(|kind| kind.unwrap_or(0) == 0)
                    && s.deleted().ok().flatten() != Some(true)
            })
            .map(|s| s.id().to_owned())
            .collect();
        if ids.len() != 1 {
            return Err(format!("Expected sole live paragraph, found {ids:?}"));
        }
        Ok(ids[0].clone())
    })
    .await
    .map_err(|_| "Live identity worker failed")?
}
async fn cancel(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    record: &mut Record,
    target: &str,
) -> Result<(), String> {
    let action = open(desktop, target, true).await?;
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
    if model(state).await? != before || !fixture(&mut state.write()).writes.is_empty() {
        return Err("Cold confirmation Cancel changed source or dispatched".into());
    }
    record
        .shot(
            state,
            desktop,
            "cancelled",
            row(desktop, target, true).await?,
            action,
        )
        .await
}
async fn pending(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    record: &mut Record,
    target: &str,
) -> Result<Value, String> {
    let action = open(desktop, target, true).await?;
    record
        .shot(
            state,
            desktop,
            "confirmation",
            editor_layout(desktop).await?,
            action,
        )
        .await?;
    let action = save(desktop).await?;
    interaction::reserved(state, desktop).await?;
    record
        .shot(
            state,
            desktop,
            "reserved",
            editor_layout(desktop).await?,
            action,
        )
        .await?;
    let id = pair::release(state).await?;
    interaction::saving(desktop).await?;
    record
        .shot(
            state,
            desktop,
            "saving",
            editor_layout(desktop).await?,
            json!({"replacement":id}),
        )
        .await?;
    let receipt = recovery::stored().await?.ok_or("Missing pending receipt")?;
    let attempt = pair::complete(state, "unknown").await?;
    pair::read_and_settle(state, true).await?;
    if recovery::stored().await?.as_ref() != Some(&receipt)
        || checklist::text_ops::settled(&state.peek(), &attempt)
    {
        return Err("Unknown result used older confirmed authority to unlock".into());
    }
    wait(desktop,r#"({ok:document.getElementById('canvas-text-save')?.disabled&&document.getElementById('canvas-text-cancel')?.disabled})"#).await?;
    record
        .shot(
            state,
            desktop,
            "unknown-locked",
            editor_layout(desktop).await?,
            json!({"exactReceiptRetained":true}),
        )
        .await?;
    Ok(json!({"pairs":1,"textEdits":0,"awaitFreshProcess":true,"noReplay":true}))
}
async fn changed(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    record: &mut Record,
) -> Result<Value, String> {
    wait(desktop,r#"({ok:document.querySelector('.canvas-dialog')?.textContent.includes('Saved canvas history changed')&&!document.getElementById('canvas-text-editor')})"#).await?;
    let before = model(state).await?;
    if before["view"]["canonical"] != Value::Null
        || before["view"]["error"].is_null()
        || recovery::stored().await?.is_some()
    {
        return Err("Damaged confirmed history was published as authoritative".into());
    }
    record.shot(state, desktop, "restore-refused", check::layout(desktop).await?, json!({"changedSection":"paragraph","changedTag":2,"changedValue":9001,"rawImportedOnly":true})).await?;
    let action=checked(desktop,r#"(()=>{const b=[...document.querySelectorAll('.canvas-paragraph-actions button')];if(b.some(b=>!b.disabled))return{ok:false};b.forEach(b=>b.click());return{ok:!document.getElementById('canvas-text-editor'),attempted:b.length,allDisabled:true}})()"#).await?;
    if before != model(state).await? || !fixture(&mut state.write()).writes.is_empty() {
        return Err("Refused restored source became writable".into());
    }
    record
        .shot(
            state,
            desktop,
            "actions-refused",
            check::layout(desktop).await?,
            action,
        )
        .await?;
    Ok(json!({"restoreRefused":true,"noReplay":true,"pairs":0,"textEdits":0}))
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    record: &mut Record,
) -> Result<Value, String> {
    let phase = case();
    if !matches!(
        phase.as_str(),
        "seed" | "resume" | "reopen" | "changed" | "pending" | "pending-reopen"
    ) {
        return Err("Unknown confirmed phase".into());
    }
    record.ready().await?;
    // Import/recovery completes before any action-row predicate; an export-only
    // first frame cannot be used as a writable or restored-state witness.
    let mut ready = false;
    for _ in 0..500 {
        if enabled(&state.peek())
            && checklist::presentation(&state.peek())
                .is_some_and(|v| !v.loading && (v.snapshot.is_some() || v.error.is_some()))
        {
            ready = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    if !ready {
        return Err("Canonical restore did not complete".into());
    }
    wait(
        desktop,
        r#"({ok:!!document.querySelector('.canvas-dialog')})"#,
    )
    .await?;
    if phase == "changed" {
        return changed(state, desktop, record).await;
    }
    if phase == "pending-reopen" {
        return interaction::failure::restart(state, desktop, record).await;
    }
    if recovery::stored().await?.is_some() {
        return Err("Expected no active receipt before writable phase".into());
    }
    let target = sole(state).await?;
    let initial = if phase == "seed" {
        "initial"
    } else if phase == "reopen" {
        "cumulative-restored"
    } else {
        "restored"
    };
    record
        .shot(
            state,
            desktop,
            initial,
            row(desktop, &target, false).await?,
            json!({"target":target,"freshProcess":phase!="seed","rawImportedOnly":true}),
        )
        .await?;
    if phase == "seed" || phase == "reopen" {
        cancel(state, desktop, record, &target).await?;
        if phase == "reopen" {
            return Ok(
                json!({"freshProcess":true,"noReplay":true,"pairs":0,"textEdits":0,"target":target}),
            );
        }
        let id = interaction::cycle(state, desktop, record, &target, false).await?;
        return Ok(
            json!({"pairs":1,"textEdits":0,"cancelNoWrite":true,"finalReplacement":id,"allRetainedRowsExact":true}),
        );
    }
    if phase == "pending" {
        return pending(state, desktop, record, &target).await;
    }
    interaction::edit(state, desktop, record, &target).await?;
    let id = interaction::cycle(state, desktop, record, &target, true).await?;
    let mut shell = state.write();
    let f = fixture(&mut shell);
    if f.writes.len() != 2 || f.acknowledgements.len() != 2 {
        return Err("Cold edit/repeat count differs".into());
    }
    Ok(
        json!({"freshProcess":true,"pairs":1,"textEdits":1,"firstReplacement":target,"finalReplacement":id,"allRetainedRowsExact":true}),
    )
}
