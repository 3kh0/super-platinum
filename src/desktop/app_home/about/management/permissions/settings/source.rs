use super::*;
use crate::app_home::channel_fetches::{canonical, teams};
use super_platinum_core::state::directory_members::Ready;

pub(super) struct Source {
    account: String,
    teams: teams::Observation,
    members: Option<Ready>,
    channels: Option<Arc<canonical::Canonical>>,
    native_users: Value,
    native_channels: Value,
    invalidated: std::collections::BTreeSet<String>,
}
impl Source {
    pub(super) fn capture(s: &ShellState, o: &Original) -> Option<Self> {
        let ctx = &o.owner.ctx;
        let account = s.core.active_account.clone()?;
        let w = s.core.workspaces.get(&ctx.session.team_id)?;
        Some(Self {
            teams: teams::Observation::capture(s, &ctx.session.team_id).ok()?,
            members: w
                .directory_members
                .ready(
                    &account,
                    &ctx.session,
                    ctx.epoch,
                    "workflow-run-settings",
                    &w.users,
                )
                .ok(),
            channels: canonical::observed(s, &ctx.session.team_id).ok(),
            native_users: serde_json::to_value(&w.users).ok()?,
            native_channels: serde_json::to_value(&w.channels).ok()?,
            invalidated: w.directory_members.invalidated(),
            account,
        })
    }
    pub(super) fn current(&self, s: &ShellState, o: &Original) -> bool {
        let ctx = &o.owner.ctx;
        s.core.active_account.as_ref() == Some(&self.account)
            && ctx.current(s)
            && self.teams.current(s)
            && s.core
                .workspaces
                .get(&ctx.session.team_id)
                .is_some_and(|w| {
                    self.members.as_ref().is_none_or(|r| {
                        r.current(
                            &w.directory_members,
                            &self.account,
                            &ctx.session,
                            ctx.epoch,
                            "workflow-run-settings",
                            &w.users,
                        )
                    }) && self.channels.as_ref().is_none_or(|r| r.current(s))
                        && serde_json::to_value(&w.users).ok().as_ref() == Some(&self.native_users)
                        && serde_json::to_value(&w.channels).ok().as_ref()
                            == Some(&self.native_channels)
                        && w.directory_members.invalidated() == self.invalidated
                })
    }
    pub(super) fn context(
        &self,
        s: &ShellState,
        o: &Original,
    ) -> Option<(Context, BTreeMap<String, Row>)> {
        let ws = &o.owner.ctx.session;
        let own = self.teams.rows().get(&ws.team_id);
        let enterprise = ws.enterprise_id.clone().or_else(|| {
            own.and_then(|v| v["enterprise_id"].as_str())
                .map(str::to_owned)
        });
        let scope = enterprise.clone().unwrap_or_else(|| ws.team_id.clone());
        let unified = s
            .core
            .session
            .as_ref()?
            .unified_user_client_enabled
            .get(&ws.team_id)
            .copied()
            .unwrap_or(false);
        let mut ctx = Context {
            current_team_id: ws.team_id.clone(),
            scope_id: scope,
            unified_user: unified,
            entities: vec![],
            collaborators: o.editor.raw_workflow()["collaborators"]
                .as_array()?
                .iter()
                .map(|v| v.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>()?,
            available_external_ids: None,
        };
        let mut rows = BTreeMap::new();
        // These are authenticated scope facts, not a fabricated selectable-directory row.
        ctx.entities.push(Entity {
            id: ws.team_id.clone(),
            kind: if ws.team_id.starts_with('E') {
                Kind::Org
            } else {
                Kind::Team
            },
            enterprise_id: enterprise,
            eligible: true,
        });
        for raw in self
            .teams
            .rows()
            .values()
            .chain(
                self.members
                    .iter()
                    .flat_map(|r| r.rows())
                    .map(|r| &r.source),
            )
            .chain(self.channels.iter().flat_map(|r| r.rows()))
        {
            if let Some((entity, row)) = row(raw, &ctx) {
                ctx.entities.retain(|e| e.id != entity.id);
                ctx.entities.push(entity);
                rows.insert(row.id.clone(), row);
            }
        }
        ctx.validate().ok()?;
        Some((ctx, rows))
    }
}
pub(in crate::app_home::about::management::permissions) fn row(
    raw: &Value,
    ctx: &Context,
) -> Option<(Entity, Row)> {
    let id = raw["id"].as_str()?;
    let kind = match id.as_bytes().first()? {
        b'U' | b'W' => Kind::User,
        b'C' | b'G' | b'D' | b'Z' => Kind::Channel,
        b'T' => Kind::Team,
        b'E' => Kind::Org,
        _ => return None,
    };
    let enterprise_id = raw["enterprise_id"].as_str().map(str::to_owned);
    let external = matches!(kind, Kind::Team | Kind::Org)
        && id != ctx.scope_id
        && (if ctx.unified_user && kind == Kind::Org {
            Some(id)
        } else {
            enterprise_id.as_deref()
        }) != Some(ctx.scope_id.as_str());
    let disabled = |key: &str| raw[key] == true;
    if ["isUnknown", "isNonExistent", "is_deleted"]
        .iter()
        .any(|k| disabled(k))
    {
        return None;
    }
    let mut eligible = !disabled("deleted");
    let label = match kind {
        Kind::User => {
            if disabled("is_bot")
                || disabled("is_restricted")
                || disabled("is_ultra_restricted")
                || disabled("is_stranger")
                || id == "USLACKBOT"
            {
                eligible = false;
            }
            let local = raw["team_id"]
                .as_str()
                .or_else(|| raw["team"].as_str())
                .or_else(|| raw.pointer("/profile/team").and_then(Value::as_str));
            let member_of = raw
                .pointer("/enterprise_user/teams")
                .and_then(Value::as_array)
                .is_some_and(|ids| ids.iter().any(|v| v == &ctx.current_team_id));
            if local != Some(ctx.current_team_id.as_str())
                && local != Some(ctx.scope_id.as_str())
                && !member_of
            {
                eligible = false;
            }
            raw.pointer("/profile/display_name")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    raw.pointer("/profile/real_name")
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty())
                })
                .or_else(|| raw["real_name"].as_str().filter(|s| !s.is_empty()))
                .or_else(|| raw["name"].as_str().filter(|s| !s.is_empty()))?
                .to_owned()
        }
        Kind::Channel => {
            if disabled("is_archived")
                || disabled("is_im")
                || disabled("is_mpim")
                || raw["is_member"] != true
            {
                eligible = false;
            }
            raw["name"].as_str().filter(|s| !s.is_empty())?.to_owned()
        }
        Kind::Team | Kind::Org => raw["name"].as_str().filter(|s| !s.is_empty())?.to_owned(),
    };
    Some((
        Entity {
            id: id.into(),
            kind,
            enterprise_id,
            eligible,
        },
        Row {
            id: id.into(),
            kind,
            label,
            raw: raw.clone(),
            external,
        },
    ))
}
