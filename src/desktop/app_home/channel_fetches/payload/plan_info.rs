//! Positive PlanInfo actions with original source-owner custody.
use super::*;

#[derive(Default)]
pub(super) struct State {
    revision: u64,
    observed: Option<Snapshot>,
    dirty: bool,
}

mod display;
pub(crate) use display::{Display, capture as display};
pub(crate) mod warm_fixture;

pub(super) fn persist(mut state: Signal<ShellState>, owner: &Owner) {
    {
        let s = state.peek();
        if !owner.current(&s)
            || !s.app_home.channel_fetches.slots[&owner.context.key()]
                .payload
                .plan_info
                .dirty
        {
            return;
        }
    }
    let dirty = {
        #[allow(deprecated)]
        let mut s = state.write_silent();
        if !owner.current(&s) {
            return;
        }
        std::mem::take(
            &mut s
                .app_home
                .channel_fetches
                .slots
                .get_mut(&owner.context.key())
                .unwrap()
                .payload
                .plan_info
                .dirty,
        )
    };
    if dirty {
        crate::bootstrap::persist_workspace_metadata(&state, &owner.context.session.team_id);
    }
}

#[derive(Clone)]
pub(crate) struct Snapshot {
    owner: Owner,
    revision: u64,
    raw: Value,
}
impl Snapshot {
    pub(crate) fn raw(&self) -> &Value {
        &self.raw
    }
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        if !self.owner.current(shell) {
            return false;
        }
        let ctx = &self.owner.context;
        let plan = &shell.app_home.channel_fetches.slots[&ctx.key()]
            .payload
            .plan_info;
        plan.revision == self.revision
            && plan.observed.as_ref().is_some_and(|v| {
                Arc::ptr_eq(&v.owner.token, &self.owner.token)
                    && Arc::ptr_eq(&v.owner.context, &self.owner.context)
                    && v.raw == self.raw
            })
            && shell
                .core
                .workspaces
                .get(&ctx.session.team_id)
                .is_some_and(|w| w.cfm_payload.plan_info.as_ref() == Some(&self.raw))
    }
}

/// Does not initialize a source, acquire data or certify retained warm state.
pub(crate) fn capture(shell: &ShellState, team: &str) -> Option<Snapshot> {
    let account = shell.core.active_account.as_ref()?;
    let session = shell.core.session.as_ref()?.workspaces.get(team)?;
    let key = (account.clone(), team.to_owned(), session.user_id.clone());
    let snapshot = shell
        .app_home
        .channel_fetches
        .slots
        .get(&key)?
        .payload
        .plan_info
        .observed
        .as_ref()?;
    snapshot.current(shell).then(|| snapshot.clone())
}

/// nsR3 processPlanInfo sees the merged canonical team, not just this partial.
/// Native Context has an actual current team, so the source enterprise fallback
/// cannot override that identity. JSON null is defined and must be forwarded.
pub(super) fn team_upsert(
    shell: &mut ShellState,
    owner: &Owner,
    merged: &Value,
) -> Result<(), source::Error> {
    if !owner.current(shell) {
        return Err(source::Error::Superseded);
    }
    if merged["id"].as_str() != Some(owner.context.session.team_id.as_str()) {
        return Ok(());
    }
    let fields = ["is_plan_frozen", "reconnect_grace_period_start_ts"]
        .into_iter()
        .filter_map(|key| merged.get(key).map(|v| (key.to_owned(), v.clone())))
        .collect::<serde_json::Map<_, _>>();
    if fields.is_empty() {
        return Ok(());
    }
    apply(
        shell,
        owner,
        &source::Effect {
            kind: source::Kind::PlanInfo,
            payload: Value::Object(fields),
        },
    )
}

pub(super) fn apply(
    shell: &mut ShellState,
    owner: &Owner,
    effect: &source::Effect,
) -> Result<(), source::Error> {
    if effect.kind != source::Kind::PlanInfo || !owner.current(shell) {
        return Err(source::Error::Superseded);
    }
    let ctx = &owner.context;
    let revision = shell.app_home.channel_fetches.slots[&ctx.key()]
        .payload
        .plan_info
        .revision
        .checked_add(1)
        .ok_or(source::Error::Adoption)?;
    let workspace = shell
        .core
        .workspaces
        .get_mut(&ctx.session.team_id)
        .ok_or(source::Error::Superseded)?;
    // Cached data is only a matching merge base, never a restored live Owner.
    // Reduce into a temporary value so malformed actions cannot change it.
    let mut next = source::Stores::default();
    next.plan_info = workspace.cfm_payload.plan_info.clone().or_else(|| {
        workspace
            .cfm_payload
            .plan_info_cache
            .as_ref()
            .filter(|v| v.matches(&ctx.account, &ctx.session))
            .map(|v| v.raw().clone())
    });
    next.apply(effect)?;
    let raw = next.plan_info.ok_or(source::Error::Adoption)?;
    workspace.cfm_payload.plan_info = Some(raw.clone());
    workspace.cfm_payload.plan_info_cache =
        source::PlanInfoCache::accepted(&ctx.account, &ctx.session, raw.clone());
    let dirty = workspace.cfm_payload.plan_info_cache.is_some();
    let plan = &mut shell
        .app_home
        .channel_fetches
        .slots
        .get_mut(&ctx.key())
        .ok_or(source::Error::Superseded)?
        .payload
        .plan_info;
    plan.revision = revision;
    plan.dirty |= dirty;
    plan.observed = Some(Snapshot {
        owner: owner.clone(),
        revision,
        raw,
    });
    Ok(())
}

#[cfg(test)]
mod tests;
