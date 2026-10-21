//! Positive PlanInfo actions with original source-owner custody.
use super::*;

#[derive(Default)]
pub(super) struct State {
    revision: u64,
    observed: Option<Snapshot>,
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
    workspace.cfm_payload.apply(effect)?;
    let raw = workspace
        .cfm_payload
        .plan_info
        .clone()
        .ok_or(source::Error::Adoption)?;
    let plan = &mut shell
        .app_home
        .channel_fetches
        .slots
        .get_mut(&ctx.key())
        .ok_or(source::Error::Superseded)?
        .payload
        .plan_info;
    plan.revision = revision;
    plan.observed = Some(Snapshot {
        owner: owner.clone(),
        revision,
        raw,
    });
    Ok(())
}

#[cfg(test)]
mod tests;
