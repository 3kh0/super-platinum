//! Captured rR47/oMux/QFH5/VRXO policy. Observations are not owner permits.
//! Callers retain complete boot membership and account/request/event witnesses.
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Response,
    Shape,
    Membership,
    MissingPreference,
}
/// The actual client-v2 constructor omits the override and registerExtensions
/// defaults it to true. This is an explicit product policy, not an auth flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OrdinaryClientV2;

#[derive(Clone, Debug, PartialEq)]
pub struct UserPrefs {
    values: Map<String, Value>,
    hidden: BTreeSet<String>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct TeamPrefs {
    team: String,
    value: Option<Value>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct EnterprisePrefs {
    enterprise: String,
    org: Option<Value>,
    locked: Option<Value>,
    hide_locked: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Membership {
    enterprise: Option<String>,
    workspaces: BTreeMap<String, Option<String>>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    membership: Membership,
    user: UserPrefs,
    enterprise: Option<EnterprisePrefs>,
    teams: BTreeMap<String, TeamPrefs>,
}
fn id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 128 && s.bytes().all(|b| b.is_ascii_alphanumeric())
}
fn truth(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::String(s) => !s.is_empty(),
        Value::Number(n) => n.as_f64().is_some_and(|n| n != 0.0),
        _ => true,
    }
}
fn response(v: &Value) -> Result<(), Error> {
    if v.is_object() && v["ok"] == true && !v.get("error").is_some_and(|e| !e.is_null()) {
        Ok(())
    } else {
        Err(Error::Response)
    }
}
fn map<'a>(v: &'a Value, key: &str) -> Result<&'a Map<String, Value>, Error> {
    v.get(key).and_then(Value::as_object).ok_or(Error::Shape)
}
fn nonnull(v: Option<&Value>) -> Option<Value> {
    v.filter(|v| !v.is_null()).cloned()
}
impl UserPrefs {
    pub fn decode(raw: &Value) -> Result<Self, Error> {
        response(raw)?;
        Self::from_complete(map(raw, "prefs")?)
    }
    /// Only a complete observed prefs object (boot or users.prefs.get).
    pub fn from_complete(p: &Map<String, Value>) -> Result<Self, Error> {
        let hidden = match p.get("hidden_users") {
            None => BTreeSet::new(),
            Some(v) if !truth(v) => BTreeSet::new(),
            Some(Value::String(s)) if s.is_empty() => BTreeSet::new(),
            Some(Value::String(s)) => s.split(',').map(str::to_owned).collect(),
            _ => return Err(Error::Shape),
        };
        let mut values = Map::new();
        for k in ["hidden_users", "display_real_names_override"] {
            if let Some(v) = p.get(k) {
                values.insert(k.into(), v.clone());
            }
        }
        Ok(Self { values, hidden })
    }
}
impl TeamPrefs {
    pub fn decode(team: &str, raw: &Value) -> Result<Self, Error> {
        response(raw)?;
        Self::from_complete(team, map(raw, "prefs")?)
    }
    /// Actual complete workspaces[].prefs from client.gantryBoot. Missing maps
    /// are unavailable, not an observed empty preference object.
    pub fn from_complete(team: &str, prefs: &Map<String, Value>) -> Result<Self, Error> {
        if !id(team) {
            return Err(Error::Membership);
        }
        Ok(Self {
            team: team.into(),
            value: prefs.get("display_real_names").cloned(),
        })
    }
}
impl EnterprisePrefs {
    pub fn decode(enterprise: &str, raw: &Value) -> Result<Self, Error> {
        response(raw)?;
        if !id(enterprise) {
            return Err(Error::Membership);
        }
        let org = map(raw, "org_prefs")?;
        let locked = map(raw, "locked_prefs")?;
        Ok(Self {
            enterprise: enterprise.into(),
            org: nonnull(org.get("display_real_names")),
            locked: nonnull(locked.get("display_real_names")),
            hide_locked: locked.get("hide_person_opt_out").is_some_and(truth),
        })
    }
}
impl Membership {
    /// `ids` is the complete acquired internal-workspace inventory. Each team
    /// object must be the corresponding actual acquired object, not credentials.
    /// An omitted enterprise_id on that complete object means no association.
    pub fn observe(
        enterprise: Option<&str>,
        ids: &[String],
        teams: &BTreeMap<String, Value>,
    ) -> Result<Self, Error> {
        if enterprise.is_some_and(|e| !id(e)) || ids.len() > 10000 {
            return Err(Error::Membership);
        }
        let mut workspaces = BTreeMap::new();
        for key in ids {
            if !id(key) || workspaces.contains_key(key) {
                return Err(Error::Membership);
            }
            let raw = teams
                .get(key)
                .filter(|v| v.is_object() && v["id"].as_str() == Some(key.as_str()))
                .ok_or(Error::Membership)?;
            let ent = match raw.get("enterprise_id") {
                None | Some(Value::Null) => None,
                Some(Value::String(e)) if id(e) => Some(e.clone()),
                _ => return Err(Error::Shape),
            };
            workspaces.insert(key.clone(), ent);
        }
        Ok(Self {
            enterprise: enterprise.map(str::to_owned),
            workspaces,
        })
    }
    pub fn workspace_ids(&self) -> impl Iterator<Item = &str> {
        self.workspaces.keys().map(String::as_str)
    }
}
impl Snapshot {
    pub fn new(
        _: OrdinaryClientV2,
        membership: Membership,
        user: UserPrefs,
        enterprise: Option<EnterprisePrefs>,
    ) -> Result<Self, Error> {
        if membership.enterprise.as_deref() != enterprise.as_ref().map(|e| e.enterprise.as_str()) {
            return Err(Error::MissingPreference);
        }
        Ok(Self {
            membership,
            user,
            enterprise,
            teams: BTreeMap::new(),
        })
    }
    pub fn observe_team(&mut self, p: TeamPrefs) -> Result<(), Error> {
        if !self.membership.workspaces.contains_key(&p.team) {
            return Err(Error::Membership);
        }
        self.teams.insert(p.team.clone(), p);
        Ok(())
    }
    pub fn workspace_ids(&self) -> impl Iterator<Item = &str> {
        self.membership.workspace_ids()
    }
    fn inherited(&self, team: &str) -> Option<&Value> {
        let own = self.membership.enterprise.as_ref()?;
        if self.membership.workspaces.get(team)?.as_ref() != Some(own) {
            return None;
        }
        let e = self.enterprise.as_ref()?;
        e.org.as_ref().or(e.locked.as_ref())
    }
    fn override_value(&self) -> Option<bool> {
        match self
            .user
            .values
            .get("display_real_names_override")
            .and_then(Value::as_f64)
        {
            Some(1.0) => Some(true),
            Some(-1.0) => Some(false),
            _ => None,
        }
    }
    pub fn missing_team_preferences(&self) -> Vec<String> {
        if self.override_value().is_some() {
            return vec![];
        }
        self.membership
            .workspaces
            .keys()
            .filter(|id| self.inherited(id).is_none() && !self.teams.contains_key(*id))
            .cloned()
            .collect()
    }
    pub fn should_display_real_names(&self) -> Result<bool, Error> {
        if let Some(v) = self.override_value() {
            return Ok(v);
        }
        let mut unknown = false;
        for team in self.membership.workspaces.keys() {
            let value = if let Some(v) = self.inherited(team) {
                Some(v)
            } else if let Some(p) = self.teams.get(team) {
                p.value.as_ref()
            } else {
                unknown = true;
                None
            };
            if value == Some(&Value::Bool(true)) {
                return Ok(true);
            }
        }
        if unknown {
            Err(Error::MissingPreference)
        } else {
            Ok(false)
        }
    }
    pub fn hidden(&self, id: &str) -> bool {
        !id.is_empty()
            && !self.enterprise.as_ref().is_some_and(|e| e.hide_locked)
            && self.user.hidden.contains(id)
    }
    /// Plain text for the existing typed renderer. Localized hidden text is an
    /// explicit renderer observation; escaping belongs to that renderer.
    pub fn member_name(&self, member: &Value, hidden_text: &str) -> Result<String, Error> {
        if member.is_null() {
            return Ok(String::new());
        }
        let id = member["id"].as_str().ok_or(Error::Shape)?;
        if self.hidden(id) {
            return Ok(hidden_text.into());
        }
        if !member["profile"].is_null() && !member["profile"].is_object() {
            return Err(Error::Shape);
        }
        for value in [
            &member["profile"]["real_name"],
            &member["profile"]["display_name"],
            &member["real_name"],
        ] {
            if !value.is_null() && !value.is_string() {
                return Err(Error::Shape);
            }
        }
        let real = member["profile"]["real_name"]
            .as_str()
            .filter(|s| !s.is_empty())
            .or_else(|| member["real_name"].as_str())
            .unwrap_or("");
        let display = member["profile"]["display_name"]
            .as_str()
            .filter(|s| !s.is_empty());
        Ok(if self.should_display_real_names()? {
            real
        } else {
            display.unwrap_or(real)
        }
        .into())
    }
}

pub mod request;
#[cfg(test)]
mod tests;
