//! Account-scoped managed-draft projection. Recovery records may outlive an
//! empty local composer; they must stay visible without becoming resend intent.
use super::{Account, Fault, View};
use crate::state::ShellState;
use dioxus::prelude::{Signal, WritableExt};
use std::{collections::BTreeMap, sync::Arc};
use super_platinum_core::drafts::{
    DraftBook, DraftKey, LocalDraft,
    store::sync::{LoadMode, Loaded, LocalAlternate, LocalReview, Stored},
    sync::{Snapshot, Workflow},
};

#[derive(Default, Debug)]
pub(crate) struct Session {
    pub account: Option<String>,
    pub epoch: u64,
    pub ready: bool,
    pub loading: bool,
    pub error: Option<&'static str>,
    pub entries: BTreeMap<(String, DraftKey), View>,
    pub projection_epoch: u64,
    pub local_alternates: BTreeMap<(String, DraftKey), Vec<LocalAlternate>>,
}
impl Session {
    pub fn ready_for(&self, account: &str, epoch: u64) -> bool {
        self.ready && self.account.as_deref() == Some(account) && self.epoch == epoch
    }
    pub fn entry(&self, account: &str, key: &DraftKey) -> Option<&View> {
        self.entries.get(&(account.to_owned(), key.clone()))
    }
    pub fn local_review_pending(&self, account: &str, key: &DraftKey) -> bool {
        self.local_alternates
            .get(&(account.to_owned(), key.clone()))
            .is_some_and(|versions| !versions.is_empty())
            || self
                .entry(account, key)
                .is_some_and(|view| view.blocked.is_some() || view.uncommitted.is_some())
    }
    pub fn apply_view(&mut self, account: &str, epoch: u64, key: DraftKey, view: View) -> bool {
        if self.account.as_deref() != Some(account) || self.epoch != epoch {
            return false;
        }
        if view.stored.is_none() {
            return false;
        }
        if view.stored.as_ref().is_some_and(|stored| {
            stored.state.scope().account_id != account || stored.state.scope().key != key
        }) {
            return false;
        }
        if let Some(current) = self
            .entry(account, &key)
            .and_then(|view| view.stored.as_ref())
            && view
                .stored
                .as_ref()
                .is_none_or(|stored| stored.revision < current.revision)
        {
            return false;
        }
        self.entries.insert((account.to_owned(), key), view);
        true
    }
    /// A failed local handoff is recovery metadata, never a new managed owner.
    /// An old writer cannot replace a destination that has since reenrolled.
    pub fn apply_local_recovery(
        &mut self,
        account: &str,
        epoch: u64,
        key: DraftKey,
        local: LocalDraft,
    ) -> bool {
        if self.account.as_deref() != Some(account)
            || self.epoch != epoch
            || key.team.is_empty()
            || key.channel.is_empty()
            || self
                .entry(account, &key)
                .is_some_and(|view| view.stored.is_some())
        {
            return false;
        }
        let view = self
            .entries
            .entry((account.to_owned(), key))
            .or_insert_with(|| View {
                local: LocalDraft::default(),
                stored: None,
                blocked: None,
                uncommitted: None,
            });
        view.uncommitted = Some(local);
        true
    }
    pub fn remove_entry(
        &mut self,
        account: &str,
        epoch: u64,
        key: &DraftKey,
        expected: u64,
    ) -> bool {
        if self.account.as_deref() != Some(account)
            || self.epoch != epoch
            || self
                .entry(account, key)
                .and_then(|view| view.stored.as_ref())
                .is_none_or(|stored| stored.revision != expected)
        {
            return false;
        }
        self.entries.remove(&(account.to_owned(), key.clone()));
        true
    }

