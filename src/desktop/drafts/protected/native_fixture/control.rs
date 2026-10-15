//! Explicit failure/interleavings; mutations remain actual public Editor calls.
use super::*;
pub(crate) async fn arm_failure(b: &Binding) -> Result<(), String> {
    if b.scenario != "failed-write-return-channel" {
        return Err("Wrong failure scenario".into());
    }
    account::wait(
        b.isolated
            .fail_next(durable::test_support::FaultPoint::WriterCommit),
    )
    .await
    .map_err(err)
}
pub(crate) async fn rotate_away(
    mut state: Signal<ShellState>,
    b: &Binding,
) -> Result<Value, String> {
    if b.scenario != "failed-write-return-channel" {
        return Err("Wrong account interleave".into());
    }
    {
        let editor = b.editor(b.target())?;
        let snapshot = editor.snapshot();
        if snapshot.running
            || snapshot.error != Some(durable::Error::Storage)
            || snapshot.failed_inputs.len() != 1
        {
            return Err("Failed input was not retained".into());
        }
    }
    let epoch = {
        let mut s = state.write();
        if !b.current(&s) {
            return Err("Protected original owner changed".into());
        }
        s.core.active_account = Some(OTHER_ACCOUNT.into());
        s.core.account_epoch += 1;
        b.registry
            .select(OTHER_ACCOUNT.into(), s.core.account_epoch);
        s.core.account_epoch
    };
    // The writer notifies flush just before its task drops its own final Arc.
    // Observe that real cleanup, without retaining an inspection Arc across an
    // await or weakening the exact Account-only ownership requirement.
    for _ in 0..100 {
        let value = {
            let retained = b.account.retained_editors().map_err(err)?;
            let target = retained
                .iter()
                .find(|e| e.retained_id() == b.writer_ids[&key(b.target())])
                .ok_or("Account did not retain failed writer")?;
            let strong = Arc::strong_count(target);
            (strong == 2).then(|| json!({"kind":"away","account":OTHER_ACCOUNT,"epoch":epoch,"retentionId":target.retained_id(),"strongCount":strong,"failedInputs":target.snapshot().failed_inputs,"pending":target.snapshot().pending}))
        };
        let current = {
            let s = state.peek();
            b.isolated_shell(&s)
                && s.core.active_account.as_deref() == Some(OTHER_ACCOUNT)
                && s.core.account_epoch == epoch
        };
        if !current {
            return Err("Away owner changed while observing writer retirement".into());
        }
        if let Some(value) = value {
            b.actions.lock().unwrap().push(value.clone());
            return Ok(value);
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    Err("Failed writer task did not relinquish its external owner".into())
}
pub(crate) fn return_owner(mut state: Signal<ShellState>, b: &Binding) -> Result<Value, String> {
    let mut s = state.write();
    if b.scenario != "failed-write-return-channel"
        || !b.isolated_shell(&s)
        || s.core.active_account.as_deref() != Some(OTHER_ACCOUNT)
    {
        return Err("Wrong return-owner state".into());
    }
    s.core.active_account = Some(ACCOUNT.into());
    s.core.account_epoch += 1;
    let epoch = s.core.account_epoch;
    b.epoch.store(epoch, Ordering::Release);
    b.registry.select(ACCOUNT.into(), epoch);
    let retained = b.account.retained_editors().map_err(err)?;
    for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
        let editor = retained
            .iter()
            .find(|e| e.retained_id() == b.writer_ids[&key(target)])
            .ok_or("Retained writer missing on return")?;
        b.registry
            .replace(ACCOUNT, epoch, &key(target), None, editor.clone())
            .map_err(err)?;
    }
    let value = json!({"kind":"returned","account":s.core.active_account,"epoch":epoch,"retentionId":b.editor(b.target())?.retained_id()});
    b.actions.lock().unwrap().push(value.clone());
    Ok(value)
}
pub(crate) async fn retry_input(b: &Binding) -> Result<Value, String> {
    if b.scenario != "failed-write-return-channel" {
        return Err("Wrong retry case".into());
    }
    let editor = b.editor(b.target())?;
    let before = editor.snapshot();
    editor.retry_storage().map_err(err)?;
    let after = editor.flush().await.map_err(err)?;
    let value = json!({"kind":"input-retried","retentionId":editor.retained_id(),"serialBefore":before.serial,"serialAfter":after.serial,"review":after.view.inventory().managed().ok().map(row_json),"pending":after.pending,"failedInputs":after.failed_inputs});
    b.actions.lock().unwrap().push(value.clone());
    Ok(value)
}
