//! FOIV/5qpr run-access draft. Canonical facts and their currentness belong to
//! the caller; lexical ID classification alone never grants picker authority.
use super::{Command, RequestPlan, Value};
use std::collections::BTreeSet;
mod policy;
pub use policy::TargetPolicy;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Everyone,
    /// The UI says workflow managers; the wire value is app_collaborators.
    AppCollaborators,
    NamedEntities,
}
impl Mode {
    pub fn parse(value: &str) -> Result<Self, Error> {
        match value {
            "everyone" => Ok(Self::Everyone),
            "app_collaborators" => Ok(Self::AppCollaborators),
            "named_entities" => Ok(Self::NamedEntities),
            _ => Err(Error::Invalid),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Everyone => "everyone",
            Self::AppCollaborators => "app_collaborators",
            Self::NamedEntities => "named_entities",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    User,
    Channel,
    Team,
    Org,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid,
    MissingEntity,
    Ineligible,
    Unresolved,
    TooMany,
    EmptyNamed,
    MissingAvailable,
    MissingPolicy,
    Restricted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entity {
    pub id: String,
    pub kind: Kind,
    pub enterprise_id: Option<String>,
    /// Actual settings-preset eligibility from the canonical source producer,
    /// including local member / bot / guest and member-channel / DM / MPIM rules.
    pub eligible: bool,
}
fn kind(id: &str) -> Result<Kind, Error> {
    let value = id.strip_prefix('@').unwrap_or(id);
    let bytes = value.as_bytes();
    if bytes.len() < 9
        || !bytes[1..]
            .iter()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
    {
        return Err(Error::Invalid);
    }
    match bytes[0] {
        b'U' | b'W' => Ok(Kind::User),
        b'C' | b'G' | b'D' | b'Z' if !id.starts_with('@') => Ok(Kind::Channel),
        b'T' if !id.starts_with('@') => Ok(Kind::Team),
        b'E' if !id.starts_with('@') => Ok(Kind::Org),
        _ => Err(Error::Invalid),
    }
}

/// Supplied from original owned canonical observations, never credential lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Context {
    pub current_team_id: String,
    pub scope_id: String,
    pub unified_user: bool,
    pub entities: Vec<Entity>,
    pub collaborators: Vec<String>,
    /// OUCs available external list; None is unobserved, Some([]) observed empty.
    pub available_external_ids: Option<Vec<String>>,
}
impl Context {
    pub fn validate(&self) -> Result<(), Error> {
        if !matches!(kind(&self.current_team_id)?, Kind::Team | Kind::Org)
            || !matches!(kind(&self.scope_id)?, Kind::Team | Kind::Org)
        {
            return Err(Error::Invalid);
        }
        let mut ids = BTreeSet::new();
        for e in &self.entities {
            if kind(&e.id)? != e.kind
                || !ids.insert(&e.id)
                || e.enterprise_id
                    .as_ref()
                    .is_some_and(|id| kind(id) != Ok(Kind::Org))
            {
                return Err(Error::Invalid);
            }
        }
        self.entity(&self.current_team_id)?;
        Ok(())
    }
    pub fn entity(&self, id: &str) -> Result<&Entity, Error> {
        self.entities
            .iter()
            .find(|e| e.id == id)
            .ok_or(Error::MissingEntity)
    }
    pub fn is_external(&self, id: &str) -> Result<bool, Error> {
        if !matches!(kind(id)?, Kind::Team | Kind::Org) {
            return Ok(false);
        }
        let e = self.entity(id)?;
        let enterprise = if self.unified_user && e.kind == Kind::Org {
            Some(e.id.as_str())
        } else {
            e.enterprise_id.as_deref()
        };
        Ok(id != self.scope_id && enterprise != Some(self.scope_id.as_str()))
    }
    fn partition(&self, ids: &[String], external: bool) -> Result<Vec<String>, Error> {
        ids.iter()
            .filter_map(|id| match self.is_external(id) {
                Ok(value) if value == external => Some(Ok(id.clone())),
                Ok(_) => None,
                Err(error) => Some(Err(error)),
            })
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selection {
    Resolved(String),
    Unresolved(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Draft {
    original_mode: Mode,
    original: [Vec<String>; 4],
    mode: Mode,
    ids: Vec<String>,
    unresolved: Vec<String>,
    external_enabled: bool,
}
fn strings(raw: &Value, key: &str) -> Result<Vec<String>, Error> {
    match raw.get(key) {
        None => Ok(vec![]),
        Some(Value::Array(values)) => values
            .iter()
            .map(|v| {
                v.as_str()
                    .filter(|v| !v.is_empty())
                    .map(str::to_owned)
                    .ok_or(Error::Invalid)
            })
            .collect(),
        _ => Err(Error::Invalid),
    }
}
fn update(original: &[String], previous: &[String], next: &[String]) -> Vec<String> {
    let mut union = Vec::new();
    for id in original.iter().chain(previous).chain(next) {
        if !union.contains(id) {
            union.push(id.clone());
        }
    }
    union.retain(|id| !previous.contains(id) || next.contains(id));
    union
}
fn optional(values: Vec<String>) -> Option<Vec<String>> {
    (!values.is_empty()).then_some(values)
}
fn same_set(a: &[String], b: &[String]) -> bool {
    a.iter().all(|id| b.contains(id)) && b.iter().all(|id| a.contains(id))
}
/// A successful response must acknowledge the actual effective grant, including
/// categories omitted from the request. Unknown response fields remain retained
/// in the caller's original receipt; they do not grant additional targets.
pub fn acknowledges(plan: &RequestPlan, raw: &Value) -> bool {
    let args = plan.arguments();
    if plan.method() != "workflows.triggers.permissions.set"
        || plan.reason() != "set-trigger-run-permission"
        || raw.get("ok") != Some(&Value::Bool(true))
        || raw.get("error").is_some_and(|v| !v.is_null())
        || raw.get("permission_type") != args.get("permission_type")
        || args["permission_type"]
            .as_str()
            .and_then(|v| Mode::parse(v).ok())
            .is_none()
    {
        return false;
    }
    ["user_ids", "channel_ids", "team_ids", "org_ids"]
        .iter()
        .all(|key| {
            let (Ok(expected), Ok(actual)) = (strings(args, key), strings(raw, key)) else {
                return false;
            };
            expected.iter().chain(&actual).all(|id| super::id(id)) && same_set(&expected, &actual)
        })
}
impl Draft {
    pub fn from_acquired(raw: &Value, ctx: &Context) -> Result<Self, Error> {
        ctx.validate()?;
        if raw.get("ok") != Some(&Value::Bool(true))
            || raw.get("error").is_some_and(|v| !v.is_null())
        {
            return Err(Error::Invalid);
        }
        let original_mode = Mode::parse(raw["permission_type"].as_str().ok_or(Error::Invalid)?)?;
        let original = [
            strings(raw, "user_ids")?,
            strings(raw, "channel_ids")?,
            strings(raw, "team_ids")?,
            strings(raw, "org_ids")?,
        ];
        for (list, expected) in
            original
                .iter()
                .zip([Kind::User, Kind::Channel, Kind::Team, Kind::Org])
        {
            if list.iter().any(|id| kind(id) != Ok(expected)) {
                return Err(Error::Invalid);
            }
        }
        let ids: Vec<_> = original.iter().flatten().cloned().collect();
        let internal = ctx.partition(&ids, false)?;
        let external = ctx.partition(&ids, true)?;
        let mode = if original_mode == Mode::NamedEntities
            && original[0].is_empty()
            && original[1].is_empty()
            && !internal.is_empty()
            && internal.iter().all(|id| id == &ctx.current_team_id)
        {
            Mode::Everyone
        } else {
            original_mode
        };
        Ok(Self {
            original_mode,
            original,
            mode,
            ids,
            unresolved: vec![],
            external_enabled: !external.is_empty(),
        })
    }
    pub fn mode(&self) -> Mode {
        self.mode
    }
    pub fn ids(&self) -> &[String] {
        &self.ids
    }
    pub fn unresolved(&self) -> &[String] {
        &self.unresolved
    }
    pub fn external_enabled(&self) -> bool {
        self.external_enabled
    }
    /// Permissions manager changes use Set([...managers, ...runTargets]), not
    /// the ordinary picker's order-preserving subset replacement. Retain the
    /// acquired baseline, unresolved tokens and external controls; desired()
    /// still applies eligibility, target policy, count and set-based no-op rules.
    pub fn union_managers(&mut self, managers: &[String], ctx: &Context) -> Result<(), Error> {
        if self.mode != Mode::NamedEntities {
            return Ok(());
        }
        ctx.validate()?;
        for id in managers {
            if ctx.entity(id)?.kind != Kind::User {
                return Err(Error::Invalid);
            }
        }
        let mut seen = BTreeSet::new();
        self.ids = managers
            .iter()
            .chain(&self.ids)
            .filter(|id| seen.insert((*id).clone()))
            .cloned()
            .collect();
        Ok(())
    }
    pub fn select_mode(&mut self, mode: Mode, ctx: &Context) -> Result<(), Error> {
        if mode == self.mode {
            return Ok(());
        }
        ctx.validate()?;
        let ids = match mode {
            Mode::NamedEntities => ctx
                .collaborators
                .iter()
                .cloned()
                .chain(ctx.partition(&self.ids, true)?)
                .collect(),
            Mode::Everyone if self.external_enabled => ctx
                .available_external_ids
                .as_ref()
                .ok_or(Error::MissingAvailable)?
                .iter()
                .take(99)
                .cloned()
                .collect(),
            _ => vec![],
        };
        self.ids = ids;
        self.unresolved.clear();
        self.mode = mode;
        if mode == Mode::AppCollaborators {
            self.external_enabled = false;
        }
        Ok(())
    }
    pub fn replace_internal(
        &mut self,
        selections: Vec<Selection>,
        ctx: &Context,
    ) -> Result<(), Error> {
        ctx.validate()?;
        let mut ids = vec![];
        let mut unresolved = vec![];
        for selection in selections {
            match selection {
                Selection::Resolved(id) => {
                    if ctx.is_external(&id)? {
                        return Err(Error::Invalid);
                    }
                    ids.push(id);
                }
                Selection::Unresolved(text) => unresolved.push(text),
            }
        }
        self.ids = update(&self.ids, &ctx.partition(&self.ids, false)?, &ids);
        self.unresolved = unresolved;
        Ok(())
    }
    pub fn replace_external(&mut self, ids: Vec<String>, ctx: &Context) -> Result<(), Error> {
        ctx.validate()?;
        for id in &ids {
            if !ctx.is_external(id)? {
                return Err(Error::Invalid);
            }
        }
        self.ids = update(&self.ids, &ctx.partition(&self.ids, true)?, &ids);
        // FOIV enables this checkbox when a newly selected external ID appears.
        if !ids.is_empty() {
            self.external_enabled = true;
        }
        Ok(())
    }
    pub fn toggle_external(&mut self, enabled: bool, ctx: &Context) -> Result<(), Error> {
        ctx.validate()?;
        if !enabled {
            let external = ctx.partition(&self.ids, true)?;
            self.ids.retain(|id| {
                !external.contains(id)
                    && !(self.mode == Mode::Everyone && id == &ctx.current_team_id)
            });
        }
        self.external_enabled = enabled;
        Ok(())
    }
    /// None is the source's changed-only no-op, not an acknowledged write.
    pub fn desired(&self, ctx: &Context, policy: &TargetPolicy) -> Result<Option<Command>, Error> {
        self.desired_for(ctx, policy, false)
    }
    /// Embedded Finish Up (fkuW) prepends the current scope with concat, unlike
    /// standalone settings' union. This can retain a duplicate current scope
    /// already present in an acquired named grant normalized to Everyone.
    pub fn desired_embedded(
        &self,
        ctx: &Context,
        policy: &TargetPolicy,
    ) -> Result<Option<Command>, Error> {
        self.desired_for(ctx, policy, true)
    }
    fn desired_for(
        &self,
        ctx: &Context,
        policy: &TargetPolicy,
        embedded: bool,
    ) -> Result<Option<Command>, Error> {
        ctx.validate()?;
        if !self.unresolved.is_empty() {
            return Err(Error::Unresolved);
        }
        if self.ids.len() > 100 {
            return Err(Error::TooMany);
        }
        if self.mode == Mode::NamedEntities && self.ids.is_empty() {
            return Err(Error::EmptyNamed);
        }
        let mut lists: [Vec<String>; 4] = Default::default();
        for id in &self.ids {
            let entity = ctx.entity(id)?;
            // Picker filters admit new choices. They do not revoke an already
            // acquired grant (e.g. an archived channel or a guest) on opening.
            if !entity.eligible && !self.original.iter().flatten().any(|old| old == id) {
                return Err(Error::Ineligible);
            }
            if ctx.is_external(id)? && !policy.allows_external(id)? {
                return Err(Error::Restricted);
            }
            let index = match entity.kind {
                Kind::User => 0,
                Kind::Channel => 1,
                Kind::Team => 2,
                Kind::Org => 3,
            };
            lists[index].push(id.clone());
        }
        if self.mode == self.original_mode
            && lists
                .iter()
                .zip(&self.original)
                .all(|(a, b)| same_set(a, b))
        {
            return Ok(None);
        }
        let [users, channels, mut teams, mut orgs] = lists;
        let mut mode = self.mode;
        if mode == Mode::Everyone && (!teams.is_empty() || !orgs.is_empty()) {
            mode = Mode::NamedEntities;
            let own = if kind(&ctx.current_team_id)? == Kind::Org {
                &mut orgs
            } else {
                &mut teams
            };
            if embedded {
                own.insert(0, ctx.current_team_id.clone());
            } else {
                *own = update(&[ctx.current_team_id.clone()], &[], own);
            }
        }
        Ok(Some(Command::RunPermission {
            permission_type: mode.as_str().into(),
            user_ids: if self.mode == Mode::NamedEntities {
                optional(users)
            } else {
                None
            },
            channel_ids: if self.mode == Mode::NamedEntities {
                optional(channels)
            } else {
                None
            },
            team_ids: if mode == Mode::NamedEntities {
                optional(teams)
            } else {
                None
            },
            org_ids: if mode == Mode::NamedEntities {
                optional(orgs)
            } else {
                None
            },
        }))
    }
}
