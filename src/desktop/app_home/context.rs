//! One source-owned route for Home suggestions, explicit actions and genuine opens.
//! Choosing a context changes no form, native-control or Home-view identity.
use super::*;
use crate::directory_member_fetches::Context as SourceContext;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use super_platinum_core::block_actions::home::context as selector;

mod source;
mod view;
pub(crate) use view::ContextChooser;
pub(crate) mod fixture;
#[cfg(test)]
mod tests;

#[derive(Default)]
pub(super) struct State {
    overrides: BTreeMap<(String, String, String), Override>,
    fixture_opens: Vec<Value>,
    fixture_action: Option<fixture::ActionGate>,
    fixture_actions: Vec<Value>,
    fixture_source_ready: bool,
    fixture_source_reads: Vec<Value>,
}
struct Override {
    account: Arc<SourceContext>,
    team: String,
    revision: u64,
}

#[derive(Clone)]
pub(crate) struct Witness {
    owner: Owner,
    account: Arc<SourceContext>,
    mode: bool,
    selection: selector::Selection,
    override_revision: Option<u64>,
    source: Option<Arc<source::Observation>>,
}
impl Witness {
    pub(crate) fn capture(s: &ShellState) -> Result<Self, Error> {
        let owner = &s.app_home.surface.as_ref().ok_or_else(unavailable)?.owner;
        Self::for_owner(s, owner)
    }
    fn for_owner(s: &ShellState, owner: &Owner) -> Result<Self, Error> {
        if !owner.current(s) {
            return Err(unavailable());
        }
        let account = Arc::new(SourceContext::capture(s, &owner.ctx.session.team_id)?);
        let mode = configuration(s, &owner.ctx.session.team_id)?;
        let selected = override_for(s, &account, &owner.channel);
        let source = if mode && selected.is_none() {
            Some(Arc::new(source::Observation::capture(s, owner, &account)?))
        } else {
            None
        };
        let selection = selector::select(&selector::Inputs {
            unified: Some(mode),
            channel_override: selected.map(|v| v.team.as_str()),
            internal_workspace_ids: source.as_ref().map(|v| v.cfm.internal_workspace_ids()),
            bot_enterprise_teams: source.as_ref().and_then(|v| v.bot_teams.as_deref()),
            active_team_id: s.core.active_team.as_deref(),
            workspace_filter_selected: source
                .as_ref()
                .and_then(|v| v.cfm.workspace_filter_selected()),
            default_workspace_id: source
                .as_ref()
                .and_then(|v| v.cfm.default_workspace())
                .and_then(Value::as_str),
        })?;
        Ok(Self {
            owner: owner.clone(),
            account,
            mode,
            selection,
            override_revision: selected.map(|v| v.revision),
            source,
        })
    }
    pub(crate) fn team_id(&self) -> Option<&str> {
        self.selection.team_id()
    }
    pub(crate) fn current(&self, s: &ShellState) -> bool {
        self.owner.current(s)
            && self.account.current(s)
            && configuration(s, &self.owner.ctx.session.team_id).ok() == Some(self.mode)
            && override_for(s, &self.account, &self.owner.channel).map(|v| v.revision)
                == self.override_revision
            && self
                .source
                .as_ref()
                .is_none_or(|v| v.current(s, &self.account))
    }
    pub(crate) fn same_route(&self, other: &Self) -> bool {
        self.mode == other.mode
            && self.selection == other.selection
            && self.override_revision == other.override_revision
            && self.owner.home_visit == other.owner.home_visit
            && self.owner.rt == other.owner.rt
    }
    pub(crate) fn proof(&self, s: &ShellState) -> Value {
        json!({"current":self.current(s),"unified":self.mode,"team":self.team_id(),
            "overrideRevision":self.override_revision,"channel":self.owner.channel,
            "socket":self.owner.rt,"source":self.source.as_ref().map(|v|v.proof())})
    }
}
fn configuration(s: &ShellState, team: &str) -> Result<bool, Error> {
    super_platinum_core::state::posting_metadata::Configuration::from_session(
        s.core.session.as_ref().ok_or_else(unavailable)?,
        team,
    )
    .map_err(|_| unavailable())?
    .unified()
    .ok_or_else(unavailable)
}
fn key(account: &SourceContext, channel: &str) -> (String, String, String) {
    (
        account.account.clone(),
        account.session.team_id.clone(),
        channel.into(),
    )
}
fn override_for<'a>(
    s: &'a ShellState,
    account: &SourceContext,
    channel: &str,
) -> Option<&'a Override> {
    s.app_home
        .context
        .overrides
        .get(&key(account, channel))
        .filter(|v| v.account.account_current(s))
}
fn unavailable() -> Error {
    Error::Api("app_home_context_unavailable".into())
}

