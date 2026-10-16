use super::*;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Loading,
    LoadFailed,
    DraftReview,
    Editing,
    Preparing,
    Sending,
    Outcome,
    StorageFailed,
}
#[derive(Clone, PartialEq)]
pub(crate) struct Destination {
    pub id: String,
    pub name: String,
    pub private: bool,
}
pub(crate) struct Dialog {
    pub id: String,
    pub file_id: String,
    pub file_title: String,
    pub note: Vec<Value>,
    pub destination: Option<Destination>,
    pub phase: Phase,
    pub error: Option<&'static str>,
    pub receipt: Option<Receipt>,
    pub return_focus: String,
    pub editor_ready: bool,
    pub closing: bool,
    pub saved_draft_stale: bool,
    pub slack_url: Option<String>,
    pub(super) admission: Option<admission::Source>,
    pub(super) draft_visit: Option<Arc<drafts::Visit>>,
    pub(super) durable: Option<super_platinum_core::file_sharing::store::drafts::Draft>,
    pub(super) draft_storage: Option<recovery::Storage>,
    pub(super) draft_task: Option<draft_io::Ticket>,
    pub(crate) draft_local: bool,
}
pub(crate) struct CapturedDraft {
    pub id: u64,
    pub file_title: String,
    pub destination_name: Option<String>,
}
#[derive(Default)]
pub(crate) struct Recovery {
    pub open: bool,
    pub loading: bool,
    pub error: Option<&'static str>,
    pub receipts: Vec<Receipt>,
    pub drafts: Vec<super_platinum_core::file_sharing::store::drafts::Draft>,
    pub(super) local: Vec<draft_io::Ticket>,
    pub captured: Vec<CapturedDraft>,
    pub serial: u64,
}
#[derive(Default)]
pub(crate) struct State {
    pub dialog: Option<Dialog>,
    pub recovery: Recovery,
    pub(super) context: Option<admission::Context>,
    pub(super) policies: BTreeMap<String, policy_owner::Owner>,
    // Carried into replacement ShellState. Captured workers also own this Arc.
    pub(super) custody: Arc<std::sync::Mutex<recovery::Custody>>,
}
impl State {
    pub(super) fn sync(&mut self, shell: &ShellState) {
        self.retire_reads();
        if self.context.as_ref().is_some_and(|c| c.current(shell)) {
            return;
        }
        if let Some(dialog) = &self.dialog {
            durable::retire(dialog, &self.custody);
        }
        self.dialog = None;
        self.recovery = Recovery::default();
        self.policies.clear();
        self.context = admission::Context::capture(shell);
    }
    pub(super) fn retire_reads(&mut self) {
        if let Ok(mut held) = self.custody.lock() {
            for (channel, read) in held.canceled_reads.drain(..) {
                if let Some(owner) = self.policies.get_mut(&channel) {
                    owner.failed(&read);
                }
            }
        }
    }
    pub(super) fn begin_policy(
        &mut self,
        shell: &ShellState,
        id: &str,
    ) -> Result<policy_owner::Read, Refusal> {
        self.retire_reads();
        if !self
            .policies
            .get(id)
            .is_some_and(|o| o.same_scope(shell, id))
        {
            // Validate the requested owner and read before removing retained
            // facts. Invalid destinations must not churn an unrelated entry.
            let mut owner = policy_owner::Owner::capture(shell, id).ok_or(Refusal::Destination)?;
            let read = owner.begin(shell).map_err(|_| Refusal::Changed)?;
            if self.policies.len() >= 32 && !self.policies.contains_key(id) {
                let idle = self
                    .policies
                    .iter()
                    .find(|(_, owner)| owner.reclaimable())
                    .map(|(channel, _)| channel.clone())
                    .ok_or(Refusal::PolicyCapacity)?;
                self.policies.remove(&idle);
            }
            self.policies.insert(id.into(), owner);
            return Ok(read);
        }
        self.policies
            .get_mut(id)
            .ok_or(Refusal::Destination)?
            .begin(shell)
            .map_err(|_| Refusal::Changed)
    }
}

/// The real account reload replaces ShellState. Keep only outcome/cooldown
/// custody and captured drafts, never the old dialog or permission grants.
pub(crate) fn carry_pending(previous: &mut ShellState, loaded: &mut ShellState) {
    loaded.file_share.custody = previous.file_share.custody.clone();
}

pub(crate) fn event(shell: &mut ShellState, team: &str, generation: u64, event: &RtEvent) {
    let mut state = std::mem::take(&mut shell.file_share);
    state.sync(shell);
    for owner in state.policies.values_mut() {
        owner.event(shell, team, generation, event);
    }
    shell.file_share = state;
}
pub(crate) fn connection(shell: &mut ShellState, team: &str, generation: u64) {
    if shell
        .core
        .workspaces
        .get(team)
        .is_none_or(|w| generation < w.rt_generation)
    {
        return;
    }
    if shell
        .file_share
        .context
        .as_ref()
        .is_some_and(|c| c.key.team == team)
    {
        for owner in shell.file_share.policies.values_mut() {
            owner.invalidate(true);
        }
    }
}
