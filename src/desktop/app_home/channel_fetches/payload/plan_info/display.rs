//! Display-only fallback. It cannot create a live PlanInfo Snapshot.
use super::*;

#[derive(Clone)]
pub(crate) enum Display {
    Live(Snapshot),
    Retained {
        owner: Owner,
        revision: u64,
        canonical: Arc<canonical::Canonical>,
        record: source::PlanInfoCache,
    },
}
impl Display {
    pub(crate) fn raw(&self) -> &Value {
        match self {
            Self::Live(v) => v.raw(),
            Self::Retained { record, .. } => record.raw(),
        }
    }
    pub(crate) fn provenance(&self) -> &'static str {
        match self {
            Self::Live(_) => "live",
            Self::Retained { .. } => "retained-cache",
        }
    }
    pub(crate) fn current(&self, s: &ShellState) -> bool {
        match self {
            Self::Live(v) => v.current(s),
            Self::Retained {
                owner,
                revision,
                canonical,
                record,
            } => {
                if !owner.current(s) || !canonical.current(s) {
                    return false;
                }
                let ctx = &owner.context;
                let plan = &s.app_home.channel_fetches.slots[&ctx.key()]
                    .payload
                    .plan_info;
                plan.revision == *revision
                    && plan.observed.is_none()
                    && record.matches(&ctx.account, &ctx.session)
                    && s.core
                        .workspaces
                        .get(&ctx.session.team_id)
                        .is_some_and(|w| {
                            w.cfm_payload.plan_info_cache.as_ref() == Some(record)
                                && w.cfm_payload.plan_info.is_none()
                        })
            }
        }
    }
}
pub(crate) fn capture(s: &ShellState, team: &str) -> Option<Display> {
    if let Some(live) = super::capture(s, team) {
        return Some(Display::Live(live));
    }
    let ctx = Context::capture(s, team).ok()?;
    let slot = s.app_home.channel_fetches.slots.get(&ctx.key())?;
    // An invalidated positive live action must not be recertified as cached.
    if slot.payload.plan_info.observed.is_some() {
        return None;
    }
    let result = Display::Retained {
        owner: Owner {
            context: slot.context.clone(),
            token: slot.token.clone(),
        },
        revision: slot.payload.plan_info.revision,
        canonical: canonical::observed(s, team).ok()?,
        record: s
            .core
            .workspaces
            .get(team)?
            .cfm_payload
            .plan_info_cache
            .clone()?,
    };
    result.current(s).then_some(result)
}
