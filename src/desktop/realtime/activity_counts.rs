//! Complete Activity badge snapshots and guarded HTTP reconciliation.
//! Category totals are Activity compatibility counts, never a Dock formula.
use std::sync::Arc;
use super_platinum_core::{
    CoreAppState,
    config::WorkspaceSession,
    slack::{
        Transport,
        models::{ActivityBadgeCounts, CountsPage},
    },
};

mod reads;
mod rules;
pub(crate) use reads::{mark_channel_read, mark_item_read};

#[derive(Clone)]
pub(crate) struct CountsOwner {
    account: Option<String>,
    epoch: u64,
    transport: Arc<Transport>,
    session: WorkspaceSession,
    cookie: String,
}

impl CountsOwner {
    pub(crate) fn capture(
        core: &CoreAppState,
        transport: &Arc<Transport>,
        session: &WorkspaceSession,
    ) -> Option<Self> {
        let owner = Self {
            account: core.active_account.clone(),
            epoch: core.account_epoch,
            transport: transport.clone(),
            session: session.clone(),
            cookie: core.session.as_ref()?.d_cookie.clone(),
        };
        owner.current(core).then_some(owner)
    }
    fn current(&self, core: &CoreAppState) -> bool {
        core.active_account == self.account
            && core.account_epoch == self.epoch
            && core
                .transport
                .as_ref()
                .is_some_and(|t| Arc::ptr_eq(t, &self.transport))
            && core.session.as_ref().is_some_and(|s| {
                s.d_cookie == self.cookie
                    && s.workspaces.get(&self.session.team_id).is_some_and(|w| {
                        w.team_id == self.session.team_id
                            && w.user_id == self.session.user_id
                            && w.token == self.session.token
                            && w.url == self.session.url
                    })
            })
            && core
                .workspaces
                .get(&self.session.team_id)
                .is_some_and(|w| w.self_user_id == self.session.user_id)
    }
    pub(crate) fn begin(self, core: &CoreAppState) -> Option<CountsRead> {
        if !self.current(core) {
            return None;
        }
        let w = core.workspaces.get(&self.session.team_id)?;
        Some(CountsRead {
            membership: w.channel_lifecycle.revision(),
            threads: w.thread_counts.revision(),
            activity: w.activity_counts.revision(),
            dock: w.dock_counts.revision(),
            owner: self,
        })
    }
}

pub(crate) struct CountsRead {
    owner: CountsOwner,
    membership: u64,
    threads: u64,
    activity: u64,
    dock: u64,
}
impl CountsRead {
    pub(crate) fn apply(self, core: &mut CoreAppState, counts: CountsPage) -> bool {
        if !self.owner.current(core) {
            return false;
        }
        let team = &self.owner.session.team_id;
        let workspace = core.workspaces.get_mut(team).unwrap();
        let zero = workspace.activity_counts.revision() == self.activity
            && counts.activity_v2.as_ref().is_some_and(|c| c.total() == 0);
        workspace.apply_counts_snapshot_with_dock(
            counts,
            self.membership,
            self.threads,
            self.activity,
            self.dock,
        );
        if zero {
            reconcile_zero(core, team);
        }
        true
    }
}

pub(super) fn apply_live(core: &mut CoreAppState, team: &str, counts: ActivityBadgeCounts) {
    let zero = counts.total() == 0;
    core.workspaces
        .get_mut(team)
        .unwrap()
        .apply_activity_counts(counts);
    if zero {
        reconcile_zero(core, team);
    }
}
fn reconcile_zero(core: &mut CoreAppState, team: &str) {
    // A background workspace must never mark the active workspace's rows read.
    if core.active_team.as_deref() == Some(team) {
        core.activity.ensure_scope(core.account_epoch, team);
        core.activity.confirm_zero_unreads();
    }
}

#[cfg(test)]
mod tests;