    pub fn apply_stored(
        &mut self,
        account: &str,
        epoch: u64,
        key: DraftKey,
        stored: Stored,
        local: LocalDraft,
    ) -> bool {
        self.apply_view(
            account,
            epoch,
            key,
            View {
                local,
                stored: Some(stored),
                blocked: None,
                uncommitted: None,
            },
        )
    }
    /// Retire only the inventory the user actually reviewed. Invalidate reads
    /// started before this commit so a late refresh cannot resurrect its rows.
    pub fn settled(&mut self, review: &LocalReview, epoch: u64, mut chosen: View) {
        let account = &review.scope.account_id;
        let key = &review.scope.key;
        if self.account.as_ref() != Some(account) || self.epoch != epoch {
            return;
        }
        self.projection_epoch = self.projection_epoch.wrapping_add(1);
        self.loading = false;
        if let Some(versions) = self
            .local_alternates
            .get_mut(&(account.clone(), key.clone()))
        {
            versions.retain(|version| {
                !review.alternates.iter().any(|old| {
                    old.writer_id == version.writer_id && old.revision >= version.revision
                })
            });
        }
        if let Some(current) = self.entry(account, key) {
            chosen.blocked = current.blocked.clone().filter(|current| {
                review
                    .blocked
                    .as_ref()
                    .is_none_or(|old| old.token != current.token)
            });
            chosen.uncommitted = current
                .uncommitted
                .clone()
                .filter(|local| !review.contains_local(local));
        }
        if chosen.stored.is_some() {
            self.apply_view(account, epoch, key.clone(), chosen);
        } else if self
            .entry(account, key)
            .is_none_or(|view| view.stored.is_none())
        {
            if chosen.blocked.is_some() || chosen.uncommitted.is_some() {
                self.entries.insert((account.clone(), key.clone()), chosen);
            } else {
                self.entries.remove(&(account.clone(), key.clone()));
            }
        }
    }
    fn begin(&mut self, account: &str, epoch: u64) {
        if self.account.as_deref() != Some(account) || self.epoch != epoch {
            *self = Self::default();
        }
        self.account = Some(account.into());
        self.epoch = epoch;
        self.loading = true;
        self.error = None;
    }
    fn accept_refresh(
        &mut self,
        account: &str,
        epoch: u64,
        projection_epoch: u64,
        loaded: Loaded,
    ) -> bool {
        if self.projection_epoch != projection_epoch {
            return false;
        }
        self.accept(account, epoch, loaded);
        true
    }
    fn accept(&mut self, account: &str, epoch: u64, loaded: Loaded) {
        if self.account.as_deref() != Some(account) || self.epoch != epoch {
            return;
        }
        let Loaded {
            local,
            synced,
            mut blocked_local,
            mut uncommitted_local,
            local_alternates,
        } = loaded;
        self.local_alternates
            .retain(|(owner, key), _| owner != account || local_alternates.contains_key(key));
        for ((owner, key), view) in &mut self.entries {
            if owner == account && view.stored.is_none() && !blocked_local.contains_key(key) {
                view.blocked = None;
            }
        }
        for (key, versions) in local_alternates {
            let current = self
                .local_alternates
                .entry((account.to_owned(), key))
                .or_default();
            current.retain(|existing| {
                versions
                    .iter()
                    .any(|version| version.writer_id == existing.writer_id)
            });
            for version in versions {
                if let Some(existing) = current
                    .iter_mut()
                    .find(|existing| existing.writer_id == version.writer_id)
                {
                    if version.revision > existing.revision {
                        *existing = version;
                    }
                } else {
                    current.push(version);
                }
            }
        }
        let mut locals: BTreeMap<_, _> = local.into_iter().collect();
        for (key, stored) in synced {
            let view = View {
                local: locals.remove(&key).unwrap_or_default(),
                stored: Some(stored),
                blocked: blocked_local.remove(&key),
                uncommitted: uncommitted_local.remove(&key),
            };
            self.apply_view(account, epoch, key, view);
        }
        // A released writer may have preserved an alternate without a managed
        // row. Inventory it after restart without replacing the ordinary draft.
        let recovery_keys: std::collections::BTreeSet<_> = blocked_local
            .keys()
            .chain(uncommitted_local.keys())
            .cloned()
            .collect();
        for key in recovery_keys {
            if self
                .entry(account, &key)
                .is_some_and(|view| view.stored.is_some())
            {
                continue;
            }
            let entry = self
                .entries
                .entry((account.to_owned(), key.clone()))
                .or_insert_with(|| View {
                    local: locals.remove(&key).unwrap_or_default(),
                    stored: None,
                    blocked: None,
                    uncommitted: None,
                });
            if let Some(blocked) = blocked_local.remove(&key) {
                entry.blocked = Some(blocked);
            }
            if entry.uncommitted.is_none() {
                entry.uncommitted = uncommitted_local.remove(&key);
            }
        }
        self.ready = true;
        self.loading = false;
        self.error = None;
    }
    fn fail(&mut self) {
        self.loading = false;
        self.error = Some(
            "Saved draft status could not be loaded. Your local text is still here; refresh before sending or deleting it.",
        );
        // Do not authorize legacy writes when managed ownership is unknown.
        self.ready = false;
    }
}

