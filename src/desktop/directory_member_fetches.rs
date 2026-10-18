//! One ephemeral owner for unknown-member read claims across Home, Activity,
//! Settings and Profile. Drivers keep one owned wake/task, never one per query.
//! This module owns no second member store and performs no IO or renderer work.
use crate::state::ShellState;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
    time::Instant,
};
use super_platinum_core::{
    block_actions::directory_endpoint::member_info::{
        Page,
        unknown::{Capture, NetworkTerminal},
    },
    config::WorkspaceSession,
    slack::{Error, SlackClient, Transport},
    state::directory_members::fetch_claim as claim,
};
pub(crate) mod automatic;
mod context;
pub(crate) mod driver;
pub(crate) mod known;
pub(crate) use context::Context;
pub(crate) use driver::{Client, Election, Stage};
pub(crate) use super_platinum_core::state::directory_members::fetch_claim::{
    ATTEMPT_TIMEOUT_MS, MAX_ATTEMPTS, Status, Timeout,
};
#[cfg(test)]
mod tests;
const GENERATION: &str = "shared-unknown-member-acquisition";
fn unavailable() -> Error {
    Error::Api("directory_member_fetch_unavailable".into())
}
type Key = (String, String, String);
struct Slot {
    context: Arc<Context>,
    manager: claim::Manager<Outcome>,
    changed: tokio::sync::watch::Sender<u64>,
    retry_at: Option<Option<Instant>>,
    last_failure: Option<Arc<Error>>,
}
impl Slot {
    fn notify(&self) {
        self.changed
            .send_modify(|value| *value = value.wrapping_add(1));
    }
}
pub(crate) struct Registry {
    automatic: automatic::Registry,
    clock: Instant,
    slots: BTreeMap<Key, Slot>,
    // Passive task accounting for closed fixture settlement. Never admission.
    activity: Arc<driver::Activity>,
}
impl Default for Registry {
    fn default() -> Self {
        Self {
            automatic: Default::default(),
            clock: Instant::now(),
            slots: BTreeMap::new(),
            activity: Arc::default(),
        }
    }
}
impl Registry {
    fn millis(&self, now: Instant) -> Result<u64, Error> {
        u64::try_from(
            now.checked_duration_since(self.clock)
                .ok_or_else(unavailable)?
                .as_millis(),
        )
        .map_err(|_| unavailable())
    }
    fn slot(&self, context: &Arc<Context>) -> Result<&Slot, Error> {
        self.slots
            .get(&context.key())
            .filter(|s| Arc::ptr_eq(&s.context, context))
            .ok_or_else(unavailable)
    }
    fn slot_mut(&mut self, context: &Arc<Context>) -> Result<&mut Slot, Error> {
        self.slots
            .get_mut(&context.key())
            .filter(|s| Arc::ptr_eq(&s.context, context))
            .ok_or_else(unavailable)
    }
}
pub(crate) struct Subscription {
    context: Arc<Context>,
    token: claim::Subscription,
    changed: tokio::sync::watch::Receiver<u64>,
}
impl Subscription {
    /// Event-driven waiting: no per-consumer clock or polling timer. Check
    /// status/current Query, await this, then recheck; closed means retirement.
    pub(crate) async fn changed(&mut self) -> Result<(), Error> {
        self.changed.changed().await.map_err(|_| unavailable())
    }
}
pub(crate) struct Wake {
    context: Arc<Context>,
    token: claim::Wake,
    due: Instant,
}
impl Wake {
    pub(crate) fn due(&self) -> Instant {
        self.due
    }
}
pub(crate) struct Dispatch {
    context: Arc<Context>,
    token: claim::Dispatch,
    capture: Capture,
    native_before: BTreeMap<String, Value>,
}
impl Dispatch {
    pub(crate) fn context(&self) -> &Context {
        &self.context
    }
    pub(crate) fn capture(&self) -> &Capture {
        &self.capture
    }
    /// Absence is known only for this Capture's source-absent IDs. Preserve
    /// existing observations and do not manufacture None for other members.
    pub(crate) fn interaction_before(&self) -> BTreeMap<String, Option<bool>> {
        let mut prior = self.capture.observations().interactions().clone();
        for id in self.capture.ids() {
            prior.entry(id.clone()).or_insert(None);
        }
        prior
    }
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        self.observations_current(shell)
            && shell
                .directory_member_fetches
                .slot(&self.context)
                .is_ok_and(|s| s.manager.dispatch_current(&self.token))
    }
    fn observations_current(&self, shell: &ShellState) -> bool {
        self.context.current(shell)
            && shell
                .core
                .workspaces
                .get(&self.context.session.team_id)
                .is_some_and(|w| {
                    self.capture.current(
                        &w.directory_members,
                        &self.context.account,
                        &self.context.session,
                        self.context.epoch,
                        GENERATION,
                    ) && context::native_snapshot(&w.users).is_ok_and(|n| n == self.native_before)
                })
    }
}
/// Original ordered pages/errors are retained once per batch, rather than in
/// each query. A cooldown subset retains the original ordered page evidence
/// but permits terminal projection only for its completed IDs.
pub(crate) struct Outcome {
    dispatch: Arc<Dispatch>,
    terminal: NetworkTerminal,
    pub(crate) pages: Vec<Arc<Page>>,
    pub(crate) errors: Vec<Arc<Error>>,
}
impl Outcome {
    pub(crate) fn capture(&self) -> &Capture {
        &self.dispatch.capture
    }
    pub(crate) fn terminal(&self) -> NetworkTerminal {
        match &self.terminal {
            NetworkTerminal::Complete(rows) => NetworkTerminal::Complete(rows.clone()),
            NetworkTerminal::Failed => NetworkTerminal::Failed,
        }
    }
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        self.dispatch.observations_current(shell)
            && shell
                .directory_member_fetches
                .slot(&self.dispatch.context)
                .is_ok_and(|s| s.manager.receipt_current(&self.dispatch.token))
    }
    /// Exact terminal rows for the elected adopter only. Reconstructing the
    /// private Capture under the same live source witness avoids lending one
    /// consumer's query-specific Capture to another consumer.
    pub(crate) fn direct_rows(
        &self,
        shell: &ShellState,
        adoption: &Adoption,
        asset:&super_platinum_core::block_actions::directory_endpoint::member_info::unknown::PlaceholderAsset,
    ) -> Result<Vec<Value>, Error> {
        if !self.current(shell)
            || !adoption.current(shell)
            || !Arc::ptr_eq(&adoption.context, &self.dispatch.context)
            || adoption.token.ids() != self.dispatch.token.ids()
        {
            return Err(unavailable());
        }
        let c = &self.dispatch.context;
        let w = shell
            .core
            .workspaces
            .get(&c.session.team_id)
            .ok_or_else(unavailable)?;
        let before = w
            .directory_members
            .observations(&c.account, &c.session, c.epoch, GENERATION)?;
        Capture::network_only(before, self.dispatch.token.ids())?.complete(
            &w.directory_members,
            &c.account,
            &c.session,
            c.epoch,
            GENERATION,
            self.terminal(),
            asset,
        )
    }
}
pub(crate) struct Adoption {
    context: Arc<Context>,
    token: claim::Adoption,
}
impl Adoption {
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        self.context.current(shell)
            && shell
                .directory_member_fetches
                .slot(&self.context)
                .is_ok_and(|s| s.manager.adoption_current(&self.token))
    }
}

