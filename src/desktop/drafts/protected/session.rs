//! Projection ordering is independent of durable row revision ordering.
use super::*;

#[derive(Default)]
pub(crate) struct Session {
    account: Option<String>,
    epoch: u64,
    generation: u64,
    entries: BTreeMap<DraftKey, View>,
    recovery: Vec<durable::RecoverySummary>,
}
/// Process-local, one-use publication permission. Never a storage capability.
pub(crate) struct LoadTicket {
    account: String,
    epoch: u64,
    generation: u64,
}
impl Session {
    pub fn select(&mut self, account: String, epoch: u64) {
        if self.account.as_ref() != Some(&account) || self.epoch != epoch {
            self.generation = self
                .generation
                .checked_add(1)
                .expect("projection generation exhausted");
            self.account = Some(account);
            self.epoch = epoch;
            self.entries.clear();
            self.recovery.clear();
        }
    }
    pub fn entry(&self, account: &str, epoch: u64, key: &DraftKey) -> Option<&View> {
        (self.account.as_deref() == Some(account) && self.epoch == epoch)
            .then(|| self.entries.get(key))
            .flatten()
    }
    pub fn recovery(&self, account: &str, epoch: u64) -> Option<&[durable::RecoverySummary]> {
        (self.account.as_deref() == Some(account) && self.epoch == epoch)
            .then_some(self.recovery.as_slice())
    }
    pub fn begin_load(&mut self) -> Option<LoadTicket> {
        let account = self.account.clone()?;
        self.generation = self
            .generation
            .checked_add(1)
            .expect("projection generation exhausted");
        Some(LoadTicket {
            account,
            epoch: self.epoch,
            generation: self.generation,
        })
    }
    pub fn accept_load(
        &mut self,
        ticket: LoadTicket,
        loaded: &durable::Loaded,
    ) -> Result<(), durable::Error> {
        if self.account.as_ref() != Some(&ticket.account)
            || self.epoch != ticket.epoch
            || self.generation != ticket.generation
        {
            return Err(durable::Error::Stale);
        }
        let entries = model::views(&ticket.account, loaded)?;
        self.entries = entries;
        self.recovery = loaded.recovery().to_vec();
        self.generation = self
            .generation
            .checked_add(1)
            .expect("projection generation exhausted");
        Ok(())
    }
    /// Exact predecessor admission; equal revisions/bytes never bypass it.
    /// A newly enrolled owner is installed by a fresh whole-inventory load, not
    /// by an older completion which happens to carry a larger revision number.
    pub(super) fn apply(&mut self, account: &str, epoch: u64, before: &View, after: View) -> bool {
        if before.scope() != after.scope()
            || before.scope().account_id != account
            || self.account.as_deref() != Some(account)
            || self.epoch != epoch
            || self
                .entries
                .get(&before.scope().key)
                .is_none_or(|current| !current.same(before))
        {
            return false;
        }
        if let (Some(old), Some(new)) = (before.inventory().row(), after.inventory().row())
            && !old.same_enrollment(new)
        {
            return false;
        }
        self.entries.insert(after.scope().key.clone(), after);
        self.generation = self
            .generation
            .checked_add(1)
            .expect("projection generation exhausted");
        true
    }
}
