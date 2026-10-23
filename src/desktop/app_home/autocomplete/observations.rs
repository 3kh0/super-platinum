//! Complete successful Share observations, owned by the account rather than a
//! transient search. No persistence, bootstrap synthesis, or cold offline default.
use crate::{scheduled::Context, state::ShellState};
use serde_json::Value;
mod acquire;
mod inputs;
mod names;
mod startup;
pub(crate) use acquire::{Acquired, acquire, acquire_with};
pub(crate) use names::assignment;
pub(crate) use startup::Startup;
use std::sync::Arc;
use super_platinum_core::{
    block_actions::name_policy as policy,
    composer_emoji::history::History,
    slack::{Error, events::RtEvent},
    state::posting_metadata::authority::Authority,
};

#[derive(Clone)]
pub(crate) struct Names {
    pub(crate) policy: policy::Snapshot,
    pub(crate) history: History,
    pub(crate) enterprise: Option<String>,
    pub(crate) self_user: Value,
    pub(crate) default_workspace: Option<String>,
    pub(crate) reads: Vec<Value>,
}

pub(crate) struct Complete {
    pub(crate) names: Names,
    pub(crate) assignments: Value,
    pub(crate) reads: Vec<Value>,
}
impl Complete {
    fn valid(&self, user: &str) -> bool {
        self.names.self_user["id"] == user
            && self.names.policy.should_display_real_names().is_ok()
            && self.assignments["ok"] == true
            && !self.assignments.get("error").is_some_and(|v| !v.is_null())
            && self.assignments["assignments"]
                .as_object()
                .is_some_and(|values| {
                    [
                        "autocomplete_flannel_fuzzy_match_passthrough",
                        "autocomplete_affinity_score",
                        "autocomplete_affinity_ml_model",
                    ]
                    .iter()
                    .all(|key| values.get(*key).is_none_or(|v| v["group"].is_string()))
                })
    }
}

#[derive(Default)]
pub(crate) struct Registry {
    owner: Option<Arc<Owner>>,
    revision: u64,
    pending: Option<Arc<Job>>,
    startup: Option<Startup>,
    stored: Option<Store>,
}

// Deliberately no Debug/Serialize: this witness contains credentials.
struct Owner {
    context: Arc<Context>,
    read_owner: crate::directory_member_fetches::Context,
    account: String,
    cookie: String,
    rt: u64,
    self_native: Value,
    authority: Authority,
    preferences: u64,
    inputs: inputs::Inputs,
}
impl Owner {
    fn capture(s: &ShellState, context: Arc<Context>, authority: Authority) -> Result<Self, Error> {
        let w = s
            .core
            .workspaces
            .get(&context.session.team_id)
            .ok_or_else(unavailable)?;
        let result = Self {
            read_owner: crate::directory_member_fetches::Context::capture(
                s,
                &context.session.team_id,
            )?,
            account: s.core.active_account.clone().ok_or_else(unavailable)?,
            cookie: s
                .core
                .session
                .as_ref()
                .ok_or_else(unavailable)?
                .d_cookie
                .clone(),
            rt: w.rt_generation,
            self_native: serde_json::to_value(
                w.users
                    .get(&context.session.user_id)
                    .ok_or_else(unavailable)?,
            )
            .map_err(|_| unavailable())?,
            context,
            authority,
            preferences: super::preferences_revision(s),
            inputs: inputs::Inputs::capture(&w.cfm_payload),
        };
        result.current(s).then_some(result).ok_or_else(unavailable)
    }
    // Event admission deliberately excludes fields the admitted event changed.
    fn admission(&self, s: &ShellState) -> bool {
        self.context.current(s)
            && self.read_owner.current(s)
            && s.core.active_account.as_deref() == Some(&self.account)
            && s.core.session.as_ref().is_some_and(|session| {
                session.d_cookie == self.cookie
                    && session
                        .workspaces
                        .get(&self.context.session.team_id)
                        .is_some_and(|actual| {
                            crate::emoji_preferences::same_session(actual, &self.context.session)
                        })
            })
            && s.core
                .workspaces
                .get(&self.context.session.team_id)
                .is_some_and(|w| w.rt_generation == self.rt)
    }
    fn current(&self, s: &ShellState) -> bool {
        self.admission(s)
            && self.authority.current(&s.core)
            && self.preferences == super::preferences_revision(s)
            && s.core
                .workspaces
                .get(&self.context.session.team_id)
                .is_some_and(|w| {
                    self.inputs.current(&w.cfm_payload)
                        && w.self_user_id == self.context.session.user_id
                        && w.users
                            .get(&self.context.session.user_id)
                            .and_then(|u| serde_json::to_value(u).ok())
                            .as_ref()
                            == Some(&self.self_native)
                })
    }
}

