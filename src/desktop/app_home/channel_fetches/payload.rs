//! Synchronous split-payload sinks. Unsupported effects are retained failures.
use super::*;
use super_platinum_core::state::{cfm_payload as source, directory_members};
mod auxiliary;
pub(crate) mod plan_info;
pub(super) fn kick(mut state: Signal<ShellState>, owner: Owner) {
    let works = {
        let mut shell = state.write();
        if !owner.current(&shell) {
            return;
        }
        std::mem::take(
            &mut shell
                .app_home
                .channel_fetches
                .slots
                .get_mut(&owner.context.key())
                .unwrap()
                .payload
                .group_work,
        )
    };
    for work in works {
        let owner = owner.clone();
        dioxus::core::spawn_forever(async move {
            let _ = work.run(state, |shell| owner.current(shell)).await;
        });
    }
    auxiliary::kick(state, owner);
}

pub(super) struct State {
    plan_info: plan_info::State,
    auxiliary: auxiliary::State,
    group_work: Vec<crate::usergroups::source::Work>,
    pub(super) screen_reader: ScreenReader,
    pub(super) workspace_ids: Option<Vec<String>>,
    pub(super) teams: BTreeMap<String, Value>,
    pub(super) settlements: Vec<source::Settlement>,
    pub(super) has_more_mpdms: Option<Value>,
    pub(super) visitors: BTreeMap<String, Value>,
    pub(super) temporary_expirations: Option<Value>,
}
pub(super) enum ScreenReader {
    BridgeUnavailable,
    Observed(bool),
}
impl ScreenReader {
    fn source_value(&self) -> bool {
        match self {
            Self::BridgeUnavailable => false,
            Self::Observed(value) => *value,
        }
    }
}
impl Default for State {
    fn default() -> Self {
        // Actual v7Vi.VO selector unions absent internal/external stores to [].
        // Electron's missing accessibility bridge yields undefined -> !!false;
        // retain that provenance instead of claiming an OS observation.
        Self {
            plan_info: plan_info::State::default(),
            auxiliary: auxiliary::State::default(),
            group_work: vec![],
            screen_reader: ScreenReader::BridgeUnavailable,
            workspace_ids: Some(vec![]),
            teams: BTreeMap::new(),
            settlements: vec![],
            has_more_mpdms: None,
            visitors: BTreeMap::new(),
            temporary_expirations: None,
        }
    }
}

