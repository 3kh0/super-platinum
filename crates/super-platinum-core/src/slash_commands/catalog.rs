//! Narrow installed-app rows only; raw catalog survives unsupported rows.
use super::*;
use std::{collections::BTreeSet, sync::Arc};

#[derive(Clone)]
pub struct Catalog(Arc<Data>);
struct Data {
    owner: Owner,
    raw: Value,
    commands: Vec<Command>,
    unsupported: usize,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub name: String,
    pub canonical: String,
    pub app: String,
    pub teams: Vec<String>,
    pub thread_allowed: bool,
    pub autocomplete: bool,
    pub raw: Value,
}
#[derive(Clone)]
pub struct Choice {
    pub(super) catalog: Catalog,
    pub(super) index: usize,
}
impl Catalog {
    pub fn decode_bytes(owner: Owner, bytes: &[u8]) -> Result<Self, Failure> {
        Self::decode(
            owner,
            codec::decode(bytes, 2 * 1024 * 1024).map_err(|_| Failure::Invalid)?,
        )
    }
    pub fn decode(owner: Owner, raw: Value) -> Result<Self, Failure> {
        owner.validate()?;
        if !bounded(&raw, 2 * 1024 * 1024)
            || raw.get("ok") != Some(&Value::Bool(true))
            || raw.get("error").is_some()
            || !raw.get("app_actions").is_some_and(Value::is_array)
        {
            return Err(Failure::Invalid);
        }
        let rows = raw
            .get("commands")
            .and_then(Value::as_array)
            .ok_or(Failure::Invalid)?;
        if rows.len() > 10_000 {
            return Err(Failure::Invalid);
        }
        let mut commands = Vec::new();
        let mut unsupported = 0;
        let mut seen = BTreeSet::new();
        for row in rows {
            if !row.is_object() {
                return Err(Failure::Invalid);
            }
            let Some(command) = parse(row) else {
                unsupported += 1;
                continue;
            };
            let key = (command.app.clone(), command.name.clone());
            if !seen.insert(key) {
                return Err(Failure::Invalid);
            }
            commands.push(command);
        }
        Ok(Self(Arc::new(Data {
            owner,
            raw,
            commands,
            unsupported,
        })))
    }
    pub fn same_generation(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    pub fn raw(&self) -> &Value {
        &self.0.raw
    }
    pub fn owner(&self) -> &Owner {
        &self.0.owner
    }
    pub fn unsupported(&self) -> usize {
        self.0.unsupported
    }
    pub fn commands(&self) -> &[Command] {
        &self.0.commands
    }
    pub fn choice(&self, index: usize) -> Option<Choice> {
        self.0.commands.get(index)?;
        Some(Choice {
            catalog: self.clone(),
            index,
        })
    }
}
impl Choice {
    pub fn command(&self) -> &Command {
        &self.catalog.0.commands[self.index]
    }
    pub fn admit(&self, current: &Catalog, owner: &Owner) -> Result<&Command, Failure> {
        if !self.catalog.same_generation(current) || current.owner() != owner {
            return Err(Failure::Stale);
        }
        let command = self.command();
        // Narrow ordinary same-workspace app path. Shared/Grid chooser not guessed.
        if !command.autocomplete
            || command.teams.as_slice() != [owner.team.clone()]
            || owner.thread.is_some() && !command.thread_allowed
        {
            return Err(Failure::Unsupported);
        }
        Ok(command)
    }
}
fn text<'a>(row: &'a Value, name: &str) -> Option<&'a str> {
    row.get(name)?.as_str()
}
pub(super) fn name(value: &str) -> bool {
    value.starts_with('/')
        && value.len() > 1
        && value.len() <= 128
        && value[1..]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
pub(super) fn parse(raw: &Value) -> Option<Command> {
    if !bounded(raw, 64 * 1024)
        || text(raw, "type") != Some("app")
        || raw.get("alias_of").is_some()
        || ["is_enterprise_install", "is_workspace_agnostic"]
            .iter()
            .any(|k| raw.get(k).is_some_and(|v| v != &Value::Bool(false)))
    {
        return None;
    }
    let command = text(raw, "name")?;
    let canonical = match raw.get("canonical_name") {
        None => command,
        Some(v) => v.as_str()?,
    };
    let app = text(raw, "app")?;
    if !name(command) || !name(canonical) || !id(app, 'A') {
        return None;
    }
    let teams: Vec<_> = raw
        .get("teams")?
        .as_array()?
        .iter()
        .map(|v| v.as_str().map(str::to_owned))
        .collect::<Option<_>>()?;
    if teams.is_empty()
        || teams.iter().any(|v| !id(v, 'T'))
        || teams.iter().collect::<BTreeSet<_>>().len() != teams.len()
    {
        return None;
    }
    let boolean = |key, default| match raw.get(key) {
        None => Some(default),
        Some(v) => v.as_bool(),
    };
    Some(Command {
        name: command.into(),
        canonical: canonical.into(),
        app: app.into(),
        teams,
        thread_allowed: boolean("is_runnable_from_thread", false)?,
        autocomplete: boolean("autocomplete", true)?,
        raw: raw.clone(),
    })
}
