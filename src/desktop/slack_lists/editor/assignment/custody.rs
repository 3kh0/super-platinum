//! Explicit local retirement never forgets a possibly sent permission grant.
use super::*;

const LIMIT: usize = 256;

pub(super) fn can_reserve(selected: usize) -> bool {
    registry(|r| reservation_fits(r, selected))
}
fn reservation_fits(r: &Registry, selected: usize) -> bool {
    r.entries
        .values()
        .fold(r.shares.len(), |n, e| n.saturating_add(e.selected.len()))
        .saturating_add(selected)
        <= LIMIT
}
pub(super) fn insert(key: Key, e: Entry) -> Result<(), &'static str> {
    registry(|r| {
        if r.entries.contains_key(&key)
            || r.entries.len() >= 32
            || !reservation_fits(r, e.selected.len())
        {
            return Err("This assignment exceeds the retained sharing receipt limit.");
        }
        r.entries.insert(key, e);
        Ok(())
    })
}
fn retained(person: &Recipient) -> bool {
    person.dm_attempted || person.result != ShareResult::NotAttempted
}
fn archive_fits(r: &Registry, e: &Entry) -> bool {
    r.shares
        .len()
        .saturating_add(e.recipients.iter().filter(|p| retained(p)).count())
        <= LIMIT
}
pub(super) fn can_archive(key: &Key, e: &Entry) -> Result<(), &'static str> {
    registry(|r| {
        if !r
            .entries
            .get(key)
            .is_some_and(|current| current.serial == e.serial)
            || !archive_fits(r, e)
        {
            return Err("The assignment receipt cannot be retired safely.");
        }
        Ok(())
    })
}
pub(super) fn archive(key: &Key, e: &Entry) -> Result<(), &'static str> {
    registry(|r| {
        if !r
            .entries
            .get(key)
            .is_some_and(|current| current.serial == e.serial)
            || !archive_fits(r, e)
        {
            return Err("The assignment receipt cannot be retired safely.");
        }
        let missing = e.intent.sharing_needed().ok();
        for person in e.recipients.iter().filter(|p| retained(p)) {
            r.shares.push(ShareReceipt {
                key: key.clone(),
                serial: e.serial,
                person: person.clone(),
                observed: missing
                    .as_ref()
                    .is_some_and(|ids| !ids.contains(&person.id)),
            });
        }
        r.entries.remove(key);
        Ok(())
    })
}

/// Content may be stale or the original row absent. The local retirement action
/// is instead owned by the exact current account/scope, List visit and draft.
/// It cannot finish while a worker still owns an in-flight response.
#[derive(Clone)]
pub(crate) struct Recovery {
    key: Key,
    identity: Arc<()>,
    visit: u64,
    draft: Option<u64>,
    pub serial: u64,
    pub can_abandon: bool,
    pub in_editor: bool,
}
pub(crate) fn recovery(shell: &ShellState) -> Option<Recovery> {
    let key = Key::current(shell)?;
    let e = entry(shell)?;
    let view = shell.slack_lists.view.as_ref()?;
    Some(Recovery {
        key: key.clone(),
        identity: shell.slack_lists.scope.as_ref()?.identity.clone(),
        visit: view.generation,
        draft: view.editor.as_ref().map(|d| d.generation),
        serial: e.serial,
        can_abandon: e.job.is_none() && can_archive(&key, &e).is_ok(),
        in_editor: view
            .editor
            .as_ref()
            .is_some_and(|d| d.row_id == e.row && d.column_id == e.column),
    })
}
pub(crate) fn abandon(mut state: Signal<ShellState>, recovery: Recovery) {
    abandon_shell(&mut state.write(), &recovery);
}
pub(super) fn abandon_shell(shell: &mut ShellState, recovery: &Recovery) -> bool {
    if Key::current(shell).as_ref() != Some(&recovery.key)
        || !shell
            .slack_lists
            .scope
            .as_ref()
            .is_some_and(|s| Arc::ptr_eq(&s.identity, &recovery.identity))
        || !shell.slack_lists.view.as_ref().is_some_and(|v| {
            v.generation == recovery.visit
                && v.editor.as_ref().map(|d| d.generation) == recovery.draft
        })
    {
        return false;
    }
    let Some(e) = entry(shell).filter(|e| e.serial == recovery.serial && e.job.is_none()) else {
        return false;
    };
    if archive(&recovery.key, &e).is_err() {
        return false;
    }
    let view = shell.slack_lists.view.as_mut().unwrap();
    if view
        .editor
        .as_ref()
        .is_some_and(|d| d.row_id == e.row && d.column_id == e.column)
    {
        view.editor = None;
    }
    view.error = Some("Assignment abandoned. Any earlier sharing result is retained; no assignment was sent. Refresh the List before starting again.".into());
    true
}

#[cfg(test)]
mod tests;
