//! Activity member acquisition owner. Connection observations select only the
//! read path; they never make a returned member selectable by themselves.
use super::*;
type Visit = u64;
use crate::connection::activity::{Observation, Path};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use super_platinum_core::block_actions::workflow_management::activity_members::{
    Query as MemberQuery, SearchPath,
};
mod agents;
mod drafts;
pub(crate) mod fixture;
pub(super) mod lookup;
mod matching;
pub(super) mod names;
mod native;
pub(super) mod ranking;
mod read;
mod recovery;
pub(super) mod render;
mod store;
pub(super) use lookup::{start, view};
pub(super) use native::choose;
use read::{Acquired, acquire};
#[cfg(test)]
mod tests;

pub(super) struct Query {
    owner: Owner,
    visit: Visit,
    document: u64,
    generation: u64,
    filters: Filters,
    member: MemberQuery,
    connection: Observation,
    fixture: bool,
    editor: Editor,
    policy: policy::Snapshot,
    draft_failures: std::sync::Mutex<Vec<Error>>,
    recovery_receipts: std::sync::Mutex<Vec<Arc<recovery::Acquisition>>>,
    failure_receipt: std::sync::Mutex<Option<Error>>,
}
impl Query {
    pub(super) fn capture(s: &mut ShellState, visit: Visit) -> Result<Self, Error> {
        if !permitted(s, visit) {
            return Err(unavailable());
        }
        let h = s
            .app_home
            .about
            .management
            .surface
            .as_ref()
            .ok_or_else(unavailable)?;
        if h.read.is_some() {
            return Err(unavailable());
        }
        let owner = h.owner.clone();
        let fixture = h.fixture;
        let policy = h
            .policy
            .clone()
            .filter(|p| p.current(s) && p.can_enter())
            .ok_or_else(unavailable)?;
        let d = document(s, visit)
            .filter(|d| d.activity.active && !d.activity.member_composing)
            .ok_or_else(unavailable)?;
        let member = MemberQuery::capture(&owner.ctx.session, &d.activity.user_search)?;
        let connection = Observation::capture(
            s,
            owner.ctx.epoch,
            &owner.ctx.transport,
            &owner.ctx.session,
            member.text(SearchPath::Remote),
        )
        .ok_or_else(unavailable)?;
        let query = Self {
            owner,
            visit,
            document: d.id,
            generation: next(),
            filters: d.activity.filters.clone(),
            member,
            connection,
            fixture,
            editor: d.editor.clone(),
            policy,
            draft_failures: Default::default(),
            recovery_receipts: Default::default(),
            failure_receipt: Default::default(),
        };
        document_mut(s, visit)
            .ok_or_else(unavailable)?
            .activity
            .member_generation = query.generation;
        Ok(query)
    }
    pub(super) fn current(&self, s: &ShellState) -> bool {
        self.owner.current(s)
            && visible(s, self.visit)
            && s.overlay.is_none()
            && s.app_home
                .about
                .management
                .surface
                .as_ref()
                .and_then(|h| h.policy.as_ref())
                .is_some_and(|p| p.current(s) && p.can_enter() && p.same(&self.policy))
            && self.member.current(&self.owner.ctx.session)
            && self
                .connection
                .current(s, self.member.text(SearchPath::Remote))
            && s.app_home
                .about
                .management
                .surface
                .as_ref()
                .is_some_and(|h| h.read.is_none())
            && document(s, self.visit).is_some_and(|d| {
                d.id == self.document
                    && d.editor.same_revision(&self.editor)
                    && d.activity.active
                    && !d.activity.member_composing
                    && d.activity.member_generation == self.generation
                    && d.activity.filters == self.filters
                    && d.activity.user_search == self.member.raw()
            })
    }
    pub(super) fn generation(&self) -> String {
        self.generation.to_string()
    }
    /// Cooldown stops new dispatch; it does not erase ownership of siblings
    /// that were already sent before another request returned Retry-After.
    pub(super) fn can_dispatch(&self, s: &ShellState) -> bool {
        self.current(s) && !cooling(s, &self.owner.ctx)
    }
    pub(super) fn path(&self) -> SearchPath {
        match self.connection.path() {
            Path::Local => SearchPath::Local,
            Path::Remote => SearchPath::Remote,
        }
    }
    pub(super) fn text(&self) -> &str {
        self.member.text(self.path())
    }
    pub(super) fn member(&self) -> &MemberQuery {
        &self.member
    }
    pub(super) async fn retired(&self, state: Signal<ShellState>) {
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if !self.current(&state.peek()) {
                return;
            }
        }
    }
}

/// Retire even a same-text event: an earlier callback cannot gain authority
/// again after the user edits away and returns to the previous string.
pub(super) fn edit(s: &mut ShellState, visit: Visit, value: String) {
    if !visible(s, visit) || s.overlay.is_some() {
        return;
    }
    if let Some(d) = document_mut(s, visit).filter(|d| d.activity.active) {
        d.activity.user_search = value;
        d.activity.member_generation = next();
        d.activity.member_pending = None;
        d.activity.member_results = None;
        d.activity.member_notice = None;
    }
}
pub(super) fn composition(s: &mut ShellState, visit: Visit, composing: bool) {
    if !visible(s, visit) || s.overlay.is_some() {
        return;
    }
    if let Some(d) = document_mut(s, visit).filter(|d| d.activity.active) {
        d.activity.member_composing = composing;
        d.activity.member_generation = next();
        d.activity.member_pending = None;
        d.activity.member_results = None;
    }
}
fn unavailable() -> Error {
    Error::Api("workflow_activity_member_observation_unavailable".into())
}

fn same_account(a: &Owner, b: &Owner) -> bool {
    a.ctx.epoch == b.ctx.epoch
        && Arc::ptr_eq(&a.ctx.transport, &b.ctx.transport)
        && crate::emoji_preferences::same_session(&a.ctx.session, &b.ctx.session)
}
fn assignment<'a>(raw: &'a Value, key: &str) -> Result<Option<&'a str>, Error> {
    if raw["ok"] != true || raw.get("error").is_some_and(|v| !v.is_null()) {
        return Err(unavailable());
    }
    raw["assignments"]
        .as_object()
        .ok_or_else(unavailable)?
        .get(key)
        .map(|v| v["group"].as_str().ok_or_else(unavailable))
        .transpose()
}
