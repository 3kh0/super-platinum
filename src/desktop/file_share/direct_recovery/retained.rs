//! Session copies remain reachable even when bounded accepted history is gone.
use super::*;
#[derive(Clone)]
pub(crate) struct RetainedResult {
    context: admission::Context,
    held: Held,
}
impl RetainedResult {
    pub(crate) fn record(&self) -> &Record {
        &self.held.record
    }
}
pub(crate) fn list(shell: &ShellState) -> Vec<RetainedResult> {
    let Some(context) = admission::Context::capture(shell) else {
        return vec![];
    };
    let Ok(custody) = shell.file_share.custody.lock() else {
        return vec![];
    };
    captured(&custody, &context.key)
        .into_iter()
        .map(|held| RetainedResult {
            context: context.clone(),
            held,
        })
        .collect()
}
fn captured(custody: &recovery::Custody, key: &admission::Key) -> Vec<Held> {
    custody
        .direct_results
        .values()
        .filter(|held| &held.storage.key == key && held.raw.is_some())
        .cloned()
        .collect()
}
fn remove_exact(custody: &mut recovery::Custody, expected: &Held) -> bool {
    let id = expected.record.id();
    if !custody
        .direct_results
        .get(id)
        .is_some_and(|current| exact(current, expected))
    {
        return false;
    }
    custody.direct_results.remove(id);
    true
}
fn exact(current: &Held, expected: &Held) -> bool {
    Arc::ptr_eq(&current.identity, &expected.identity)
        && current.storage.key == expected.storage.key
        && current.record == expected.record
        && current.raw == expected.raw
        && current.raw.is_some()
}
pub(crate) fn discard(mut state: Signal<ShellState>, serial: u64, expected: RetainedResult) {
    let mut shell = state.write();
    if !expected.context.current(&shell)
        || !recovery_visible(&shell).is_some_and(|r| r.serial == serial)
    {
        return;
    }
    let custody = shell.file_share.custody.clone();
    let Ok(mut held) = custody.lock() else {
        return;
    };
    // An explicit reviewed discard removes only this captured session copy.
    // It never deletes an uncertain durable record or grants any network work.
    if !remove_exact(&mut held, &expected.held) {
        return;
    }
    drop(held);
    shell.show_toast(
        "This session's retained DM result was discarded. Saved messages and Slack are unchanged.",
    );
}

#[cfg(test)]
mod tests;
