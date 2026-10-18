use super::*;
use std::collections::BTreeSet;
use super_platinum_core::{config::WorkspaceSession, state::directory_members::Subset};
mod groups;
use groups::Groups;

/// A bounded observation of actual member source rows. The opening token owns
/// account/transport/visit custody; the subset adds source and native witnesses.
struct Members {
    session: WorkspaceSession,
    epoch: u64,
    generation: String,
    subset: Subset,
}
impl Members {
    fn capture(
        shell: &ShellState,
        token: &Token,
        opening: u64,
        ids: &BTreeSet<String>,
    ) -> Result<Self, Error> {
        let session = shell
            .core
            .session
            .as_ref()
            .and_then(|s| s.workspaces.get(&token.owner.team))
            .filter(|s| s.team_id == token.owner.team && s.user_id == token.owner.user)
            .cloned()
            .ok_or_else(unavailable)?;
        let w = shell
            .core
            .workspaces
            .get(&token.owner.team)
            .ok_or_else(unavailable)?;
        let epoch = shell.core.account_epoch;
        let generation = format!("profile-user-mentions-{opening}");
        let subset = w.directory_members.subset(
            &token.owner.account,
            &session,
            epoch,
            &generation,
            ids,
            &w.users,
        )?;
        Ok(Self {
            session,
            epoch,
            generation,
            subset,
        })
    }
    fn current(&self, shell: &ShellState, token: &Token) -> bool {
        shell
            .core
            .workspaces
            .get(&token.owner.team)
            .is_some_and(|w| {
                self.subset.current(
                    &w.directory_members,
                    &token.owner.account,
                    &self.session,
                    self.epoch,
                    &self.generation,
                    &w.users,
                )
            })
    }
    fn label(&self, token: &Value) -> Result<String, Error> {
        let id = token["user_id"].as_str().ok_or_else(unavailable)?;
        let row = self
            .subset
            .rows()
            .iter()
            .find(|r| r.source["id"].as_str() == Some(id))
            .ok_or_else(unavailable)?;
        member_label(&row.source)
    }
}
fn member_label(row: &Value) -> Result<String, Error> {
    for key in ["isUnknown", "isNonExistent"] {
        if row
            .get(key)
            .is_some_and(|v| !matches!(v, Value::Null | Value::Bool(false)))
        {
            return Err(unavailable());
        }
    }
    // hWCs.Gt/E1L/: preserve whitespace and Unicode exactly. Username is not a
    // fallback, and a known member with no display/real name is the label "@".
    for value in [
        &row["profile"]["display_name"],
        &row["profile"]["real_name"],
        &row["real_name"],
    ] {
        match value {
            Value::Null => {}
            Value::String(name) if name.is_empty() => {}
            Value::String(name) => return Ok(format!("@{name}")),
            _ => return Err(unavailable()),
        }
    }
    Ok("@".into())
}
/// Explicit Retry only: acquire unavailable known references, never all members
/// or healthy names merely because another mention prevented this opening.
pub(super) fn known_recovery_ids(
    shell: &ShellState,
    token: &Token,
    opening: u64,
    requested: &[String],
) -> Result<Vec<String>, Error> {
    if !token.current(shell) {
        return Err(unavailable());
    }
    let session = shell
        .core
        .session
        .as_ref()
        .and_then(|s| s.workspaces.get(&token.owner.team))
        .ok_or_else(unavailable)?;
    let w = shell
        .core
        .workspaces
        .get(&token.owner.team)
        .ok_or_else(unavailable)?;
    let observations = w.directory_members.observations(
        &token.owner.account,
        session,
        shell.core.account_epoch,
        &format!("profile-known-mentions-{opening}"),
    )?;
    let known = observations.source_ids().collect::<BTreeSet<_>>();
    let required = w.directory_members.required_observations(&w.users);
    let mut needed: BTreeSet<String> = requested
        .iter()
        .filter(|id| known.contains(id.as_str()) && required.contains(*id))
        .cloned()
        .collect();
    let ready: BTreeSet<String> = requested
        .iter()
        .filter(|id| known.contains(id.as_str()) && !needed.contains(*id))
        .cloned()
        .collect();
    if !ready.is_empty() {
        let members = Members::capture(shell, token, opening, &ready)?;
        for row in members.subset.rows() {
            if member_label(&row.source).is_err() {
                needed.insert(row.source["id"].as_str().ok_or_else(unavailable)?.into());
            }
        }
    }
    Ok(requested
        .iter()
        .filter(|id| needed.contains(*id))
        .cloned()
        .collect())
}
pub(super) enum Hit {
    Channel {
        source: Arc<canonical::Canonical>,
        id: String,
    },
    Member(crate::directory_member_fetches::automatic::Hit),
}
impl Hit {
    pub(super) fn schedule(self, state: Signal<ShellState>) {
        // Scheduling is the selector's side effect, not new authority for the
        // returned row or a reason to erase an already admitted description.
        match self {
            Self::Channel { source, id } => {
                let _ = canonical::hit(state, &source, &id);
            }
            Self::Member(hit) => {
                hit.schedule(state);
            }
        }
    }
    pub(super) fn is_member(&self) -> bool {
        matches!(self, Self::Member(_))
    }
}
pub(crate) struct Opening {
    token: Token,
    visit: u64,
    serial: u64,
    opening: u64,
    session: Session,
    source: Option<Arc<canonical::Canonical>>,
    members: Option<Members>,
    groups: Option<Groups>,
}
pub(crate) fn begin(
    shell: &mut ShellState,
    visit: u64,
    serial: u64,
    id: &str,
    key: u64,
) -> Result<Opening, Error> {
    tracked(shell, visit, serial, id, key).0
}
pub(super) fn tracked(
    shell: &mut ShellState,
    visit: u64,
    serial: u64,
    id: &str,
    key: u64,
) -> (Result<Opening, Error>, Vec<Hit>) {
    tracked_with_members(shell, visit, serial, id, key, true)
}
pub(super) fn tracked_with_members(
    shell: &mut ShellState,
    visit: u64,
    serial: u64,
    id: &str,
    key: u64,
    automatic_members: bool,
) -> (Result<Opening, Error>, Vec<Hit>) {
    let mut hits = Vec::new();
    let result = prepare(shell, visit, serial, id, key, automatic_members, &mut hits);
    (result, hits)
}
fn prepare(
    shell: &mut ShellState,
    visit: u64,
    serial: u64,
    id: &str,
    key: u64,
    automatic_members: bool,
    hits: &mut Vec<Hit>,
) -> Result<Opening, Error> {
    if !editable(shell, visit, serial) {
        return Err(unavailable());
    }
    let opening = next_id();
    let d = shell.profile_edit.sections.draft.as_mut().unwrap();
    retire_recovery(d);
    d.rich_opening = opening;
    d.rich = None;
    d.rich_reading = false;
    let token = d.token.clone();
    let Some(SectionValue::LongText(blocks)) = d.form.as_ref().and_then(|f| f.value(id)) else {
        return Err(unavailable());
    };
    let before = blocks.clone();
    let tokens = collect(&before)?;
    if automatic_members {
        // Observe every USER occurrence before checked label/subset acquisition.
        // A stale second member or another unavailable token must not strand an
        // otherwise valid selector hit. The shared manager coalesces duplicate
        // IDs; source failure never grants a label or aborts later observations.
        for raw in tokens.iter().filter(|raw| raw["type"] == "user") {
            if let Some(id) = raw["user_id"].as_str().filter(|id| !id.is_empty()) {
                if let Ok(hit) = crate::directory_member_fetches::automatic::observe(
                    shell,
                    &token.owner.team,
                    id,
                ) {
                    hits.push(Hit::Member(hit));
                }
            }
        }
    }
    let source = if tokens.iter().any(|v| v["type"] == "channel") {
        Some(canonical::capture(shell, &token.owner.team)?)
    } else {
        None
    };
    let mut members = None;
    let group_ids = tokens
        .iter()
        .filter(|v| v["type"] == "usergroup")
        .map(|v| {
            v["usergroup_id"]
                .as_str()
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
                .ok_or_else(unavailable)
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    let groups = if group_ids.is_empty() {
        None
    } else {
        Some(Groups::capture(shell, &token, &group_ids)?)
    };
    // Preserve the existing renderer's labels for every other token kind.
    let mut labels: serde_json::Map<String, Value> = shell
        .core
        .workspaces
        .get(&token.owner.team)
        .and_then(|w| {
            serde_json::from_str(&crate::view::rich_editor::mention_labels(w, &before)).ok()
        })
        .unwrap_or_default();
    let mut catalog = catalog::Catalog::default();
    for raw in tokens
        .iter()
        .copied()
        .filter(|v| matches!(v["type"].as_str(), Some("channel" | "user" | "usergroup")))
    {
        if raw["type"] == "usergroup" {
            let label = groups.as_ref().ok_or_else(unavailable)?.label(raw)?;
            // Exact token keys also preserve an intentionally empty group
            // label; the generic editor's ID fallback uses truthy strings.
            labels.insert(raw.to_string(), Value::String(label.clone()));
            catalog.insert(raw, label)?;
            continue;
        }
        if raw["type"] == "user" {
            if members.is_none() {
                let ids = tokens
                    .iter()
                    .filter(|v| v["type"] == "user")
                    .map(|v| {
                        v["user_id"]
                            .as_str()
                            .filter(|id| !id.is_empty())
                            .map(str::to_owned)
                            .ok_or_else(unavailable)
                    })
                    .collect::<Result<BTreeSet<_>, _>>()?;
                members = Some(Members::capture(shell, &token, opening, &ids)?);
            }
            let label = members.as_ref().ok_or_else(unavailable)?.label(raw)?;
            labels.insert(
                raw["user_id"].as_str().ok_or_else(unavailable)?.into(),
                Value::String(label.clone()),
            );
            catalog.insert(raw, label)?;
            continue;
        }
        let id = raw["channel_id"].as_str().ok_or_else(unavailable)?;
        let row = source
            .as_ref()
            .and_then(|s| s.row(id))
            .ok_or_else(unavailable)?;
        let label = canonical::ordinary_label(raw, row)?;
        // Retain every actual ordinary selector hit, including an earlier hit
        // when a later token prevents this description from being opened.
        hits.push(Hit::Channel {
            source: source.as_ref().unwrap().clone(),
            id: id.to_owned(),
        });
        labels.insert(id.into(), Value::String(label.clone()));
        catalog.insert(raw, label)?;
    }
    Ok(Opening {
        token,
        visit,
        serial,
        opening,
        source,
        members,
        groups,
        session: Session {
            id: id.into(),
            key,
            before,
            catalog,
            labels: Value::Object(labels).to_string(),
            notice: None,
            query: None,
        },
    })
}
pub(crate) fn finish(shell: &mut ShellState, opening: Opening) -> bool {
    if !editable(shell, opening.visit, opening.serial)
        || !opening.token.current(shell)
        || opening.source.as_ref().is_some_and(|s| !s.current(shell))
        || opening
            .members
            .as_ref()
            .is_some_and(|m| !m.current(shell, &opening.token))
        || opening.groups.as_ref().is_some_and(|g| !g.current(shell))
    {
        return false;
    }
    let d = shell.profile_edit.sections.draft.as_mut().unwrap();
    if d.rich_opening != opening.opening {
        return false;
    }
    if d.form.as_ref().and_then(|f| f.value(&opening.session.id))
        != Some(&SectionValue::LongText(opening.session.before.clone()))
    {
        return false;
    }
    d.rich = Some(opening.session);
    true
}
