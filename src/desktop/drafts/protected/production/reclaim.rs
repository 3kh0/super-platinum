//! Successful observed delivery closes its exact old route, not a key-selected writer.
use super::*;
use crate::view::composer::protected_operations::Root as ActionRoot;
use dioxus::prelude::*;
pub(super) fn complete(
    state: Signal<ShellState>,
    actions: &ActionRoot,
    account: &Arc<Account>,
    key: &DraftKey,
    operation: &operation::Operation,
) -> Result<(), durable::Error> {
    let shell = state.try_peek().map_err(|_| durable::Error::Stale)?;
    let mut root = root().lock().map_err(|_| durable::Error::Storage)?;
    // Actual native/root publication has already reached Finished. The Host
    // transfers its already-issued LocalWriter, without opening another lease.
    root.host_mut()
        .complete_sent(&shell, account, key, operation)?;
    actions.acknowledge(operation)
}