/// Renderer-dependent member projection is prepared before entering the serial
/// source dispatcher. The dispatcher either installs this exact result or fails.
pub(super) struct Prepared {
    payload: source::Payload,
    member: Option<Member>,
}
struct Member {
    rows: Value,
    before: Value,
    observed: directory_members::Observations,
    store: directory_members::Store,
    native: std::collections::HashMap<String, super_platinum_core::slack::models::User>,
}
pub(super) async fn prepare(
    state: Signal<ShellState>,
    owner: &Owner,
    kind: source::PayloadKind,
    raw: &Value,
    bytes: String,
) -> Result<Prepared, Error> {
    let ctx = &owner.context;
    let source_owner = source::Owner::capture(
        &ctx.account,
        &ctx.session,
        ctx.transport.d_cookie(),
        ctx.epoch,
    );
    let payload =
        source::Payload::observed(kind, source_owner, bytes).map_err(|_| unavailable())?;
    if kind != source::PayloadKind::Init {
        return Ok(Prepared {
            payload,
            member: None,
        });
    }
    let rows = serde_json::json!([raw.get("self").ok_or_else(unavailable)?]);
    let (before, observed, mut store, native, flags) = {
        let s = state.peek();
        if !owner.current(&s) {
            return Err(unavailable());
        }
        let w = &s.core.workspaces[&ctx.session.team_id];
        if w.directory_members
            .has_pending(&ctx.account, &ctx.session)?
        {
            return Err(unavailable());
        }
        (
            serde_json::to_value(&w.users).map_err(|_| unavailable())?,
            w.directory_members
                .observations(&ctx.account, &ctx.session, ctx.epoch, GENERATION)?,
            w.directory_members.clone(),
            w.users.clone(),
            s.app_home.channel_fetches.slots[&ctx.key()]
                .transformations
                .flags
                .clone(),
        )
    };
    store.enqueue(
        &ctx.account,
        &ctx.session,
        directory_members::Origin::DirectUpsert,
        rows.as_array().unwrap().clone(),
        serde_json::json!({}),
    )?;
    let pending = store.prepare(
        &ctx.account,
        &ctx.session,
        ctx.epoch,
        GENERATION,
        flags.clone(),
        &native,
    )?;
    let lowered =
        render::fields_guarded(state, |s| owner.current(s), GENERATION, pending.fields()).await?;
    let reply =
        super_platinum_core::block_actions::workflow_management::member_hydration::RendererReply {
            generation: GENERATION.into(),
            operation: "ordinary-toLowerCase".into(),
            fields: pending.fields().to_vec(),
            lowered,
        };
    let (_, native) = store
        .adopt(
            pending,
            reply,
            &flags,
            &ctx.account,
            &ctx.session,
            ctx.epoch,
            GENERATION,
            &native,
        )?
        .into_parts();
    Ok(Prepared {
        payload,
        member: Some(Member {
            rows,
            before,
            observed,
            store,
            native,
        }),
    })
}
pub(super) fn dispatch(
    shell: &mut ShellState,
    owner: &Owner,
    prepared: Prepared,
) -> Result<(), Error> {
    let Prepared { payload, member } = prepared;
    let mut sink = Sink {
        shell,
        owner,
        member,
    };
    let settlement = payload.dispatch(&mut sink);
    let success = settlement.result().is_ok();
    let slot = sink
        .shell
        .app_home
        .channel_fetches
        .slots
        .get_mut(&owner.context.key())
        .ok_or_else(unavailable)?;
    slot.payload.settlements.push(settlement);
    if slot.payload.settlements.len() > 32 {
        slot.payload.settlements.remove(0);
    }
    success.then_some(()).ok_or_else(unavailable)
}
struct Sink<'a> {
    shell: &'a mut ShellState,
    owner: &'a Owner,
    member: Option<Member>,
}
impl source::Adopter for Sink<'_> {
    fn current(&self, owner: &source::Owner) -> bool {
        let ctx = &self.owner.context;
        self.owner.current(self.shell)
            && owner.matches(
                &ctx.account,
                &ctx.session,
                ctx.transport.d_cookie(),
                ctx.epoch,
            )
    }
    fn facts(&self) -> source::Facts {
        let ctx = &self.owner.context;
        let slot = &self.shell.app_home.channel_fetches.slots[&ctx.key()];
        source::Facts {
            team: Some(ctx.session.team_id.clone()),
            enterprise: ctx.session.enterprise_id.clone(),
            workspace_ids: slot.payload.workspace_ids.clone(),
            auth_workspace_ids: self
                .shell
                .core
                .session
                .as_ref()
                .map(|s| s.workspaces.keys().cloned().collect()),
            temp_mute_enabled: Some(slot.transformations.temp_mute),
            unix_seconds: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|d| d.as_secs_f64()),
            screen_reader_enabled: Some(slot.payload.screen_reader.source_value()),
        }
    }
    fn apply(&mut self, effect: &source::Effect) -> Result<(), source::Error> {
        use source::Kind;
        let ctx = &self.owner.context;
        let w = self
            .shell
            .core
            .workspaces
            .get_mut(&ctx.session.team_id)
            .ok_or(source::Error::Superseded)?;
        match effect.kind {
            Kind::PlanInfo => return plan_info::apply(self.shell, self.owner, effect),
            Kind::SelfId => {
                if effect.payload["user_id"].as_str() != Some(&ctx.session.user_id) {
                    return Err(source::Error::Adoption);
                }
                w.self_user_id = ctx.session.user_id.clone();
            }
            Kind::ScreenReaderEnabled => {
                let value = effect
                    .payload
                    .as_bool()
                    .ok_or(source::Error::Malformed("screen reader state"))?;
                w.cfm_payload.apply(effect)?;
                // Updating the source reducer does not change input provenance.
                if value
                    != self.shell.app_home.channel_fetches.slots[&ctx.key()]
                        .payload
                        .screen_reader
                        .source_value()
                {
                    return Err(source::Error::Adoption);
                }
            }
            Kind::TeamUpsert => {
                let team = effect.payload["team"]
                    .as_object()
                    .ok_or(source::Error::Malformed("team"))?;
                let id = team
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or(source::Error::Malformed("team id"))?;
                if id == ctx.session.team_id {
                    if let Some(name) = team.get("name").and_then(Value::as_str) {
                        w.name = name.into();
                    }
                    if let Some(url) = team.get("url").and_then(Value::as_str) {
                        w.url = url.into();
                    }
                }
                let target = self
                    .shell
                    .app_home
                    .channel_fetches
                    .slots
                    .get_mut(&ctx.key())
                    .unwrap()
                    .payload
                    .teams
                    .entry(id.into())
                    .or_insert_with(|| serde_json::json!({}));
                target.as_object_mut().unwrap().extend(team.clone());
                let merged = target.clone();
                plan_info::team_upsert(self.shell, self.owner, &merged)?;
            }
            Kind::MemberUpsert => {
                let member = self
                    .member
                    .take()
                    .ok_or(source::Error::Unavailable("prepared member upsert"))?;
                if member.rows != effect.payload["members"]
                    || serde_json::to_value(&w.users).ok().as_ref() != Some(&member.before)
                    || !member.observed.current(
                        &w.directory_members,
                        &ctx.account,
                        &ctx.session,
                        ctx.epoch,
                        GENERATION,
                    )
                {
                    return Err(source::Error::Superseded);
                }
                w.directory_members = member.store;
                w.users = member.native;
            }
            Kind::UserPrefs => {
                w.cfm_payload.apply(effect)?;
                let raw = serde_json::to_value(&w.cfm_payload.user_prefs)
                    .map_err(|_| source::Error::Adoption)?;
                let prefs = serde_json::from_value(raw)
                    .map_err(|_| source::Error::Malformed("user preferences"))?;
                w.sidebar.apply_prefs(&prefs);
                w.notifications.apply_prefs(&prefs);
                w.read_preferences.apply_boot(&prefs);
                w.clear_unreads_preference.apply_boot(&prefs);
            }
            Kind::TeamPrefs | Kind::HiddenUsers | Kind::DefaultWorkspace => {
                w.cfm_payload.apply(effect)?;
            }
            Kind::MutedChannels => {
                w.cfm_payload.apply(effect)?;
                w.sidebar.muted = w.cfm_payload.muted_channels.keys().cloned().collect();
            }
            Kind::MemberPrimaryOwner
            | Kind::MemberOwner
            | Kind::MemberAdmin
            | Kind::MemberRoleClear
            | Kind::PrimaryOwner
            | Kind::Owner
            | Kind::Admin
            | Kind::AccountTypes
            | Kind::EnterprisePrimaryOwner
            | Kind::EnterpriseOwner
            | Kind::EnterpriseAdmin => {
                w.cfm_payload.apply(effect)?;
            }
            Kind::InternalWorkspaces => {
                w.cfm_payload.apply(effect)?;
                let ids = effect.payload["internalIds"]
                    .as_array()
                    .ok_or(source::Error::Malformed("internal workspace IDs"))?
                    .iter()
                    .map(|v| v.as_str().map(str::to_owned))
                    .collect::<Option<Vec<_>>>()
                    .ok_or(source::Error::Malformed("internal workspace ID"))?;
                self.shell
                    .app_home
                    .channel_fetches
                    .slots
                    .get_mut(&ctx.key())
                    .unwrap()
                    .payload
                    .workspace_ids = Some(ids);
            }
            Kind::ResetCommands => {
                crate::drafts::protected::production::commands::invalidate_catalogs(
                    &ctx.account,
                    &ctx.session.team_id,
                );
            }
            Kind::Subteams => {
                w.cfm_payload.apply(effect)?;
                let ids = effect.payload["userGroups"]
                    .as_array()
                    .ok_or(source::Error::Malformed("subteams array"))?
                    .iter()
                    .map(|id| id.as_str().map(str::to_owned))
                    .collect::<Option<Vec<_>>>()
                    .ok_or(source::Error::Malformed("subteam id"))?;
                let group_owner = crate::directory_member_fetches::Context::capture(
                    self.shell,
                    &ctx.session.team_id,
                )
                .ok()
                .map(Arc::new);
                let admission =
                    group_owner.map(|owner| {
                        crate::usergroups::source::enqueue(
                    self.shell, owner,
                    super_platinum_core::state::directory_usergroups::Operation::Membership { ids },
                )
                    });
                if let Some(crate::usergroups::source::Admission::Tracked(work)) = admission {
                    self.shell
                        .app_home
                        .channel_fetches
                        .slots
                        .get_mut(&ctx.key())
                        .unwrap()
                        .payload
                        .group_work
                        .push(work);
                } else {
                    let w = self
                        .shell
                        .core
                        .workspaces
                        .get_mut(&ctx.session.team_id)
                        .ok_or(source::Error::Adoption)?;
                    for group in w.usergroups.values_mut() {
                        group.users.retain(|id| id != &ctx.session.user_id);
                        if w.cfm_payload.user_group_membership.contains_key(&group.id) {
                            group.users.push(ctx.session.user_id.clone());
                        }
                    }
                }
            }
            _ => return auxiliary::apply(self.shell, self.owner, effect),
        }
        Ok(())
    }
}