impl Account {
    pub(crate) async fn load_all(&self) -> Result<Loaded, Fault> {
        // This is the same cancellation-safe once-per-account initialization
        // used by Owner. Never normalize a live operation on account revisits.
        self.initialize().await?;
        self.backend.load(LoadMode::Current).await
    }
}

/// Called before a newly loaded ShellState is exposed to the UI. Local-only
/// drafts from the legacy worker retain its in-memory unsaved-write overlay.
pub(crate) async fn restore(shell: &mut ShellState) {
    if crate::fixture::is_fixture() {
        return;
    }
    let Some(account) = shell.core.active_account.clone() else {
        return;
    };
    let epoch = shell.core.account_epoch;
    shell.managed_drafts.begin(&account, epoch);
    let result = match Account::production(&account) {
        Ok(owner) => owner.load_all().await,
        Err(error) => Err(error),
    };
    match result {
        Ok(loaded) => {
            shell.managed_drafts.accept(&account, epoch, loaded);
            restore_locals(shell, &account);
        }
        Err(_) => shell.managed_drafts.fail(),
    }
}

fn restore_locals(shell: &mut ShellState, account: &str) {
    let mut locals = shell.core.drafts.items.clone();
    for ((owner, key), view) in &shell.managed_drafts.entries {
        if owner != account || view.stored.is_none() {
            continue;
        }
        if view.local.empty() {
            locals.remove(key);
        } else {
            locals.insert(key.clone(), view.local.clone());
        }
    }
    shell.core.drafts = DraftBook::restored(locals.into_iter().collect());
    shell.core.composer = Default::default();
    shell.core.thread_composer = Default::default();
    shell.core.composer_attachments.clear();
    shell.core.thread_composer_attachments.clear();
    shell.sync_composer_drafts();
}

/// Refresh only metadata in an exposed shell. Current typing belongs to the
/// checkpoint queue and must not be replaced by an older disk snapshot.
pub(crate) async fn load(mut state: Signal<ShellState>) {
    if crate::fixture::is_fixture() {
        return;
    }
    let (account, epoch, transport, projection_epoch) = {
        let mut shell = state.write();
        let Some(account) = shell.core.active_account.clone() else {
            return;
        };
        let epoch = shell.core.account_epoch;
        if shell.managed_drafts.loading {
            return;
        }
        shell.managed_drafts.begin(&account, epoch);
        (
            account,
            epoch,
            shell.core.transport.clone(),
            shell.managed_drafts.projection_epoch,
        )
    };
    let result = match Account::production(&account) {
        Ok(owner) => owner.load_all().await,
        Err(error) => Err(error),
    };
    let mut shell = state.write();
    if shell.core.active_account.as_deref() != Some(&account)
        || shell.core.account_epoch != epoch
        || !same_transport(&transport, &shell.core.transport)
        || projection_epoch != shell.managed_drafts.projection_epoch
    {
        return;
    }
    match result {
        Ok(loaded) => {
            shell
                .managed_drafts
                .accept_refresh(&account, epoch, projection_epoch, loaded);
        }
        Err(_) => shell.managed_drafts.fail(),
    }
}