#[derive(Clone)]
struct Choice {
    witness: Witness,
    source: Arc<source::Observation>,
    rows: Vec<(String, String)>,
}
impl Choice {
    fn capture(s: &ShellState) -> Result<Option<Self>, Error> {
        let witness = Witness::capture(s)?;
        if !witness.mode {
            return Ok(None);
        }
        let source = match &witness.source {
            Some(v) => v.clone(),
            None => Arc::new(source::Observation::capture(
                s,
                &witness.owner,
                &witness.account,
            )?),
        };
        let ids = selector::candidates(
            source.cfm.internal_workspace_ids(),
            source.bot_teams.as_deref(),
        )?;
        let rows = ids
            .into_iter()
            .map(|id| {
                let name = source
                    .cfm
                    .rows()
                    .get(&id)
                    .and_then(|v| v.get("name"))
                    .and_then(Value::as_str)
                    .filter(|v| !v.is_empty())
                    .ok_or_else(unavailable)?;
                Ok((id, name.to_owned()))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        Ok(Some(Self {
            witness,
            source,
            rows,
        }))
    }
    fn current(&self, s: &ShellState) -> bool {
        self.witness.current(s) && self.source.current(s, &self.witness.account)
    }
}
/// A retained explicit route may no longer belong to the current menu. Display
/// its observed name without making it a selectable candidate or changing route.
fn outside_option(
    selected: Option<&str>,
    rows: &[(String, String)],
    names: &BTreeMap<String, Value>,
) -> Option<(String, String)> {
    let selected = selected?;
    if rows.iter().any(|(id, _)| id == selected) {
        return None;
    }
    let name = names
        .get(selected)
        .and_then(|v| v.get("name"))
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .unwrap_or("Workspace name unavailable");
    Some((selected.to_owned(), name.to_owned()))
}

fn choose(s: &mut ShellState, choice: &Choice, team: &str) -> bool {
    if !choice.current(s)
        || !choice.rows.iter().any(|(id, _)| id == team)
        || s.overlay.is_some()
        || workflows::locked(s)
        || s.app_home.surface.as_ref().is_none_or(|h| {
            h.loading || h.confirmation.is_some() || h.calendar_confirmation.is_some()
        })
    {
        return false;
    }
    let mut retained = std::mem::take(&mut s.app_home.context.overrides);
    retained.retain(|_, v| v.account.account_current(s));
    let k = key(&choice.witness.account, &choice.witness.owner.channel);
    if !retained.contains_key(&k) && retained.len() >= 128 {
        s.app_home.context.overrides = retained;
        return false;
    }
    retained.insert(
        k,
        Override {
            account: choice.witness.account.clone(),
            team: team.into(),
            revision: next(),
        },
    );
    s.app_home.context.overrides = retained;
    true
}

/// A genuine Home opening may wait for existing source producers. This is a
/// bounded visit-owned wait, never a new acquisition manager or periodic task.
pub(super) async fn ensure(state: Signal<ShellState>, owner: &Owner) -> Result<Witness, Error> {
    if let Ok(witness) = Witness::for_owner(&state.peek(), owner) {
        return Ok(witness);
    }
    if !owner.current(&state.peek()) || !configuration(&state.peek(), &owner.ctx.session.team_id)? {
        return Err(unavailable());
    }
    let account = SourceContext::capture(&state.peek(), &owner.ctx.session.team_id)?;
    let revision = override_for(&state.peek(), &account, &owner.channel).map(|v| v.revision);
    source::ensure(state, owner, |s| {
        account.current(s)
            && override_for(s, &account, &owner.channel).map(|v| v.revision) == revision
    })
    .await?;
    let s = state.peek();
    if !account.current(&s)
        || override_for(&s, &account, &owner.channel).map(|v| v.revision) != revision
    {
        return Err(unavailable());
    }
    Witness::for_owner(&s, owner)
}
