//! Explicit disk-only retry and fresh native rebind of the same retained Editor.
use super::*;
use crate::view::composer::{protected_adoption as native, protected_enrollment};
use controller::Panel;

pub(super) fn retry(state: Signal<ShellState>, mut ui: Signal<Panel>) {
    if ui.peek().busy {
        return;
    }
    let Some(b) = binding(&state.peek()) else {
        return;
    };
    if b.scenario != "failed-managed-write-channel" {
        return;
    }
    ui.write().busy = true;
    *b.result.lock().unwrap() = None;
    b.record(json!({"kind":"retry-input-click"}));
    let task = spawn(Box::pin(async move {
        let result=async {
            let receipt={b.host.lock().unwrap().installation(&b.account,&key(b.target()))}.ok_or(durable::Error::Stale)?;
            let editor=receipt.editor();
            let before=editor.snapshot();
            editor.retry_storage()?;
            let after=editor.flush().await?;
            let value=json!({"kind":"input-retried","retentionId":editor.retained_id(),"receiptId":receipt.id(),"serialBefore":before.serial,"serialAfter":after.serial,"review":after.view.inventory().managed().ok().map(super::super::native_fixture::row_json),"pending":after.pending,"failedInputs":after.failed_inputs});
            b.record(value.clone());
            b.reload(state).await.map_err(|_|durable::Error::Stale)?;
            Ok(value)
        }.await;
        if let Ok(mut panel) = ui.try_write() {
            panel.busy = false;
            panel.message = Some(
                if result.is_ok() {
                    "Your retained input was saved. Resume editing to restore its route."
                } else {
                    "Saving still needs attention. Your retained input is kept."
                }
                .into(),
            );
        }
        b.result(result.unwrap_or_else(|error| json!({"kind":"retry-refused","error":err(error)})));
    }));
    WAITER.with_borrow_mut(|old| *old = Some(task));
}

pub(super) fn start(state: Signal<ShellState>, mut ui: Signal<Panel>) {
    if ui.peek().busy || ui.peek().dialog.is_some() {
        return;
    }
    let Some(b) = binding(&state.peek()) else {
        return;
    };
    ui.write().busy = true;
    ui.write().adopting = true;
    *b.result.lock().unwrap() = None;
    let visit = Visit::new();
    *b.install_visit.lock().unwrap() = Some(visit.clone());
    b.record(json!({"kind":"resume-click"}));
    let task = spawn(Box::pin(async move {
        let result=async {
            let prior={b.host.lock().unwrap().installation(&b.account,&key(b.target()))}.ok_or(durable::Error::Stale)?;
            prior.editor().flush().await?;
            b.reload(state).await.map_err(|_|durable::Error::Stale)?;
            let expected={let s=state.peek();let h=b.host.lock().unwrap();h.discovery().entry(&s,&key(b.target())).ok_or(durable::Error::Stale)?.inventory().managed()?.clone()};
            let capture=protected_enrollment::capture(state,b.target(),expected.local()).await?;
            let selected=b.selected.lock().unwrap().clone();
            let prepared=native::prepare_rebind(state,&b.host,&selected,&prior,capture,visit).await?;
            b.record(json!({"kind":"rebind-prepared","installAttempt":prepared.id(),"priorReceipt":prior.id(),"editor":prior.editor().retained_id()}));
            let locked=prepared.lock().await?;
            let receipt={let s=state.peek();if !b.current(&s){return Err(durable::Error::Stale);}locked.install(&mut b.host.lock().unwrap(),&s)?};
            let value=json!({"kind":"rebound","priorReceipt":prior.id(),"receipt":inventory::installed(&receipt)});
            b.record(value.clone());Ok(value)
        }.await;
        if let Ok(mut panel) = ui.try_write() {
            panel.busy = false;
            panel.adopting = false;
            panel.message = Some(
                if result.is_ok() {
                    "Editing resumed. Your draft and selection were kept."
                } else {
                    "Editing could not resume. Your retained input is kept."
                }
                .into(),
            );
        }
        b.result(
            result.unwrap_or_else(|error| json!({"kind":"resume-refused","error":err(error)})),
        );
    }));
    WAITER.with_borrow_mut(|old| *old = Some(task));
}