pub(super) fn same_transport<T>(captured: &Option<Arc<T>>, current: &Option<Arc<T>>) -> bool {
    match (captured, current) {
        (Some(captured), Some(current)) => Arc::ptr_eq(captured, current),
        (None, None) => true,
        _ => false,
    }
}

#[derive(Clone)]
pub(crate) struct RecoveryRow {
    pub key: DraftKey,
    pub kind: &'static str,
    pub writer_id: Option<String>,
    pub status: &'static str,
    pub explanation: &'static str,
    pub snapshot: Option<Snapshot>,
    pub local: Option<LocalDraft>,
}

pub(crate) fn status(view: &View) -> &'static str {
    if view.blocked.is_some() {
        return "Another local version needs review";
    }
    if view.uncommitted.is_some() {
        return "Local changes are not saved";
    }
    workflow_status(view)
}

fn workflow_status(view: &View) -> &'static str {
    let Some(stored) = &view.stored else {
        return "On this device";
    };
    let schedule = stored.state.consumption().is_some_and(|intent| {
        matches!(
            intent.kind,
            super_platinum_core::drafts::sync::ConsumeKind::Schedule { .. }
        )
    });
    match stored.state.workflow() {
        Workflow::Uncertain(_) if schedule => "Schedule unconfirmed",
        Workflow::Rejected(_) if schedule => "Schedule rejected",
        Workflow::InFlight(_) if schedule => "Waiting for schedule confirmation",
        Workflow::Idle if schedule => "Saved schedule waiting",
        Workflow::Uncertain(_) if stored.state.consumption().is_some() => "Delivery unconfirmed",
        Workflow::Uncertain(_) => "Draft change unconfirmed",
        Workflow::Rejected(_) => "Not sent",
        Workflow::Conflict { .. } => "Draft versions need review",
        Workflow::InFlight(_) => "Waiting for confirmation",
        Workflow::Idle if stored.state.consumption().is_some() => "Saved delivery waiting",
        Workflow::Idle if stored.state.tombstone() && stored.state.acknowledged().is_some() => {
            "Draft removal waiting"
        }
        Workflow::Idle => "Saved on this device",
    }
}