#[derive(Clone)]
pub(crate) struct Store {
    owner: Arc<Owner>,
    identity: Arc<()>,
    revision: u64,
    complete: Arc<Complete>,
}
impl Store {
    pub(crate) fn capture(s: &ShellState) -> Option<Self> {
        s.app_home
            .share_ranking_observations
            .stored
            .as_ref()
            .filter(|v| v.current(s))
            .cloned()
    }
    pub(crate) fn current(&self, s: &ShellState) -> bool {
        let registry = &s.app_home.share_ranking_observations;
        self.owner.current(s)
            && registry.revision == self.revision
            && registry
                .owner
                .as_ref()
                .is_some_and(|o| Arc::ptr_eq(o, &self.owner))
            && registry
                .stored
                .as_ref()
                .is_some_and(|v| Arc::ptr_eq(&v.identity, &self.identity))
    }
    pub(crate) fn complete(&self) -> &Complete {
        &self.complete
    }
}

pub(crate) struct Pending {
    owner: Arc<Owner>,
    revision: u64,
    token: Arc<Job>,
}
impl Pending {
    pub(crate) fn capture(
        s: &mut ShellState,
        context: Arc<Context>,
        authority: Authority,
    ) -> Result<Self, Error> {
        let owner = Arc::new(Owner::capture(s, context, authority)?);
        let registry = &mut s.app_home.share_ranking_observations;
        registry.revision = registry.revision.wrapping_add(1);
        registry.stored = None;
        registry.owner = Some(owner.clone());
        if let Some(previous) = registry.pending.take() {
            previous.finish();
        }
        let token = Arc::new(Job::new());
        registry.pending = Some(token.clone());
        Ok(Self {
            owner,
            revision: registry.revision,
            token,
        })
    }
    pub(crate) fn current(&self, s: &ShellState) -> bool {
        let r = &s.app_home.share_ranking_observations;
        !self.token.finished()
            && self.owner.current(s)
            && r.revision == self.revision
            && r.owner
                .as_ref()
                .is_some_and(|v| Arc::ptr_eq(v, &self.owner))
            && r.pending
                .as_ref()
                .is_some_and(|v| Arc::ptr_eq(v, &self.token))
    }
    pub(crate) fn publish(self, s: &mut ShellState, complete: Complete) -> Result<Store, Error> {
        if !self.current(s) || !complete.valid(&self.owner.context.session.user_id) {
            return Err(unavailable());
        }
        let result = Store {
            owner: self.owner.clone(),
            identity: Arc::new(()),
            revision: self.revision,
            complete: Arc::new(complete),
        };
        let r = &mut s.app_home.share_ranking_observations;
        r.pending = None;
        r.stored = Some(result.clone());
        Ok(result)
    }
}

// A producer owns Pending; waiters own only a completion receiver. Dropping a
// waiter never cancels the producer. A dropped producer always wakes waiters.
struct Job {
    completed: tokio::sync::watch::Sender<bool>,
}
impl Job {
    fn new() -> Self {
        Self {
            completed: tokio::sync::watch::channel(false).0,
        }
    }
    fn finish(&self) {
        self.completed.send_replace(true);
    }
    fn finished(&self) -> bool {
        *self.completed.borrow()
    }
}
impl Drop for Pending {
    fn drop(&mut self) {
        self.token.finish();
    }
}

/// Passive evidence only; credentials and owner snapshots are never serialized.
pub(crate) fn snapshot(s: &ShellState) -> Value {
    let r = &s.app_home.share_ranking_observations;
    let store = Store::capture(s);
    serde_json::json!({
        "currentComplete":store.is_some(), "revision":r.revision,
        "pending":r.owner.as_ref().is_some_and(|v|v.current(s))
            && r.pending.as_ref().is_some_and(|v|!v.finished()),
        "startupAttempted":r.startup.as_ref().is_some_and(|v|v.current(s)),
        "reads":store.as_ref().map(|v|&v.complete.reads)
    })
}

