//! Captured conversations.open publication. Not a known-ID info response.
//! Parent correlates the returned peer/type before calling publish.
use super::*;
use std::sync::Mutex;
use super_platinum_core::state::{
    directory_channels::opened::Companions, directory_members::Observations,
    posting_metadata::authority::Authority,
};

#[cfg(test)]
mod tests;

enum Status {
    Pending,
    Retired,
    Applied,
}
type Completion = Arc<Mutex<Status>>;

struct Members {
    observed: Observations,
    native: Value,
}
impl Members {
    fn capture(s: &ShellState, owner: &Owner) -> Result<Self, Error> {
        let c = &owner.context;
        let w = &s.core.workspaces[&c.session.team_id];
        Ok(Self {
            observed: w
                .directory_members
                .observations(&c.account, &c.session, c.epoch, GENERATION)?,
            native: serde_json::to_value(&w.users).map_err(|_| unavailable())?,
        })
    }
    fn current(&self, s: &ShellState, owner: &Owner) -> bool {
        let c = &owner.context;
        s.core.workspaces.get(&c.session.team_id).is_some_and(|w| {
            self.observed.current(
                &w.directory_members,
                &c.account,
                &c.session,
                c.epoch,
                GENERATION,
            ) && serde_json::to_value(&w.users).ok().as_ref() == Some(&self.native)
        })
    }
}

pub(crate) struct OpenRead {
    read: super::Read,
    members: Members,
    authority: Authority,
}
impl OpenRead {
    pub(crate) fn capture(
        mut state: Signal<ShellState>,
        team: &str,
        current: &impl Fn(&ShellState) -> bool,
    ) -> Result<Self, Error> {
        if !current(&state.peek()) {
            return Err(unavailable());
        }
        canonical::capture(&mut state.write(), team)?;
        let mut read = super::Read::capture(state, team, current)?;
        read.receipt.origin = Origin::OpenedConversation;
        let s = state.peek();
        let members = Members::capture(&s, &read.receipt.owner)?;
        let authority = Authority::capture(&s.core, team).map_err(|_| unavailable())?;
        Ok(Self {
            read,
            members,
            authority,
        })
    }
    pub(crate) fn current(&self, s: &ShellState) -> bool {
        self.read.current(s)
            && self.members.current(s, &self.read.receipt.owner)
            && self.authority.current(&s.core)
    }
    pub(crate) async fn publish(
        self,
        mut state: Signal<ShellState>,
        raw: Value,
        current: impl Fn(&ShellState) -> bool,
    ) -> Result<Arc<canonical::Canonical>, Error> {
        let companions = Companions::capture(&raw).map_err(|_| unavailable())?;
        let completion = Arc::new(Mutex::new(Status::Pending));
        {
            let mut s = state.write();
            if !current(&s) || !self.current(&s) {
                return Err(unavailable());
            }
            let typed = self
                .read
                .receipt
                .accept(&mut s, std::slice::from_ref(&raw), &current)?;
            let observed = observe(&s, &self.read.receipt.owner.context)?;
            let pending = &mut s.app_home.channel_fetches.opened;
            if pending.len() >= 32 {
                let old = pending.remove(0);
                *old.completion.lock().unwrap() = Status::Retired;
            }
            pending.push(Pending {
                owner: self.read.receipt.owner.clone(),
                observed,
                lifecycle: self.read.receipt.lifecycle.clone(),
                transformations: self.read.receipt.transformations.clone(),
                members: self.members,
                authority: self.authority,
                raw,
                companions,
                completion: completion.clone(),
            });
            drop(s);
            self.read.posting.channels(&state, &typed);
        }
        let owner = &self.read.receipt.owner;
        if let Err(e) = drain::wait(state, owner).await {
            *completion.lock().unwrap() = Status::Retired;
            return Err(e);
        }
        let mut s = state.write();
        if !current(&s)
            || !owner.publication_current(&s)
            || !matches!(*completion.lock().unwrap(), Status::Applied)
        {
            return Err(unavailable());
        }
        canonical::capture(&mut s, &owner.context.session.team_id)
    }
}

pub(in crate::app_home::channel_fetches) struct Pending {
    owner: Owner,
    observed: Observation,
    lifecycle: crate::app_home::cfm_lifecycle::Snapshot,
    transformations: Arc<members::Transformations>,
    members: Members,
    authority: Authority,
    raw: Value,
    companions: Companions,
    completion: Completion,
}
pub(in crate::app_home::channel_fetches) fn prepare(
    s: &mut ShellState,
    owner: &Owner,
) -> Vec<Pending> {
    let pending = std::mem::take(&mut s.app_home.channel_fetches.opened);
    let mut ready = vec![];
    for p in pending {
        if p.owner.context.key() != owner.context.key() {
            if p.owner.publication_current(s) {
                s.app_home.channel_fetches.opened.push(p);
            } else {
                *p.completion.lock().unwrap() = Status::Retired;
            }
            continue;
        }
        let valid = Arc::ptr_eq(&p.owner.token, &owner.token)
            && matches!(*p.completion.lock().unwrap(), Status::Pending)
            && p.owner.publication_current(s)
            && p.lifecycle.current(s)
            && p.members.current(s, owner)
            && p.authority.current(&s.core)
            && observed_current(s, &owner.context, &p.observed)
            && Arc::ptr_eq(
                &p.transformations,
                &s.app_home.channel_fetches.slots[&owner.context.key()].transformations,
            );
        if valid {
            ready.push(p);
        } else {
            *p.completion.lock().unwrap() = Status::Retired;
        }
    }
    ready
}
/// Called in the same serial sink as the accepted canonical map. This observes
/// companion flags only; it creates no HTTP read reservation or RawChannel.
pub(in crate::app_home::channel_fetches) fn apply(s: &mut ShellState, pending: Vec<Pending>) {
    for p in pending {
        let config = p.authority.configuration().clone();
        if let Ok(metadata) = p.authority.metadata_mut(&mut s.core) {
            // A missing policy dependency remains an explicit unknown. The
            // source channel/companions are still valid accepted publication.
            let _ = metadata.observe_channel(&config, &p.raw);
            p.companions.apply(
                &mut s
                    .core
                    .workspaces
                    .get_mut(&p.owner.context.session.team_id)
                    .unwrap()
                    .messages,
            );
            *p.completion.lock().unwrap() = Status::Applied;
        } else {
            *p.completion.lock().unwrap() = Status::Retired;
        }
    }
}
