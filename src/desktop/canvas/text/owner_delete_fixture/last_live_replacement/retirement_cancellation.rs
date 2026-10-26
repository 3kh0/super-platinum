//! Real component Refresh/Close lifetime; only storage completion is gated.
use super::*;
use crate::canvas::checklist::recovery::cancellation_fixture as gate;
use capture::{Record, click, model, open, row, save};

pub(in crate::canvas::text::owner_delete_fixture) fn requested() -> bool {
    gate::requested()
}
pub(super) fn case() -> String {
    env(
        "SUPER_PLATINUM_CANVAS_RETIREMENT_CANCELLATION_CASE",
        "precommit",
    )
}

/// Called by loader::load under the actual Canvas Refresh component task.
pub(in crate::canvas::text::owner_delete_fixture) async fn reload(state: Signal<ShellState>) {
    if gate::proof(&state.peek())["phase"] != "armed" {
        confirmed::reload(state).await;
        return;
    }
    let Ok(read) = pair::read_and_settle(state, false).await else {
        return;
    };
    let Some(mut witness) = gate::waiter(&state.peek(), &read) else {
        return;
    };
    checklist::recovery::settle(state, &read).await;
    witness.returned();
}

async fn reached(
    state: Signal<ShellState>,
    predicate: impl Fn(&Value) -> bool,
) -> Result<Value, String> {
    for _ in 0..500 {
        let proof = gate::proof(&state.peek());
        if predicate(&proof) {
            return Ok(proof);
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    Err(format!(
        "Retirement boundary not reached: {}",
        gate::proof(&state.peek())
    ))
}
async fn refresh(desktop: &dioxus::desktop::DesktopContext) -> Result<Value, String> {
    checked(desktop,r#"(()=>{const b=[...document.querySelectorAll('.canvas-toolbar button')].find(b=>b.textContent==='Refresh');if(!b||b.disabled)return{ok:false};b.id='retirement-refresh';return{ok:true}})()"#).await?;
    click(desktop, "#retirement-refresh").await
}
async fn closed(desktop: &dioxus::desktop::DesktopContext) -> Result<Value, String> {
    wait(desktop,r#"({ok:!document.querySelector('.canvas-dialog'),closed:true,viewport:[innerWidth,innerHeight]})"#).await
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    record: &mut Record,
) -> Result<Value, String> {
    let boundary = case();
    if !matches!(boundary.as_str(), "precommit" | "postcommit") {
        return Err("Unknown cancellation boundary".into());
    }
    record.ready().await?;
    row(desktop, "neighbor", false).await?;
    let initial = snapshot(&state.peek());
    let target = state.peek().canvas.clone().ok_or("Missing Canvas target")?;
    open(desktop, "neighbor", true).await?;
    save(desktop).await?;
    interaction::reserved(state, desktop).await?;
    let replacement = pair::release(state).await?;
    interaction::saving(desktop).await?;
    let attempt = pair::complete(state, "success").await?;
    pair::read_and_settle(state, false).await?;
    let receipt = recovery::stored()
        .await?
        .ok_or("Missing accepted receipt")?;
    wait(desktop,r#"({ok:document.getElementById('canvas-text-save')?.disabled&&document.getElementById('canvas-text-cancel')?.disabled&&document.querySelector('.canvas-toolbar')?.textContent.includes('Refresh')})"#).await?;
    record
        .shot(
            state,
            desktop,
            "accepted-locked",
            editor_layout(desktop).await?,
            json!({"operation":receipt.operation_id(),"replacement":replacement}),
        )
        .await?;
    gate::arm(&state.peek(), &boundary)?;
    let action = refresh(desktop).await?;
    let held = reached(state, |p| {
        p["phase"] == "held" && p["entered"] == 1 && p["durableFinishing"] == true
    })
    .await?;
    let pending = recovery::stored().await?;
    if (boundary == "precommit" && pending.as_ref() != Some(&receipt))
        || (boundary == "postcommit" && pending.is_some())
    {
        return Err("Held storage boundary disagrees with pending receipt".into());
    }
    record.shot(state,desktop,"retirement-held",editor_layout(desktop).await?,json!({"action":action,"gate":held,"pendingReceipt":pending.is_some(),"outsideSqliteTransaction":true})).await?;
    let action = click(desktop, "[aria-label='Close canvas']").await?;
    closed(desktop).await?;
    let dropped = reached(state, |p| {
        p["cancelled"] == 1 && p["returned"] == 0 && p["viewPresent"] == false
    })
    .await?;
    record
        .shot(
            state,
            desktop,
            "caller-cancelled",
            closed(desktop).await?,
            json!({"action":action,"gate":dropped,"actualComponentWaiterDropped":true}),
        )
        .await?;
    gate::release()?;
    let retired = reached(state, |p| {
        p["phase"] == "released" && p["operationPresent"] == false && p["durableFinishing"] == false
    })
    .await?;
    if recovery::stored().await?.is_some() || checklist::presentation(&state.peek()).is_some() {
        return Err("Retirement retained receipt or published closed view".into());
    }
    record
        .shot(
            state,
            desktop,
            "retired-closed",
            closed(desktop).await?,
            json!({"gate":retired,"noOldViewPublication":true}),
        )
        .await?;
    crate::canvas::open_target(state, target);
    wait(desktop,r#"({ok:!!document.querySelector('.canvas-dialog')&&!document.getElementById('canvas-text-editor')})"#).await?;
    // Reopen starts export-only. Use the real current Refresh, then exact row mount.
    refresh(desktop).await?;
    let geometry = row(desktop, &replacement, false).await?;
    let view = checklist::presentation(&state.peek()).ok_or("No restored visit")?;
    if view.visit == attempt.visit() || view.operation.is_some() || view.stale || view.loading {
        return Err("Reopened view is not independently writable".into());
    }
    let restored = view.snapshot.ok_or("Missing restored snapshot")?;
    let before = initial.clone();
    let fresh = restored.clone();
    let id = replacement.clone();
    tokio::task::spawn_blocking(move || {
        for old in before.sections.iter().filter(|s| s.id() != "neighbor") {
            if fresh.section(old.id()) != Some(old) {
                return Err("Retained bytes changed");
            }
        }
        ReplacementIntent::main_canvas(&fresh, &id, 100)
            .map(|_| ())
            .map_err(|_| "Restored action refused")
    })
    .await
    .map_err(|_| "Restored action worker failed")??;
    record.shot(state,desktop,"reopened",geometry,json!({"oldVisit":attempt.visit(),"newVisit":view.visit,"sharedRestore":true,"replacement":replacement})).await?;
    open(desktop, &replacement, false).await?;
    let action = click(desktop, "#canvas-text-cancel").await?;
    wait(
        desktop,
        r#"({ok:!document.getElementById('canvas-text-editor')})"#,
    )
    .await?;
    let final_model = model(state).await?;
    if final_model["pairReservations"] != 1
        || final_model["pairDispatches"] != 1
        || final_model["writes"].as_array().map(Vec::len) != Some(1)
        || final_model["acknowledgements"].as_array().map(Vec::len) != Some(1)
    {
        return Err("Cancellation replayed mutation".into());
    }
    pair::retain_confirmed(state).await?;
    record
        .shot(
            state,
            desktop,
            "edit-cancelled",
            row(desktop, &replacement, true).await?,
            action,
        )
        .await?;
    Ok(
        json!({"boundary":boundary,"pairs":1,"noReplay":true,"callerCancelled":true,
        "retiredWhileClosed":true,"reopenedWritable":true,"finalReplacement":replacement}),
    )
}
