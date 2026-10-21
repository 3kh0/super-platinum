//! Manager selection shares the original Permissions lease, never its own draft authority.
use super::*;
use protocol::manager_settings::{Draft, Plan};
use std::collections::BTreeMap;
mod acquire;
mod actions;
mod agents;
pub(in crate::app_home::about::management) mod departure;
mod group;
mod native;
mod source;
mod view;
pub(super) use actions::{bind, bump_cancel, change, search};
pub(super) use departure::{dispatch_current, dispatch_native, maybe_depart};
pub(super) use group::execute;
pub(in crate::app_home::about::management) use view::panel;

pub(super) struct Managers {
    pub(super) draft: Draft,
    source: source::Source,
    rows: BTreeMap<String, source::Row>,
    pub(super) epoch: u64,
    generation: u64,
    query: String,
    results: Vec<String>,
    pending: bool,
    hydration_started: bool,
    notice: Option<String>,
    receipts: Vec<Value>,
    agent_facts: Option<agents::Attempt>,
    pub(super) speed_bump: bool,
}
impl Managers {
    pub(super) fn capture(s: &ShellState, o: &Original) -> Option<Self> {
        let source = source::Source::capture(s, o)?;
        let rows = source.rows(s, o);
        let results = rows
            .values()
            .filter(|r| r.eligible)
            .take(24)
            .map(|r| r.id.clone())
            .collect();
        Some(Self {
            draft: Draft::new(&o.editor, &o.owner.ctx.session.user_id).ok()?,
            source,
            rows,
            epoch: next(),
            generation: next(),
            query: String::new(),
            results,
            pending: false,
            hydration_started: false,
            notice: None,
            receipts: vec![],
            agent_facts: None,
            speed_bump: false,
        })
    }
    fn ids(&self) -> Vec<String> {
        self.draft.selected().iter().flatten().cloned().collect()
    }
    fn missing(&self) -> Vec<String> {
        self.ids()
            .into_iter()
            .filter(|id| !self.rows.contains_key(id))
            .collect()
    }
    pub(super) fn current(&self, s: &ShellState, o: &Original) -> bool {
        self.source.current(s, o)
    }
    fn facts_loading(&self) -> bool {
        self.agent_facts
            .as_ref()
            .is_some_and(|a| a.loading(self.generation))
    }
    fn facts_retry(&self) -> bool {
        !self.pending
            && self
                .agent_facts
                .as_ref()
                .is_some_and(|a| a.retry(self.generation))
    }
    pub(super) fn valid(&self) -> bool {
        !self.pending && self.draft.resolved().is_ok()
    }
    pub(super) fn expected(&self) -> Value {
        json!({"epoch":self.epoch,"ids":self.ids(),"query":self.query,"results":self.results,"speedBump":self.speed_bump})
    }
    pub(super) fn snapshot(&self) -> Value {
        let mut snapshot = json!({"epoch":self.epoch,"generation":self.generation,"ids":self.ids(),"query":self.query,"results":self.results,"pending":self.pending,"notice":self.notice,"receipts":self.receipts,"selfRemoval":self.draft.self_removal(),"speedBump":self.speed_bump,"rows":self.rows.values().map(|r|json!({"id":r.id,"label":r.label,"eligible":r.eligible,"eligibility":r.eligibility,"raw":r.raw})).collect::<Vec<_>>()});
        if let Some(facts) = &self.agent_facts {
            snapshot["agentFacts"] = facts.snapshot();
        }
        snapshot
    }
}
pub(super) struct Group {
    pub(super) plan: Plan,
    pub(super) failures: Vec<Option<&'static str>>,
}
impl Group {
    pub(super) fn new(plan: Plan) -> Self {
        Self {
            failures: vec![None; plan.slots().len()],
            plan,
        }
    }
    pub(super) fn complete(&self) -> bool {
        self.plan.complete()
    }
    pub(super) fn no_send(&self) -> bool {
        self.plan
            .slots()
            .iter()
            .all(|s| matches!(s.outcome(), Outcome::Pending | Outcome::NotSent))
    }
    pub(super) fn snapshot(&self) -> Value {
        json!({"selected":self.plan.selected(),"selfRemoval":self.plan.self_removal(),"complete":self.complete(),"slots":self.plan.slots().iter().enumerate().map(|(i,s)|json!({"index":i,"method":s.plan().method(),"arguments":s.plan().arguments(),"prerequisite":s.prerequisite(),"outcome":match s.outcome(){Outcome::Pending=>"pending",Outcome::InFlight=>"in-flight",Outcome::NotSent=>"not-sent",Outcome::Succeeded(_)=>"acknowledged",Outcome::Rejected(_)=>"rejected",Outcome::Unknown=>"unknown"},"raw":self.plan.responses()[i],"failure":self.failures[i]})).collect::<Vec<_>>()})
    }
}
pub(super) fn owned<'a>(s: &'a ShellState, o: &Original) -> Option<&'a Managers> {
    settings::owned(s, o.visit, o.id)?.managers.as_ref()
}
pub(super) fn owned_mut(s: &mut ShellState) -> Option<&mut Managers> {
    s.app_home
        .about
        .management
        .permissions
        .confirmation
        .as_mut()?
        .managers
        .as_mut()
}
pub(super) fn ready(s: &ShellState, o: &Original, g: u64) -> bool {
    owned(s, o).is_some_and(|m| m.generation == g && m.current(s, o))
}

pub(super) async fn verify(state: Signal<ShellState>, o: &Original) -> bool {
    let e = {
        let s = state.peek();
        let Some(m) = owned(&s, o).filter(|m| m.current(&s, o)) else {
            return false;
        };
        m.expected()
    };
    native::witness(o, &e, None, false).await
        && owned(&state.peek(), o)
            .is_some_and(|m| m.current(&state.peek(), o) && Some(m.epoch) == e["epoch"].as_u64())
}
