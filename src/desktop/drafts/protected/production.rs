//! Configured startup uses the existing Root/Host and actual owned worker.
//! No ordinary Client fallback, fabricated Stream, or migration proof from IDs.
use super::*;
use crate::state::ShellState;
use std::sync::{Mutex, OnceLock};

pub(crate) mod baseline;
#[cfg(feature = "protected-native-fixture")]
pub(crate) mod baseline_native;
pub(crate) mod commands;
mod enroll;
mod reclaim;
pub(crate) mod send;
pub(crate) mod socket;
mod uploads;

fn root() -> &'static Mutex<root_factory::Root> {
    static ROOT: OnceLock<Mutex<root_factory::Root>> = OnceLock::new();
    ROOT.get_or_init(|| Mutex::new(root_factory::Root::new()))
}
fn error(_: root_factory::Error) -> durable::Error {
    durable::Error::Unsupported
}

pub(crate) fn account(id: &str) -> Result<Arc<Account>, durable::Error> {
    root()
        .lock()
        .map_err(|_| durable::Error::Storage)?
        .configured_account(id)
        .map_err(error)
}

/// Awaited before ShellState is published. The Root task owns initialization;
/// a canceled startup waiter cannot reopen the store or abandon its result.
pub(crate) async fn restore(shell: &mut ShellState) -> Result<(), durable::Error> {
    let id = shell
        .core
        .active_account
        .clone()
        .ok_or(durable::Error::Stale)?;
    let acquired = root()
        .lock()
        .map_err(|_| durable::Error::Storage)?
        .acquire_configured(&id)
        .map_err(error)?;
    acquired.ready().await;
    let account = acquired.account().map_err(error)?;
    let load = {
        let mut root = root().lock().map_err(|_| durable::Error::Storage)?;
        let selected = root.select(shell, &acquired).map_err(error)?;
        root.begin_load(shell, &selected).map_err(error)?
    };
    load.ready().await;
    let locals = {
        let mut root = root().lock().map_err(|_| durable::Error::Storage)?;
        root.finish_load(shell, &load).map_err(error)?;
        let discovery = root.host().discovery();
        let mut locals = BTreeMap::new();
        for key in discovery.keys(shell).ok_or(durable::Error::Stale)? {
            let view = discovery.entry(shell, key).ok_or(durable::Error::Stale)?;
            // Full raw inventory/alternates/recovery remain in Discovery. This
            // is only the editable presentation, never an invented capability.
            let local = view.uncommitted().unwrap_or_else(|| view.local());
            if !local.empty() {
                locals.insert(key.clone(), local.clone());
            }
        }
        let retained = root
            .host()
            .local_presentations(&account)?
            .into_iter()
            .chain(root.host().managed_presentations(&account)?.into_iter());
        for (key, value) in retained {
            if value.empty() {
                locals.remove(&key);
            } else {
                locals.insert(key, value);
            }
        }
        locals
    };
    shell.core.drafts =
        super_platinum_core::drafts::DraftBook::restored(locals.into_iter().collect());
    shell.core.attachment_seq = shell
        .core
        .drafts
        .items
        .values()
        .flat_map(|draft| &draft.attachments)
        .map(|attachment| attachment.id)
        .max()
        .unwrap_or(0);
    shell.core.composer = Default::default();
    shell.core.thread_composer = Default::default();
    shell.core.composer_attachments.clear();
    shell.core.thread_composer_attachments.clear();
    shell.sync_composer_drafts();
    Ok(())
}

/// The ordinary changed-input callback reaches the same Host lane as startup.
/// Managed/unverified scopes retain input in the opening Slot and refuse remote
/// admission; a local read is never promoted into Stream verification.
pub(crate) fn route(shell: &mut ShellState, changes: BTreeMap<DraftKey, LocalDraft>) {
    let result = (|| {
        let mut root = root().lock().map_err(|_| durable::Error::Storage)?;
        let mut failure = None;
        for (key, local) in changes {
            if let Err(e) = root.submit_current(shell, &key, &local) {
                failure = Some(error(e));
            }
        }
        failure.map_or(Ok(()), Err)
    })();
    if result.is_err() {
        shell.managed_drafts.error =
            Some("Draft input is retained; owned storage or review is still pending.");
    }
}

/// The legacy controller cannot acquire a second writer. Ordinary actions use
/// the owned producer paths; unresolved/recovery actions retain their inventory.
pub(crate) fn legacy_refusal() -> &'static str {
    "This draft is owned by the protected workflow. Its saved state needs review."
}

/// Existing selected Host registry; never manufacture another editor registry.
pub(crate) fn registry(shell: &ShellState) -> Result<Arc<registry::Registry>, durable::Error> {
    root()
        .lock()
        .map_err(|_| durable::Error::Storage)?
        .host()
        .discovery()
        .registry(shell)
}

pub(super) fn session_view(shell: &ShellState, key: &DraftKey) -> Option<View> {
    root()
        .lock()
        .ok()?
        .host()
        .discovery()
        .entry(shell, key)
        .cloned()
}
pub(super) fn publish_session(shell: &ShellState, before: &View, after: View) -> bool {
    root()
        .lock()
        .is_ok_and(|mut root| root.host_mut().publish_current(shell, before, after))
}
