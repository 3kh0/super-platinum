//! Open existing managed inventory without enrollment or remote verification.
//! The Host retains unadmitted input; the existing Editor owns admitted input.
use super::super::editor::Editor;
use super::*;
use std::sync::Mutex;
mod install;
pub(crate) use install::{ManagedInstallRequest, ReadyManagedInstall};
#[cfg(test)]
mod tests;

pub(super) struct Slot {
    pub account: Arc<Account>,
    pub expected: durable::Review,
    pub job: Arc<Job<Editor>>,
    pub pending: Arc<Mutex<Option<LocalDraft>>>,
}
impl Slot {
    pub fn submit(&self, local: &LocalDraft) -> Result<u64, durable::Error> {
        super::super::retention::input_size(local)?;
        let mut pending = self.pending.lock().map_err(|_| durable::Error::Storage)?;
        match self.job.result() {
            Ok(editor) => {
                let result = editor.submit(local);
                *pending = result.is_err().then(|| local.clone());
                result
            }
            Err(error) => {
                *pending = Some(local.clone());
                Err(error)
            }
        }
    }
}
#[derive(Clone)]
pub(crate) struct ManagedOpen {
    id: String,
    wait: Wait<Editor>,
}
impl ManagedOpen {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub async fn ready(&self) {
        self.wait.ready().await;
    }
}

