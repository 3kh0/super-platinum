//! Captured CFM, canonical DM and effective bot-member facts for the selector.
use super::*;
use super_platinum_core::state::directory_members::Subset;
const GENERATION: &str = "app-home-context-member";

pub(super) struct Observation {
    pub(super) cfm: channel_fetches::teams::HomeContextObservation,
    channel: Option<Arc<channel_fetches::canonical::Canonical>>,
    dm: Option<Value>,
    members: Option<Arc<Subset>>,
    pub(super) bot_teams: Option<Vec<String>>,
}
impl Observation {
    pub(super) fn capture(
        s: &ShellState,
        owner: &Owner,
        account: &SourceContext,
    ) -> Result<Self, Error> {
        let team = &account.session.team_id;
        let cfm = channel_fetches::teams::HomeContextObservation::capture(s, team)?;
        if cfm.internal_workspace_ids().is_empty() {
            return Ok(Self {
                cfm,
                channel: None,
                dm: None,
                members: None,
                bot_teams: None,
            });
        }
        let channel = channel_fetches::canonical::observed(s, team)?;
        let dm = channel.row(&owner.channel).ok_or_else(unavailable)?.clone();
        if dm["is_im"] != true {
            return Err(unavailable());
        }
        let user = dm["user"]
            .as_str()
            .filter(|v| !v.is_empty())
            .ok_or_else(unavailable)?;
        let w = s.core.workspaces.get(team).ok_or_else(unavailable)?;
        let members = w.directory_members.subset(
            &account.account,
            &account.session,
            account.epoch,
            GENERATION,
            &[user.to_owned()].into(),
            &w.users,
        )?;
        let member = members.rows().first().ok_or_else(unavailable)?;
        // Canonical source-effective fields select context; the native User
        // projection is an independently checked custody witness only.
        let effective = &member.source;
        let bot_teams = match effective.get("enterprise_user").filter(|v| !v.is_null()) {
            None => None,
            Some(v) if v.is_object() => match v.get("teams").filter(|v| !v.is_null()) {
                None => None,
                Some(Value::Array(ids)) => Some(
                    ids.iter()
                        .map(|v| v.as_str().map(str::to_owned).ok_or_else(unavailable))
                        .collect::<Result<Vec<_>, _>>()?,
                ),
                _ => return Err(unavailable()),
            },
            _ => return Err(unavailable()),
        };
        Ok(Self {
            cfm,
            channel: Some(channel),
            dm: Some(dm),
            members: Some(Arc::new(members)),
            bot_teams,
        })
    }
    pub(super) fn current(&self, s: &ShellState, account: &SourceContext) -> bool {
        self.cfm.current(s)
            && self.channel.as_ref().is_none_or(|v| v.current(s))
            && account.current(s)
            && self.members.as_ref().is_none_or(|members| {
                s.core
                    .workspaces
                    .get(&account.session.team_id)
                    .is_some_and(|w| {
                        members.current_rows(
                            &w.directory_members,
                            &account.account,
                            &account.session,
                            account.epoch,
                            GENERATION,
                            &w.users,
                        )
                    })
            })
    }
    pub(super) fn proof(&self) -> Value {
        json!({"cfm":self.cfm.provenance(),"dm":self.dm,
            "members":self.members.as_ref().map(|m|m.rows().iter().map(|r|json!({"source":r.source,"effective":r.native})).collect::<Vec<_>>()),
            "memberDependency":if self.members.is_some(){"observed"}else{"not-needed-empty-candidates"},"botTeams":self.bot_teams})
    }
}

