//! Restricted-send channel recovery joins the original raw permission read to
//! its exact accepted canonical publication. No read is recaptured after await.
use super::*;
use std::sync::Mutex;
use super_platinum_core::state::posting_metadata::authority::{
    Authority, ChannelRead, Publication, RawChannel,
};

enum Status {
    Pending,
    Retired,
    Applied(RawChannel),
}
type Completion = Arc<Mutex<Status>>;
#[cfg(test)]
mod tests;
pub(in crate::app_home::channel_fetches) struct Pending {
    owner: Owner,
    observed: Observation,
    read: ChannelRead,
    raw: Value,
    completion: Completion,
}
pub(crate) struct PostingRead {
    direct: direct::Read,
    posting: ChannelRead,
}
impl PostingRead {
    pub(crate) fn capture(
        mut state: Signal<ShellState>,
        team: &str,
        id: &str,
        current: &impl Fn(&ShellState) -> bool,
    ) -> Result<Self, Error> {
        let direct = direct::Read::capture(state, team, id, current)?;
        let posting = {
            let mut s = state.write();
            if !current(&s) || !direct.current(&s) {
                return Err(unavailable());
            }
            Authority::capture(&s.core, team)
                .and_then(|a| a.begin_channel_read(&mut s.core, id))
                .map_err(|_| unavailable())?
        };
        Ok(Self { direct, posting })
    }
    pub(crate) fn current(&self, s: &ShellState) -> bool {
        self.direct.current(s) && self.posting.current(&s.core)
    }
    pub(crate) async fn publish(
        self,
        mut state: Signal<ShellState>,
        raw: Value,
        current: impl Fn(&ShellState) -> bool,
    ) -> Result<Arc<canonical::Canonical>, Error> {
        let completion = Arc::new(Mutex::new(Status::Pending));
        let read = self.direct.0;
        let owner = &read.receipt.owner;
        {
            let mut s = state.write();
            if !current(&s) || !read.current(&s) || !self.posting.current(&s.core) {
                return Err(unavailable());
            }
            let typed = read
                .receipt
                .accept(&mut s, std::slice::from_ref(&raw), &current)?;
            // Capture the EXACT post-enqueue queue. A later accepted row must
            // not inherit this response's posting facts, even before drain.
            let observed = observe(&s, &owner.context)?;
            let pending = &mut s.app_home.channel_fetches.posting;
            // Bounded retained work; superseded account/source entries cannot
            // grow this collection or become a future permission grant.
            if pending.len() >= 32 {
                let old = pending.remove(0);
                *old.completion.lock().unwrap() = Status::Retired;
            }
            pending.push(Pending {
                owner: owner.clone(),
                observed,
                read: self.posting,
                raw,
                completion: completion.clone(),
            });
            drop(s);
            read.posting.channels(&state, &typed);
        }
        if let Err(error) = drain::wait(state, owner).await {
            // The already accepted canonical queue remains shared work, but a
            // timed-out pair must not acquire permission authority later.
            *completion.lock().unwrap() = Status::Retired;
            return Err(error);
        }
        let mut s = state.write();
        if !current(&s)
            || !owner.publication_current(&s)
            || !matches!(&*completion.lock().unwrap(),Status::Applied(raw) if raw.current(&s.core))
        {
            return Err(unavailable());
        }
        canonical::capture(&mut s, &owner.context.session.team_id)
    }
}

pub(in crate::app_home::channel_fetches) struct Prepared {
    publication: Publication,
    completion: Completion,
}
/// Called ONLY in drain's final serial sink, before installing its staged native
/// map. No asynchronous work or authority recapture can occur between these two
/// halves of the shared canonical/posting commit.
pub(in crate::app_home::channel_fetches) fn prepare(
    shell: &mut ShellState,
    owner: &Owner,
    native: &BTreeMap<String, Channel>,
) -> Vec<Prepared> {
    let pending = std::mem::take(&mut shell.app_home.channel_fetches.posting);
    let mut out = vec![];
    for p in pending {
        if p.owner.context.key() != owner.context.key() {
            if p.owner.publication_current(shell) {
                shell.app_home.channel_fetches.posting.push(p);
            } else {
                *p.completion.lock().unwrap() = Status::Retired;
            }
            continue;
        }
        let valid = Arc::ptr_eq(&p.owner.token, &owner.token)
            && matches!(*p.completion.lock().unwrap(), Status::Pending)
            && p.owner.publication_current(shell)
            && observed_current(shell, &owner.context, &p.observed)
            && p.read.current(&shell.core);
        let typed = p.raw["id"].as_str().and_then(|id| native.get(id));
        if valid && let Some(typed) = typed {
            if let Ok(publication) = p.read.prepare_publication(&shell.core, p.raw, typed) {
                out.push(Prepared {
                    publication,
                    completion: p.completion,
                });
                continue;
            }
        }
        *p.completion.lock().unwrap() = Status::Retired;
    }
    out
}
pub(in crate::app_home::channel_fetches) fn apply(shell: &mut ShellState, prepared: Vec<Prepared>) {
    for p in prepared {
        *p.completion.lock().unwrap() = match p.publication.apply(&mut shell.core) {
            Ok(raw) => Status::Applied(raw),
            Err(_) => Status::Retired,
        };
    }
}