/// Retires even a first acquisition with no installed Store, independently of
/// Home/About visibility. The caller supplies the admitted socket generation.
pub(crate) fn realtime(s: &mut ShellState, team: &str, generation: u64, event: &RtEvent) {
    let registry = &s.app_home.share_ranking_observations;
    let Some(owner) = registry
        .startup
        .as_ref()
        .map(|v| &v.owner)
        .filter(|v| v.admission(s))
        .or(registry.owner.as_ref())
    else {
        return;
    };
    if team != owner.context.session.team_id || generation != owner.rt || !owner.admission(s) {
        return;
    }
    let changed = match event {
        RtEvent::PreferencesChanged { .. } => true,
        RtEvent::UserChanged(user) => user.id == owner.context.session.user_id,
        RtEvent::Unknown(raw) => matches!(
            raw.kind.as_str(),
            "team_pref_change" | "enterprise_prefs_updated" | "team_join" | "team_leave"
        ),
        _ => false,
    };
    if changed {
        let r = &mut s.app_home.share_ranking_observations;
        r.revision = r.revision.wrapping_add(1);
        r.stored = None;
        if let Some(job) = r.pending.take() {
            job.finish();
        }
        r.startup = None;
    }
}
fn unavailable() -> Error {
    Error::Api("share_complete_observations_unavailable".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use super_platinum_core::state::posting_metadata::authority::BootRead;

    pub(super) fn setup() -> (ShellState, Arc<Context>, Authority) {
        let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
        s.core.active_account = Some("complete-observation-fixture".into());
        crate::app_home::fixture::install(&mut s);
        let ctx = Arc::new(Context::capture(&s).unwrap());
        s.core
            .session
            .as_mut()
            .unwrap()
            .unified_user_client_enabled
            .insert(ctx.session.team_id.clone(), false);
        let read = BootRead::begin(&mut s.core, &ctx.transport, &ctx.session).unwrap();
        assert!(
            read.apply(
                &mut s.core,
                json!({"ok":true,
            "team":{"id":ctx.session.team_id,"plan":"plus"},
            "self":{"id":ctx.session.user_id,"team_id":ctx.session.team_id},
            "workspaces":[{"id":ctx.session.team_id}],
            "read_only_channels":[],"thread_only_channels":[]}),
                false
            )
            .unwrap()
        );
        let authority = Authority::capture(&s.core, &ctx.session.team_id).unwrap();
        (s, ctx, authority)
    }
    fn complete(s: &ShellState, c: &Context, a: &Authority) -> Complete {
        let (membership, enterprise) = a
            .name_policy_membership(&s.core, policy::OrdinaryClientV2)
            .unwrap();
        let raw = json!({"ok":true,"prefs":{"display_real_names_override":1}});
        let policy = policy::Snapshot::new(
            policy::OrdinaryClientV2,
            membership,
            policy::UserPrefs::decode(&raw).unwrap(),
            None,
        )
        .unwrap();
        let reads = vec![json!({"method":"users.prefs.get","raw":raw})];
        Complete {
            names: Names {
                policy,
                history: History::default(),
                enterprise,
                self_user: json!({"id":c.session.user_id}),
                default_workspace: None,
                reads: reads.clone(),
            },
            assignments: json!({"ok":true,"assignments":{}}),
            reads,
        }
    }
    fn installed() -> (ShellState, Store) {
        let (mut s, c, a) = setup();
        let data = complete(&s, &c, &a);
        let pending = Pending::capture(&mut s, c, a).unwrap();
        let stored = pending.publish(&mut s, data).unwrap();
        (s, stored)
    }
    #[test]
    fn complete_store_reuses_across_ui_queries_without_granting_new_owner() {
        let (mut s, stored) = installed();
        let reads = stored.complete().reads.clone();
        s.channel_generation += 1;
        s.app_home.surface = None;
        assert!(stored.current(&s));
        assert_eq!(Store::capture(&s).unwrap().complete().reads, reads);
        s.core.active_account = Some("other".into());
        assert!(!stored.current(&s));
        assert!(Store::capture(&s).is_none());
    }
    #[test]
    fn complete_store_retires_on_credentials_socket_self_and_preferences() {
        for change in 0..8 {
            let (mut s, stored) = installed();
            let team = stored.owner.context.session.team_id.clone();
            let user = stored.owner.context.session.user_id.clone();
            match change {
                0 => s.core.account_epoch += 1,
                1 => s.core.session.as_mut().unwrap().d_cookie.push('x'),
                2 => s
                    .core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut(&team)
                    .unwrap()
                    .token
                    .push('x'),
                3 => s.core.workspaces.get_mut(&team).unwrap().rt_generation += 1,
                4 => {
                    let w = s.core.workspaces.get_mut(&team).unwrap();
                    let mut raw = serde_json::to_value(&w.users[&user]).unwrap();
                    raw["name"] = "changed".into();
                    w.users.insert(user, serde_json::from_value(raw).unwrap());
                }
                5 => s.app_home.workflow_names_revision += 1,
                6 => {
                    s.core
                        .session
                        .as_mut()
                        .unwrap()
                        .unified_user_client_enabled
                        .insert(team, true);
                }
                _ => {
                    let mut other = s.core.session.as_ref().unwrap().workspaces[&team].clone();
                    other.team_id = "TOTHER".into();
                    s.core
                        .session
                        .as_mut()
                        .unwrap()
                        .workspaces
                        .insert(other.team_id.clone(), other);
                }
            }
            assert!(!stored.current(&s), "change {change}");
        }
    }
    #[test]
    fn admitted_preference_event_retires_pending_even_without_visible_surface() {
        let (mut s, c, a) = setup();
        let p = Pending::capture(&mut s, c.clone(), a).unwrap();
        s.app_home.surface = None;
        let generation = s.core.workspaces[&c.session.team_id].rt_generation;
        let event = RtEvent::PreferencesChanged {
            name: "frecency".into(),
            value: json!("{}"),
        };
        realtime(&mut s, &c.session.team_id, generation + 1, &event);
        realtime(&mut s, "OTHER", generation, &event);
        assert!(p.current(&s));
        realtime(&mut s, &c.session.team_id, generation, &event);
        assert!(!p.current(&s));
        assert!(Store::capture(&s).is_none());
    }
    #[test]
    fn admitted_self_change_can_retire_after_native_self_has_changed() {
        let (mut s, stored) = installed();
        let team = stored.owner.context.session.team_id.clone();
        let user = stored.owner.context.session.user_id.clone();
        let generation = s.core.workspaces[&team].rt_generation;
        let mut raw = serde_json::to_value(&s.core.workspaces[&team].users[&user]).unwrap();
        raw["name"] = "new name".into();
        let changed: super_platinum_core::slack::models::User =
            serde_json::from_value(raw).unwrap();
        s.core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .users
            .insert(user, changed.clone());
        realtime(&mut s, &team, generation, &RtEvent::UserChanged(changed));
        assert!(s.app_home.share_ranking_observations.stored.is_none());
    }
    #[test]
    fn newer_pending_rejects_older_complete_publication() {
        let (mut s, c, a) = setup();
        let data = complete(&s, &c, &a);
        let first = Pending::capture(&mut s, c.clone(), a.clone()).unwrap();
        let second = Pending::capture(&mut s, c, a).unwrap();
        assert!(!first.current(&s));
        assert!(first.publish(&mut s, data).is_err());
        assert!(second.current(&s));
    }
    #[test]
    fn malformed_assignments_or_wrong_self_never_install_complete() {
        for raw in [
            json!({"ok":true}),
            json!({"ok":false,"assignments":{}}),
            json!({"ok":true,"assignments":{},"error":"denied"}),
            json!({"ok":true,"assignments":{"autocomplete_affinity_score":{"group":true}}}),
        ] {
            let (mut s, c, a) = setup();
            let mut data = complete(&s, &c, &a);
            data.assignments = raw;
            let p = Pending::capture(&mut s, c, a).unwrap();
            assert!(p.publish(&mut s, data).is_err());
            assert!(Store::capture(&s).is_none());
        }
        let (mut s, c, a) = setup();
        let mut data = complete(&s, &c, &a);
        data.names.self_user["id"] = "OTHER".into();
        assert!(
            Pending::capture(&mut s, c, a)
                .unwrap()
                .publish(&mut s, data)
                .is_err()
        );
    }
}

#[cfg(test)]
#[path = "observations/acquisition_tests.rs"]
mod acquisition_tests;
