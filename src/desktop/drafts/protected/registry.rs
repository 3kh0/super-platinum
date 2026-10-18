//! Exact renderer ownership registry. Detached writers are not implicitly current.
use super::editor::Editor;
use super::*;
use std::sync::Mutex;

#[derive(Default)]
pub(crate) struct Registry(Mutex<State>);
#[derive(Default)]
struct State {
    account: Option<String>,
    epoch: u64,
    writers: BTreeMap<DraftKey, Arc<Editor>>,
}
impl Registry {
    pub fn select(&self, account: String, epoch: u64) {
        let mut state = self.0.lock().unwrap();
        if state.account.as_ref() != Some(&account) || state.epoch != epoch {
            state.account = Some(account);
            state.epoch = epoch;
            state.writers.clear();
        }
    }
    pub fn get(&self, account: &str, epoch: u64, key: &DraftKey) -> Option<Arc<Editor>> {
        let state = self.0.lock().ok()?;
        (state.account.as_deref() == Some(account) && state.epoch == epoch)
            .then(|| state.writers.get(key).cloned())
            .flatten()
    }
    pub fn current(&self, account: &str, epoch: u64, key: &DraftKey, editor: &Arc<Editor>) -> bool {
        self.get(account, epoch, key)
            .is_some_and(|current| Arc::ptr_eq(&current, editor))
    }
    /// Input preservation only, never renderer installation or dispatch proof.
    /// A replacement registry may have no installed writer yet, while Host still
    /// owns this exact Editor. A competing writer or wrong registry owner refuses.
    pub fn accepts_retained_input(
        &self,
        account: &str,
        epoch: u64,
        key: &DraftKey,
        editor: &Arc<Editor>,
    ) -> bool {
        let Ok(state) = self.0.lock() else {
            return false;
        };
        state.account.as_deref() == Some(account)
            && state.epoch == epoch
            && editor.scope().account_id == account
            && &editor.scope().key == key
            && state
                .writers
                .get(key)
                .is_none_or(|current| Arc::ptr_eq(current, editor))
    }
    pub fn replace(
        &self,
        account: &str,
        epoch: u64,
        key: &DraftKey,
        expected: Option<&Arc<Editor>>,
        next: Arc<Editor>,
    ) -> Result<(), durable::Error> {
        let mut state = self.0.lock().map_err(|_| durable::Error::Storage)?;
        if state.account.as_deref() != Some(account)
            || state.epoch != epoch
            || next.owner.scope().account_id != account
            || &next.owner.scope().key != key
            || !same_writer(state.writers.get(key), expected)
        {
            return Err(durable::Error::Stale);
        }
        state.writers.insert(key.clone(), next);
        Ok(())
    }
    pub fn remove(
        &self,
        account: &str,
        epoch: u64,
        key: &DraftKey,
        expected: &Arc<Editor>,
    ) -> bool {
        let mut state = self.0.lock().unwrap();
        if state.account.as_deref() != Some(account)
            || state.epoch != epoch
            || !same_writer(state.writers.get(key), Some(expected))
        {
            return false;
        }
        state.writers.remove(key);
        true
    }
}
fn same_writer(a: Option<&Arc<Editor>>, b: Option<&Arc<Editor>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        _ => false,
    }
}