/// Never select an arbitrary writer, nor pass a retained failed opening as an
/// absent writer. Completed jobs and their Editor are one retention entry.
fn existing(account: &Arc<Account>, scope: &Scope) -> Result<Option<Arc<Editor>>, durable::Error> {
    let entries = account
        .editors
        .lock()
        .map_err(|_| durable::Error::Storage)?;
    let mut found = None;
    for entry in entries.entries.values() {
        match &entry.editor {
            Some(editor) if editor.scope() == scope => {
                if found.replace(editor.clone()).is_some() {
                    return Err(durable::Error::ReviewRequired);
                }
            }
            Some(_) => (),
            None => {
                if entry.opening.as_ref().is_none_or(|job| job.scope == *scope) {
                    return Err(durable::Error::ReviewRequired);
                }
            }
        }
    }
    Ok(found)
}
impl Host {
    pub fn begin_managed(
        &mut self,
        shell: &ShellState,
        selected: &Selection,
        expected: durable::Review,
    ) -> Result<ManagedOpen, durable::Error> {
        self.check(shell, selected)?;
        let scope = expected.scope();
        if scope.account_id != selected.0.account.id() {
            return Err(durable::Error::Stale);
        }
        let key = (scope.account_id.clone(), scope.key.clone());
        if let Some(route) = self.routes.get(&key) {
            return match route {
                route::Route::OpeningManaged(id) => {
                    let slot = self.managed.get(id).ok_or(durable::Error::Stale)?;
                    if !Arc::ptr_eq(&slot.account, &selected.0.account)
                        || !slot.expected.same(&expected)
                    {
                        return Err(durable::Error::Stale);
                    }
                    Ok(ManagedOpen {
                        id: id.clone(),
                        wait: slot.job.wait(),
                    })
                }
                _ => Err(durable::Error::ReviewRequired),
            };
        }
        if !matches!(self.discovery.entry(shell,&scope.key).map(View::inventory), Some(Inventory::Managed(current)) if current.same(&expected))
        {
            return Err(durable::Error::Stale);
        }
        if self.managed.len() >= MAX_ADOPTIONS || self.routes.len() >= MAX_LOCAL_LANES {
            return Err(durable::Error::Pending);
        }
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| durable::Error::Storage)?;
        let account = selected.0.account.clone();
        let owner = account.owner(scope.key.clone())?;
        owner.check(&expected)?;
        let retained = existing(&account, scope)?;
        if let Some(editor) = &retained {
            if !matches!(editor.snapshot().view.inventory(), Inventory::Managed(current) if current.same(&expected))
            {
                return Err(durable::Error::Stale);
            }
        }
        // Reserve synchronously, using the same exact-expected opener as
        // Editor::open_expected. No task can wait before its owner is retained.
        let opening = if retained.is_none() {
            Some(
                super::super::editor_open::Request {
                    owner,
                    expected: Some(expected.clone()),
                }
                .begin()
                .map_err(|refused| refused.error)?,
            )
        } else {
            None
        };
        let id = barrier_id();
        let job = Job::new();
        let completion = job.completion();
        let pending = Arc::new(Mutex::new(None::<LocalDraft>));
        let opened = ManagedOpen {
            id: id.clone(),
            wait: job.wait(),
        };
        self.managed.insert(
            id.clone(),
            Slot {
                account,
                expected,
                job,
                pending: pending.clone(),
            },
        );
        self.routes.insert(key, route::Route::OpeningManaged(id));
        runtime.spawn(async move {
            let result = match (retained, opening) {
                (Some(editor), None) => Ok(editor),
                (None, Some(opening)) => {
                    opening.ready().await;
                    opening.result()
                }
                _ => Err(durable::Error::Storage),
            };
            // Same order as submit: pending -> result. Admission transfers
            // custody to Editor; failed admission keeps the exact local value.
            let Ok(mut input) = pending.lock() else {
                completion.finish(Err(durable::Error::Storage));
                return;
            };
            if let (Ok(editor), Some(local)) = (&result, input.as_ref()) {
                if editor.submit(local).is_ok() {
                    *input = None;
                }
            }
            completion.finish(result);
        });
        Ok(opened)
    }
    pub fn managed(&self, opened: &ManagedOpen) -> Result<Arc<Editor>, durable::Error> {
        self.managed
            .get(&opened.id)
            .ok_or(durable::Error::Stale)?
            .job
            .result()
    }
    pub fn retained_managed(&self, account: &Arc<Account>) -> Vec<ManagedOpen> {
        self.managed
            .iter()
            .filter(|(_, s)| Arc::ptr_eq(&s.account, account))
            .map(|(id, s)| ManagedOpen {
                id: id.clone(),
                wait: s.job.wait(),
            })
            .collect()
    }
    /// Presentation only. Never converts retained native text to managed/remote
    /// proof. An opening failure leaves the original raw inventory in its slot.
    pub fn managed_presentations(
        &self,
        account: &Arc<Account>,
    ) -> Result<BTreeMap<DraftKey, LocalDraft>, durable::Error> {
        let mut result = BTreeMap::new();
        for slot in self
            .managed
            .values()
            .filter(|s| Arc::ptr_eq(&s.account, account))
        {
            let pending = slot.pending.lock().map_err(|_| durable::Error::Storage)?;
            let local = pending
                .clone()
                .or_else(|| {
                    slot.job.result().ok().map(|editor| {
                        let snapshot = editor.snapshot();
                        snapshot
                            .pending
                            .unwrap_or_else(|| snapshot.view.local().clone())
                    })
                })
                .unwrap_or_else(|| slot.expected.local().clone());
            result.insert(slot.expected.scope().key.clone(), local);
        }
        // Installed routes outlive replaceable ShellState/Registry owners too.
        // Their queued/failed input remains owned by the same Editor after the
        // opening slot is removed; restore must not substitute older disk text.
        for ((owner, key), route) in &self.routes {
            let route::Route::Managed(receipt) = route else {
                continue;
            };
            if owner != account.id() || !Arc::ptr_eq(&receipt.account, account) {
                continue;
            }
            if receipt.editor.scope().account_id != *owner || receipt.editor.scope().key != *key {
                return Err(durable::Error::Stale);
            }
            let snapshot = receipt.editor.snapshot();
            let local = snapshot
                .pending
                .or_else(|| snapshot.view.uncommitted().cloned())
                .unwrap_or_else(|| snapshot.view.local().clone());
            result.insert(key.clone(), local);
        }
        Ok(result)
    }
}
