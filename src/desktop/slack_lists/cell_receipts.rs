//! Retain potentially sent cell writes independently of the visible account/visit.
//! Active lookups remain per List; inactive receipts keep stable owner identity.
use super::*;

const MAX_PER_OWNER: usize = 8;
const MAX_TOTAL: usize = 32;
pub(super) fn blocks_target(shell: &ShellState, scope: &Scope, list: &str) -> bool {
    let owner = scope.stable_owner();
    shell
        .slack_lists
        .operations
        .values()
        .chain(shell.slack_lists.retained_cells.receipts.values())
        .any(|m| m.attempt.scope.stable_owner() == owner && m.attempt.edit.list_id() == list)
}
#[derive(Clone, Default)]
pub(super) struct Retained {
    // The unique attempt serial prevents any owner/List collision from replacing
    // an unresolved receipt. Restoring an ambiguous List fails closed.
    receipts: BTreeMap<u64, Mutation>,
}

pub(super) fn retire(shell: &mut ShellState) {
    for (_, mut mutation) in std::mem::take(&mut shell.slack_lists.operations) {
        if mutation.phase == Phase::Preparing {
            continue;
        }
        mutation.attempt.scope = mutation.attempt.scope.receipt_only();
        shell
            .slack_lists
            .retained_cells
            .receipts
            .entry(mutation.attempt.serial)
            .or_insert(mutation);
    }
}
pub(super) fn restore(shell: &mut ShellState, scope: &Scope) -> bool {
    let owner = scope.stable_owner();
    if shell
        .slack_lists
        .operations
        .values()
        .any(|m| m.attempt.scope.stable_owner() != owner)
    {
        return false;
    }
    let mut lists = shell
        .slack_lists
        .operations
        .keys()
        .cloned()
        .collect::<std::collections::HashSet<_>>();
    let mut serials = Vec::new();
    for (serial, mutation) in &shell.slack_lists.retained_cells.receipts {
        if mutation.attempt.scope.stable_owner() == owner {
            if !lists.insert(mutation.attempt.edit.list_id().to_owned()) {
                return false;
            }
            serials.push(*serial);
        }
    }
    for serial in serials {
        let mutation = shell
            .slack_lists
            .retained_cells
            .receipts
            .remove(&serial)
            .unwrap();
        shell
            .slack_lists
            .operations
            .insert(mutation.attempt.edit.list_id().into(), mutation);
    }
    true
}
pub(super) fn capacity(shell: &ShellState) -> Result<(), &'static str> {
    let scope = shell
        .slack_lists
        .scope
        .as_ref()
        .ok_or("Open the list again.")?;
    let owner = scope.stable_owner();
    let operations = &shell.slack_lists.operations;
    let retained = &shell.slack_lists.retained_cells.receipts;
    if operations.len() + retained.len() >= MAX_TOTAL {
        return Err(
            "Check earlier cell changes before starting another edit. Unconfirmed changes have been retained.",
        );
    }
    if operations
        .values()
        .chain(retained.values())
        .filter(|m| m.attempt.scope.stable_owner() == owner)
        .count()
        >= MAX_PER_OWNER
    {
        return Err("Check earlier cell changes in this workspace before starting another edit.");
    }
    Ok(())
}
fn same(mutation: &Mutation, attempt: &EditAttempt) -> bool {
    mutation.attempt.serial == attempt.serial
        && mutation.attempt.scope.stable_owner() == attempt.scope.stable_owner()
        && mutation.attempt.edit.list_id() == attempt.edit.list_id()
}
pub(super) fn get<'a>(shell: &'a ShellState, attempt: &EditAttempt) -> Option<&'a Mutation> {
    shell
        .slack_lists
        .operations
        .get(attempt.edit.list_id())
        .filter(|m| same(m, attempt))
        .or_else(|| {
            shell
                .slack_lists
                .retained_cells
                .receipts
                .get(&attempt.serial)
                .filter(|m| same(m, attempt))
        })
}
pub(super) fn settle(shell: &mut ShellState, attempt: &EditAttempt, uncertain: bool) {
    if shell
        .slack_lists
        .operations
        .get(attempt.edit.list_id())
        .is_some_and(|m| same(m, attempt))
    {
        if uncertain {
            let mutation = shell
                .slack_lists
                .operations
                .get_mut(attempt.edit.list_id())
                .unwrap();
            mutation.phase = Phase::Uncertain;
            mutation.attempt.scope = mutation.attempt.scope.receipt_only();
        } else {
            shell.slack_lists.operations.remove(attempt.edit.list_id());
        }
    } else if shell
        .slack_lists
        .retained_cells
        .receipts
        .get(&attempt.serial)
        .is_some_and(|m| same(m, attempt))
    {
        if uncertain {
            shell
                .slack_lists
                .retained_cells
                .receipts
                .get_mut(&attempt.serial)
                .unwrap()
                .phase = Phase::Uncertain;
        } else {
            shell
                .slack_lists
                .retained_cells
                .receipts
                .remove(&attempt.serial);
        }
    }
}
pub(super) fn notice(shell: &ShellState, id: &str) -> Option<String> {
    shell.slack_lists.operations.get(id).map(|mutation| match mutation.phase {
        Phase::Preparing => "Checking the cell before saving…",
        Phase::Saving => "A cell change is still being sent. Wait before checking its result.",
        Phase::Uncertain => "A previous cell change is unconfirmed. Refresh to check its result before editing again.",
    }.into())
}
#[cfg(test)]
mod tests;
