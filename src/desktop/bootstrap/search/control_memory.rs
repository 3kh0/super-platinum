//! Transient native window control choices, not Slack preference observations.
//! Only rendered user choices populate this memory. Core/history/default/People
//! behavior and legacy transport mappings remain independent and unchanged.
use super::recovery::{Cursor, Identity};
use crate::state::{Overlay, ShellState};
use dioxus::prelude::*;
use std::future::Future;
use super_platinum_core::{
    search::{SearchKind, SearchSort},
    slack::SlackClient,
};

#[derive(Clone)]
enum Owner {
    Auth(Identity),
    Local(LocalOwner),
}
#[derive(Clone)]
struct LocalOwner {
    account: Option<String>,
    epoch: u64,
    team: Option<String>,
    user: Option<String>,
    client: SlackClient,
}
fn user(s: &ShellState) -> Option<String> {
    Some(
        s.core
            .workspaces
            .get(s.core.active_team.as_deref()?)?
            .self_user_id
            .clone(),
    )
}
impl Owner {
    fn capture(s: &ShellState) -> Option<Self> {
        if let Some(identity) = Identity::capture_account(s) {
            let transport = s.core.transport.as_ref()?;
            let session = s.core.session.as_ref()?.workspaces.get(identity.team())?;
            return identity
                .matches_session(transport, session)
                .then_some(Self::Auth(identity));
        }
        // This branch grants UI-data ownership only. It cannot survive login or
        // emit a request: both auth parts must remain absent at deferred entry.
        if s.core.session.is_some() || s.core.transport.is_some() {
            return None;
        }
        Some(Self::Local(LocalOwner {
            account: s.core.active_account.clone(),
            epoch: s.core.account_epoch,
            team: s.core.active_team.clone(),
            user: user(s),
            client: s.core.client.clone(),
        }))
    }
    fn account_current(&self, s: &ShellState) -> bool {
        match self {
            Self::Auth(i) => i.account_current(s),
            Self::Local(i) => {
                s.core.session.is_none()
                    && s.core.transport.is_none()
                    && i.account == s.core.active_account
                    && i.epoch == s.core.account_epoch
                    && i.team == s.core.active_team
                    && i.user == user(s)
                    && i.client.same_configuration(&s.core.client)
            }
        }
    }
}
// Private ephemeral credentials/values: no Debug, serialization, disk or agent
// projection. Keep the ORIGINAL account owner across query/dialog visits.
pub(crate) struct Record {
    owner: Owner,
    messages: Option<SearchSort>,
    files: Option<SearchSort>,
}
impl Record {
    fn get(&self, kind: SearchKind) -> Option<SearchSort> {
        match kind {
            SearchKind::Messages => self.messages,
            SearchKind::Files => self.files,
            SearchKind::People => None,
        }
    }
    fn set(&mut self, kind: SearchKind, sort: SearchSort) {
        match kind {
            SearchKind::Messages => self.messages = Some(sort),
            SearchKind::Files => self.files = Some(sort),
            SearchKind::People => {}
        }
    }
}
#[derive(Clone)]
pub(crate) struct Control {
    visit: Visit,
}
#[derive(Clone)]
pub(super) struct Visit {
    owner: Owner,
    raw: String,
    dialog: u64,
    cursor: Cursor,
    pager_query: String,
    loading: bool,
    failed: bool,
    history: bool,
}
impl Visit {
    fn capture(s: &ShellState, owner: Owner) -> Option<Self> {
        if s.overlay != Some(Overlay::Search) || !owner.account_current(s) {
            return None;
        }
        Some(Self {
            owner,
            raw: s.search_query.clone(),
            dialog: s.search_open_generation,
            cursor: Cursor::capture(s),
            pager_query: s.search_pager.query.clone(),
            loading: s.search_pager.loading,
            failed: s.search_pager.failed,
            history: s.search_history_open,
        })
    }
    fn current(&self, s: &ShellState) -> bool {
        self.owner.account_current(s)
            && s.overlay == Some(Overlay::Search)
            && self.raw == s.search_query
            && self.dialog == s.search_open_generation
            && self.cursor.current(s)
            && self.pager_query == s.search_pager.query
            && self.loading == s.search_pager.loading
            && self.failed == s.search_pager.failed
            && self.history == s.search_history_open
    }
}
impl Control {
    pub(crate) fn capture(s: &ShellState) -> Option<Self> {
        Some(Self {
            visit: Visit::capture(s, Owner::capture(s)?)?,
        })
    }
}
// Called only AFTER the rendered source is admitted. A stale callback must not
// clear the successor owner's choices while trying to retire its own record.
fn retire(s: &mut ShellState) {
    if s.search_control_memory
        .as_ref()
        .is_some_and(|r| !r.owner.account_current(s))
    {
        s.search_control_memory = None;
    }
}
pub(super) fn plan_sort(s: &mut ShellState, source: Control, sort: SearchSort) -> Option<Visit> {
    if !source.visit.current(s)
        || !matches!(
            s.search_pager.kind,
            SearchKind::Messages | SearchKind::Files
        )
        || s.search_pager.sort == sort
    {
        return None;
    }
    retire(s);
    let kind = s.search_pager.kind;
    if s.search_control_memory.is_none() {
        s.search_control_memory = Some(Record {
            owner: source.visit.owner.clone(),
            messages: None,
            files: None,
        });
    }
    s.search_control_memory.as_mut()?.set(kind, sort);
    if !s.search_pager.change_sort(sort) {
        return None;
    }
    s.cancel_search_open();
    s.clear_search_results();
    s.search_history_open = false;
    Visit::capture(s, source.visit.owner)
}
pub(super) fn plan_kind(s: &mut ShellState, source: Control, kind: SearchKind) -> Option<Visit> {
    if !source.visit.current(s) || s.search_pager.kind == kind {
        return None;
    }
    retire(s);
    let remembered = s.search_control_memory.as_ref().and_then(|r| r.get(kind));
    // Preserve the legacy primitive contract, including unknown/default carry.
    // Only this explicit USER transition restores a known target choice.
    if !s.set_search_kind(kind) {
        return None;
    }
    if let Some(sort) = remembered {
        s.search_pager.change_sort(sort);
    }
    Visit::capture(s, source.visit.owner)
}
pub(super) async fn run_with<F, Fut>(state: Signal<ShellState>, visit: Visit, read: F) -> bool
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = ()>,
{
    if !state.try_read().ok().is_some_and(|s| visit.current(&s)) {
        return false;
    }
    // No intervening await: the existing Search admission captures credentials
    // and request state synchronously before its first network await.
    read().await;
    true
}
fn schedule(state: Signal<ShellState>, visit: Visit) {
    dioxus::prelude::spawn(async move {
        run_with(state, visit, move || super::search(state)).await;
    });
}
pub(crate) fn choose_sort(mut state: Signal<ShellState>, source: Control, sort: SearchSort) {
    let visit = state
        .try_write()
        .ok()
        .and_then(|mut s| plan_sort(&mut s, source, sort));
    if let Some(visit) = visit {
        schedule(state, visit);
    }
}
pub(crate) fn choose_kind(mut state: Signal<ShellState>, source: Control, kind: SearchKind) {
    let visit = state
        .try_write()
        .ok()
        .and_then(|mut s| plan_kind(&mut s, source, kind));
    if let Some(visit) = visit {
        schedule(state, visit);
    }
}
#[cfg(test)]
mod tests;