pub(crate) fn recovery_rows(shell: &ShellState) -> Vec<RecoveryRow> {
    let Some(account) = shell.core.active_account.as_ref() else {
        return vec![];
    };
    if shell.managed_drafts.account.as_ref() != Some(account)
        || shell.managed_drafts.epoch != shell.core.account_epoch
    {
        return vec![];
    }
    let mut rows = Vec::new();
    for ((owner, key), versions) in &shell.managed_drafts.local_alternates {
        if owner != account || Some(&key.team) != shell.core.active_team.as_ref() {
            continue;
        }
        for version in versions {
            rows.push(RecoveryRow { key:key.clone(), kind:"writer-alternate", writer_id:Some(version.writer_id.clone()),
                status:"Another local version was preserved",
                explanation:"This edit could not replace the current draft. Its latest version is saved separately for review.",
                snapshot:None, local:Some(version.draft.clone()) });
        }
    }

    for ((owner, key), view) in &shell.managed_drafts.entries {
        if owner != account || Some(&key.team) != shell.core.active_team.as_ref() {
            continue;
        }
        if let Some(blocked) = &view.blocked {
            rows.push(RecoveryRow {writer_id:None, key: key.clone(), kind: "alternate", status: "Another local version needs review", explanation: "This local edit was preserved separately. It has not overwritten the saved draft.", snapshot: None, local: Some(blocked.draft.clone())});
        }
        if let Some(local) = &view.uncommitted {
            rows.push(RecoveryRow {
                writer_id: None,
                key: key.clone(),
                kind: "unsaved",
                status: "Local changes are not saved",
                explanation: "Keep the app open while these changes are waiting to be saved.",
                snapshot: None,
                local: Some(local.clone()),
            });
        }
        let Some(stored) = &view.stored else {
            continue;
        };
        if let Some(operation) = super::recovery::operation(view)
            .filter(|op| op.kind == super_platinum_core::drafts::sync::OperationKind::Update)
        {
            rows.push(RecoveryRow {writer_id:None,key:key.clone(),kind:"update",status:"Draft update is unconfirmed",explanation:"Check the saved update in Slack. This checks only the existing draft; your current text will be kept and nothing will be written again.",snapshot:operation.snapshot,local:None});
        }
        if super::recovery::clean_local(view)
            && shell
                .managed_drafts
                .local_alternates
                .get(&(account.clone(), key.clone()))
                .is_none_or(Vec::is_empty)
        {
            rows.push(RecoveryRow {writer_id:None, key:key.clone(), kind:"local", status:"Local draft ready to edit",
                explanation:"This local draft has no outstanding delivery or remote draft. Continue editing locally to use the ordinary composer.",
                snapshot:None, local:Some(view.local.clone()) });
        }
        let frozen = stored
            .state
            .consumption()
            .map(|intent| intent.snapshot.clone());
        let detached = frozen.or_else(|| {
            view.local
                .empty()
                .then(|| {
                    stored
                        .state
                        .pending()
                        .and_then(|op| {
                            op.snapshot
                                .clone()
                                .or_else(|| op.base.as_ref().map(|base| base.snapshot.clone()))
                        })
                        .or_else(|| {
                            stored
                                .state
                                .tombstone()
                                .then(|| {
                                    stored
                                        .state
                                        .acknowledged()
                                        .map(|remote| remote.snapshot.clone())
                                })
                                .flatten()
                        })
                })
                .flatten()
        });
        if let Some(snapshot) = detached {
            let schedule = stored.state.consumption().is_some_and(|intent| {
                matches!(
                    intent.kind,
                    super_platinum_core::drafts::sync::ConsumeKind::Schedule { .. }
                )
            });
            let explanation = if schedule && stored.state.rejected_schedule().is_some() {
                "This message was not scheduled. Review the exact saved message to retry the same schedule or choose a new time. Your separate composer draft will be kept."
            } else if schedule && super::recovery::operation(view).is_some() {
                "Check the saved schedule in Slack. This reads only the existing draft; your current text will be kept and nothing will be scheduled again."
            } else if schedule {
                "This saved schedule is separate from your current draft. Its result cannot be checked here yet. Keep this content while reviewing the schedule in Slack."
            } else if super::recovery::rejected(view).is_some() {
                "Slack did not send this message. Return it to a draft to edit it; nothing will be sent automatically."
            } else {
                "This saved content is separate from your current draft. Check the conversation before sending it again."
            };
            rows.push(RecoveryRow {
                writer_id: None,
                key: key.clone(),
                kind: if schedule { "schedule" } else { "delivery" },
                status: workflow_status(view),
                explanation,
                snapshot: Some(snapshot),
                local: None,
            });
        }
        if let Workflow::Conflict {
            remote: Some(remote),
            ..
        } = stored.state.workflow()
        {
            rows.push(RecoveryRow {writer_id:None, key: key.clone(), kind: "remote", status: "Different version in Slack", explanation: "This version has been preserved for review. Your local text has not been replaced.", snapshot: Some(remote.snapshot.clone()), local: None});
        }
    }
    rows
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;

#[path = "session_fixture.rs"]
mod fixture;
pub(crate) use fixture::apply as fixture;

#[cfg(test)]
#[path = "session_schedule_tests.rs"]
mod schedule_tests;