/// Register actual source-absent IDs. None means no unknown source IDs, not
/// native/source Ready; callers still perform known-row recovery and eligibility.
pub(crate) fn register(
    shell: &mut ShellState,
    team: &str,
    requested: &[String],
) -> Result<Option<Subscription>, Error> {
    let context = Arc::new(Context::capture(shell, team)?);
    let w = shell.core.workspaces.get(team).ok_or_else(unavailable)?;
    let prior = w.directory_members.observations(
        &context.account,
        &context.session,
        context.epoch,
        GENERATION,
    )?;
    let capture = Capture::network_only(prior, requested)?;
    if capture.ids().is_empty() {
        return Ok(None);
    }
    // Only current contexts remain addressable. Retired futures retain no new
    // authority and can no longer publish into a replacement slot.
    let stale: Vec<_> = shell
        .directory_member_fetches
        .slots
        .iter()
        .filter(|(_, s)| !s.context.current(shell))
        .map(|(k, _)| k.clone())
        .collect();
    for key in stale {
        shell.directory_member_fetches.slots.remove(&key);
    }
    // A canceled consumer can leave a useful received batch for its successor.
    // If source/native state changed since that batch, retire its old claims
    // before registering this newly captured absent set. Otherwise refusing
    // the old receipt during adoption would retire this new subscriber too.
    let retained = shell
        .directory_member_fetches
        .slots
        .get(&context.key())
        .and_then(|slot| slot.manager.retained_receipt());
    if let Some(receipt) = retained.filter(|receipt| !receipt.current(shell)) {
        let slot = shell
            .directory_member_fetches
            .slot_mut(&receipt.dispatch.context)?;
        slot.manager.refuse_receipt(&receipt.dispatch.token)?;
        slot.notify();
    }
    let slot = shell
        .directory_member_fetches
        .slots
        .entry(context.key())
        .or_insert_with(|| Slot {
            context,
            manager: Default::default(),
            changed: tokio::sync::watch::channel(0).0,
            retry_at: None,
            last_failure: None,
        });
    let token = slot.manager.register(capture.ids())?;
    slot.notify();
    Ok(Some(Subscription {
        context: slot.context.clone(),
        token,
        changed: slot.changed.subscribe(),
    }))
}
pub(crate) fn status(shell: &ShellState, sub: &Subscription) -> claim::Status {
    if !sub.context.current(shell) {
        return claim::Status::Retired;
    }
    shell
        .directory_member_fetches
        .slot(&sub.context)
        .map(|s| s.manager.status(&sub.token))
        .unwrap_or(claim::Status::Retired)
}
pub(crate) fn arm(
    shell: &mut ShellState,
    sub: &Subscription,
    now: Instant,
) -> Result<Option<Wake>, Error> {
    if status(shell, sub) == claim::Status::Retired {
        return Err(unavailable());
    }
    let millis = shell.directory_member_fetches.millis(now)?;
    let origin = shell.directory_member_fetches.clock;
    let slot = shell.directory_member_fetches.slot_mut(&sub.context)?;
    slot.manager
        .arm(millis)?
        .map(|token| {
            let due = origin
                .checked_add(std::time::Duration::from_millis(token.due_ms()))
                .ok_or_else(unavailable)?;
            Ok(Wake {
                context: sub.context.clone(),
                token,
                due,
            })
        })
        .transpose()
}
pub(crate) fn dispatch(
    shell: &mut ShellState,
    wake: &Wake,
    now: Instant,
) -> Result<Arc<Dispatch>, Error> {
    if !wake.context.current(shell) {
        return Err(unavailable());
    }
    let captured = (|| {
        let w = shell
            .core
            .workspaces
            .get(&wake.context.session.team_id)
            .ok_or_else(unavailable)?;
        let prior = w.directory_members.observations(
            &wake.context.account,
            &wake.context.session,
            wake.context.epoch,
            GENERATION,
        )?;
        Ok::<_, Error>((prior, context::native_snapshot(&w.users)?))
    })();
    let (prior, native_before) = match captured {
        Ok(value) => value,
        Err(error) => {
            cancel_wake(shell, wake)?;
            return Err(error);
        }
    };
    let millis = shell.directory_member_fetches.millis(now)?;
    let slot = shell.directory_member_fetches.slot_mut(&wake.context)?;
    let token = slot.manager.dispatch(&wake.token, millis)?;
    let capture = match Capture::network_only(prior, token.ids()) {
        Ok(value) => value,
        Err(error) => {
            slot.manager.refuse(&token)?;
            slot.notify();
            return Err(error);
        }
    };
    Ok(Arc::new(Dispatch {
        context: wake.context.clone(),
        token,
        capture,
        native_before,
    }))
}
/// Release an owned wake on local admission/cancellation. This records neither
/// a dispatch timestamp nor a terminal result; original pending IDs remain.
pub(crate) fn cancel_wake(shell: &mut ShellState, wake: &Wake) -> Result<(), Error> {
    let slot = shell.directory_member_fetches.slot_mut(&wake.context)?;
    slot.manager.cancel_wake(&wake.token)?;
    slot.notify();
    Ok(())
}
pub(crate) fn timeout(
    shell: &mut ShellState,
    dispatch: &Dispatch,
) -> Result<claim::Timeout, Error> {
    if !dispatch.current(shell) {
        return Err(unavailable());
    }
    shell
        .directory_member_fetches
        .slot_mut(&dispatch.context)?
        .manager
        .timeout(&dispatch.token)
}
pub(crate) fn refuse(shell: &mut ShellState, dispatch: &Dispatch) -> Result<(), Error> {
    // Local retirement is allowed to release only its exact old running token;
    // it is never converted into a failed terminal or source placeholder.
    let slot = shell.directory_member_fetches.slot_mut(&dispatch.context)?;
    slot.manager.refuse(&dispatch.token)?;
    slot.notify();
    Ok(())
}
pub(crate) fn publish_observed_terminal(
    shell: &mut ShellState,
    dispatch: Arc<Dispatch>,
    terminal: NetworkTerminal,
    pages: Vec<Arc<Page>>,
    errors: Vec<Arc<Error>>,
) -> Result<(), Error> {
    if !dispatch.current(shell) {
        return Err(unavailable());
    }
    let ids: BTreeSet<_> = dispatch.capture.ids().iter().map(String::as_str).collect();
    if pages.iter().any(|p| {
        p.request()
            .ids()
            .iter()
            .any(|id| !ids.contains(id.as_str()))
    }) || matches!(&terminal,NetworkTerminal::Complete(rows) if rows.len()>100_000 || rows.iter().any(|row|!row["id"].as_str().is_some_and(|id|ids.contains(id)) || !row.get("is_stranger").is_some_and(Value::is_boolean)))
    {
        return Err(unavailable());
    }
    let slot = shell.directory_member_fetches.slot_mut(&dispatch.context)?;
    slot.manager.publish(
        &dispatch.token,
        Outcome {
            dispatch: dispatch.clone(),
            terminal,
            pages,
            errors,
        },
    )?;
    slot.notify();
    Ok(())
}
/// Publish a completed source subset while retaining cooldown-blocked IDs.
/// Evidence pages stay bound to the original dispatch; only completed IDs can
/// produce direct rows/placeholders. Deferred IDs keep their subscriptions.
pub(crate) fn publish_observed_completed(
    shell: &mut ShellState,
    dispatch: Arc<Dispatch>,
    completed: Vec<String>,
    terminal: NetworkTerminal,
    pages: Vec<Arc<Page>>,
    errors: Vec<Arc<Error>>,
) -> Result<(), Error> {
    if completed == dispatch.token.ids() {
        return publish_observed_terminal(shell, dispatch, terminal, pages, errors);
    }
    if !dispatch.current(shell) {
        return Err(unavailable());
    }
    let original: BTreeSet<_> = dispatch.token.ids().iter().map(String::as_str).collect();
    let complete: BTreeSet<_> = completed.iter().map(String::as_str).collect();
    let requested: BTreeSet<_> = dispatch.capture.ids().iter().map(String::as_str).collect();
    if complete.len() != completed.len()
        || !complete.is_subset(&original)
        || !matches!(&terminal,NetworkTerminal::Complete(rows) if rows.len()<=100_000 && rows.iter().all(|row| row["id"].as_str().is_some_and(|id|complete.contains(id) && requested.contains(id)) && row.get("is_stranger").is_some_and(Value::is_boolean)))
        || pages.iter().any(|page| {
            page.request()
                .ids()
                .iter()
                .any(|id| !requested.contains(id.as_str()))
        })
    {
        return Err(unavailable());
    }
    if completed.is_empty() {
        let slot = shell.directory_member_fetches.slot_mut(&dispatch.context)?;
        slot.manager.defer(&dispatch.token)?;
        slot.notify();
        return Ok(());
    }
    let ctx = &dispatch.context;
    let w = shell
        .core
        .workspaces
        .get(&ctx.session.team_id)
        .ok_or_else(unavailable)?;
    let prior =
        w.directory_members
            .observations(&ctx.account, &ctx.session, ctx.epoch, GENERATION)?;
    let capture = Capture::network_only(prior, &completed)?;
    let slot = shell.directory_member_fetches.slot_mut(ctx)?;
    slot.manager
        .publish_completed(&dispatch.token, &completed, |token| Outcome {
            dispatch: Arc::new(Dispatch {
                context: dispatch.context.clone(),
                token,
                capture,
                native_before: dispatch.native_before.clone(),
            }),
            terminal,
            pages,
            errors,
        })?;
    slot.notify();
    Ok(())
}
pub(crate) fn claim_adoption(
    shell: &mut ShellState,
    sub: &Subscription,
) -> Result<Option<(Adoption, Arc<Outcome>)>, Error> {
    if status(shell, sub) == claim::Status::Retired {
        return Err(unavailable());
    }
    let result = shell
        .directory_member_fetches
        .slot_mut(&sub.context)?
        .manager
        .claim_adoption(&sub.token)?;
    let Some((token, receipt)) = result else {
        return Ok(None);
    };
    if !receipt.current(shell) {
        let slot = shell.directory_member_fetches.slot_mut(&sub.context)?;
        slot.manager.refuse_receipt(&receipt.dispatch.token)?;
        slot.notify();
        return Err(unavailable());
    }
    Ok(Some((
        Adoption {
            context: sub.context.clone(),
            token,
        },
        receipt,
    )))
}
pub(crate) fn release_adoption(shell: &mut ShellState, adoption: &Adoption) -> Result<(), Error> {
    let slot = shell.directory_member_fetches.slot_mut(&adoption.context)?;
    slot.manager.release_adoption(&adoption.token)?;
    slot.notify();
    Ok(())
}
/// Invoke synchronously in the same serial mutation that installed the checked
/// Store+native pair. Consumer Query/renderer checks remain mandatory before it.
pub(crate) fn settle(shell: &mut ShellState, adoption: &Adoption) -> Result<(), Error> {
    if !adoption.current(shell) {
        return Err(unavailable());
    }
    let w = shell
        .core
        .workspaces
        .get(&adoption.context.session.team_id)
        .ok_or_else(unavailable)?;
    let slot = shell.directory_member_fetches.slot_mut(&adoption.context)?;
    slot.manager.settle(
        &adoption.token,
        &w.directory_members,
        &adoption.context.account,
        &adoption.context.session,
        &w.users,
    )?;
    slot.notify();
    Ok(())
}
pub(crate) fn retire(shell: &mut ShellState, sub: &Subscription) {
    if let Ok(slot) = shell.directory_member_fetches.slot_mut(&sub.context) {
        slot.manager.retire(&sub.token);
        slot.notify();
    }
}
/// Called only by the two fresh-Shell installation paths. Same-context claims
/// survive a shell rebuild; replaced credentials/Transport/RT retire locally.
pub(crate) fn carry_into_fresh_shell(previous: &mut ShellState, loaded: &mut ShellState) {
    let mut registry = std::mem::take(&mut previous.directory_member_fetches);
    registry
        .slots
        .retain(|_, slot| slot.context.current(loaded));
    registry.automatic.retain(loaded);
    loaded.directory_member_fetches = registry;
}