pub(super) async fn ensure(
    mut state: Signal<ShellState>,
    owner: &Owner,
    still_current: impl Fn(&ShellState) -> bool,
) -> Result<(), Error> {
    let team = &owner.ctx.session.team_id;
    let account = SourceContext::capture(&state.peek(), team)?;
    if crate::fixture::is_fixture() {
        if !super::fixture::named()
            || !still_current(&state.peek())
            || !owner.current(&state.peek())
        {
            return Err(unavailable());
        }
        super::fixture::acquire_sources(state)
            .await
            .map_err(|_| unavailable())?;
        return (account.current(&state.peek())
            && still_current(&state.peek())
            && owner.current(&state.peek()))
        .then_some(())
        .ok_or_else(unavailable);
    }
    let current = |s: &ShellState| owner.current(s) && account.current(s) && still_current(s);
    if channel_fetches::teams::HomeContextObservation::capture(&state.peek(), team).is_err()
        || channel_fetches::canonical::observed(&state.peek(), team).is_err()
    {
        channel_fetches::resync(state, team.clone());
    }
    let wait = async {
        loop {
            if !current(&state.peek()) {
                return Err(unavailable());
            }
            if channel_fetches::teams::HomeContextObservation::capture(&state.peek(), team).is_ok()
                && channel_fetches::canonical::observed(&state.peek(), team).is_ok()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let mut canonical = channel_fetches::canonical::observed(&state.peek(), team)?;
        if canonical.row(&owner.channel).is_none() {
            let selected =
                channel_fetches::unknown::select(&mut state.write(), team, &owner.channel)?;
            selected.schedule(state);
            loop {
                if !current(&state.peek()) {
                    return Err(unavailable());
                }
                canonical = channel_fetches::canonical::observed(&state.peek(), team)?;
                if canonical.row(&owner.channel).is_some() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
        let user = canonical
            .row(&owner.channel)
            .and_then(|v| v["user"].as_str())
            .ok_or_else(unavailable)?
            .to_owned();
        members(state, team, &user, |s| current(s) && canonical.current(s)).await
    };
    tokio::time::timeout(Duration::from_secs(20), wait)
        .await
        .map_err(|_| unavailable())?
}

async fn members(
    mut state: Signal<ShellState>,
    team: &str,
    user: &str,
    current: impl Fn(&ShellState) -> bool,
) -> Result<(), Error> {
    use super_platinum_core::block_actions::{
        directory_endpoint::member_info::unknown::PlaceholderAsset,
        workflow_management::member_hydration::RendererReply,
    };
    let ctx = SourceContext::capture(&state.peek(), team)?;
    let available = |s: &ShellState| {
        s.core.workspaces.get(team).is_some_and(|w| {
            w.directory_members
                .subset(
                    &ctx.account,
                    &ctx.session,
                    ctx.epoch,
                    GENERATION,
                    &[user.to_owned()].into(),
                    &w.users,
                )
                .is_ok()
        })
    };
    if available(&state.peek()) {
        return Ok(());
    }
    let Some(mut client) =
        crate::directory_member_fetches::Client::begin(state, team, &[user.into()])?
    else {
        if !available(&state.peek()) {
            crate::directory_member_fetches::known::recover(state, team, &[user.into()], &current)
                .await?;
        }
        return (current(&state.peek()) && available(&state.peek()))
            .then_some(())
            .ok_or_else(unavailable);
    };
    while let Some(mut election) = client.next(&current).await? {
        let flags = channel_fetches::member_flags(&state.peek(), team)?;
        let asset = PlaceholderAsset::observed("https://a.slack-edge.com/bv1-13/")?;
        let mut stage = election.stage(
            &state.peek(),
            flags.flags().clone(),
            GENERATION.into(),
            &asset,
        )?;
        loop {
            let Some(pending) = stage.prepare(&state.peek(), &election)? else {
                break;
            };
            let fields = pending.fields().to_vec();
            let lowered = channel_fetches::member_fields(
                state,
                |s| current(s) && flags.current(s) && election.current(s),
                GENERATION,
                &fields,
            )
            .await?;
            let reply = RendererReply {
                generation: pending.generation().into(),
                operation: "ordinary-toLowerCase".into(),
                fields,
                lowered,
            };
            stage.apply(&state.peek(), &election, pending, reply)?;
        }
        stage.install(&mut state.write(), &mut election, |s| {
            current(s) && flags.current(s)
        })?;
    }
    (current(&state.peek()) && available(&state.peek()))
        .then_some(())
        .ok_or_else(unavailable)
}
