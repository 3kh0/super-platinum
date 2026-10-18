//! The client's positive-only externalTeamIds routing store (j08h/V/NW).
//! This is neither an internal-membership complement nor a posting entitlement.
use super::*;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Owner {
    team: String,
    user: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
enum Saved {
    #[default]
    Uninitialized,
    Positive {
        owner: Owner,
        ids: BTreeSet<String>,
    },
    Invalid,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    version: u32,
    saved: Saved,
}

#[derive(Clone, Debug, Default)]
pub(super) struct Store {
    saved: Saved,
    top: Option<String>,
    revision: Arc<()>,
    free: Option<bool>,
    validated: Option<Instant>,
    attempted: Option<Instant>,
    pending: Option<Arc<()>>,
}
impl Store {
    pub(super) fn retire(&mut self) {
        self.top = None;
        self.revision = Arc::new(());
        self.free = None;
        self.pending = None;
    }
    pub(super) fn bind(&mut self, team: &str, user: &str, top: Result<String>) -> Result<()> {
        self.retire();
        let top = top?;
        if !id(team) || !id(user) || !id(&top) {
            return Err(Unknown::Identity);
        }
        let owner = Owner {
            team: team.into(),
            user: user.into(),
        };
        match &self.saved {
            Saved::Invalid => return Err(Unknown::Shape),
            Saved::Positive {
                owner: previous, ..
            } if previous == &owner => {}
            _ => {
                self.saved = Saved::Positive {
                    owner,
                    ids: BTreeSet::new(),
                };
                self.validated = None;
                self.attempted = None;
            }
        }
        self.top = Some(top);
        Ok(())
    }
    fn add(&mut self, values: impl IntoIterator<Item = String>) -> Result<()> {
        if self.top.is_none() {
            return Err(Unknown::Stale);
        }
        let Saved::Positive { ids, .. } = &mut self.saved else {
            return Err(Unknown::Shape);
        };
        let mut changed = false;
        for value in values {
            changed |= ids.insert(value);
        }
        if changed {
            self.revision = Arc::new(());
        }
        Ok(())
    }
    /// 340b's explicitly present raw connection arrays. Omission means no new
    /// positives; empty arrays do not delete any already retained positives.
    pub(super) fn channel(&mut self, raw: &Value) -> Result<()> {
        let top = self.top.as_ref().ok_or(Unknown::Stale)?.clone();
        let mut found = Vec::new();
        for key in ["connected_team_ids", "connected_limited_team_ids"] {
            let Some(value) = raw.get(key) else {
                continue;
            };
            if value.is_null() {
                continue;
            }
            let Some(values) = strings(value) else {
                // A malformed ingestion cannot establish a negative store view.
                self.retire();
                return Err(Unknown::Shape);
            };
            found.extend(values.into_iter().filter(|value| value != &top));
        }
        self.add(found)
    }
    /// nsR3(isExternal:true) and CZZ0 add the supplied actual positive without
    /// 340b's Kc exclusion. Only their owned source-specific adapters call this.
    pub(super) fn positive(&mut self, value: &str) -> Result<()> {
        if !id(value) {
            self.retire();
            return Err(Unknown::Identity);
        }
        self.add([value.into()])
    }
    pub(super) fn contains(&self, key: &str) -> Result<bool> {
        if !id(key) {
            return Err(Unknown::Identity);
        }
        if self.top.is_none() {
            return Err(Unknown::Stale);
        }
        match &self.saved {
            Saved::Positive { ids, .. } if ids.contains(key) => Ok(true),
            Saved::Positive { .. } if self.status(Instant::now())? == Validation::Ready => {
                Ok(false)
            }
            Saved::Positive { .. } => Err(Unknown::Stale),
            _ => Err(Unknown::Shape),
        }
    }
    pub(super) fn revision(&self) -> Arc<()> {
        self.revision.clone()
    }
    pub(super) fn current(&self, revision: &Arc<()>) -> bool {
        self.top.is_some() && Arc::ptr_eq(&self.revision, revision)
    }
    pub(super) fn set_free(&mut self, free: Result<bool>) {
        self.free = free.ok();
    }
    pub(super) fn status(&self, now: Instant) -> Result<Validation> {
        if self.top.is_none() {
            return Err(Unknown::Stale);
        }
        let free = self.free.ok_or(Unknown::Shape)?;
        if !free
            || self
                .validated
                .is_some_and(|at| now.saturating_duration_since(at) < INTERVAL)
        {
            return Ok(Validation::Ready);
        }
        if self.pending.is_some() {
            return Ok(Validation::Pending);
        }
        if let Some(at) = self.attempted {
            let age = now.saturating_duration_since(at);
            if age < INTERVAL {
                return Ok(Validation::Throttled(INTERVAL - age));
            }
        }
        Ok(Validation::Required)
    }
    pub(super) fn begin_validation(&mut self, now: Instant) -> Result<Arc<()>> {
        if self.status(now)? != Validation::Required {
            return Err(Unknown::Stale);
        }
        let nonce = Arc::new(());
        self.attempted = Some(now);
        self.pending = Some(nonce.clone());
        self.revision = Arc::new(());
        Ok(nonce)
    }
    pub(super) fn validation_current(&self, nonce: &Arc<()>) -> bool {
        self.top.is_some() && self.pending.as_ref().is_some_and(|n| Arc::ptr_eq(n, nonce))
    }
    pub(super) fn finish_validation(
        &mut self,
        nonce: &Arc<()>,
        response: Option<&Value>,
        now: Instant,
    ) -> Result<()> {
        if !self.validation_current(nonce) {
            return Err(Unknown::Stale);
        }
        self.pending = None;
        let raw = response.ok_or(Unknown::Shape)?;
        if raw.get("ok") != Some(&Value::Bool(true))
            || raw.get("error").is_some_and(|value| !value.is_null())
        {
            return Err(Unknown::Shape);
        }
        let orgs = raw
            .get("orgs")
            .and_then(Value::as_array)
            .ok_or(Unknown::Shape)?;
        if let Some(first) = orgs.first() {
            let team = first
                .get("team_summary")
                .and_then(|v| v.get("id"))
                .and_then(Value::as_str)
                .ok_or(Unknown::Shape)?;
            self.positive(team)?;
        }
        self.validated = Some(now);
        self.revision = Arc::new(());
        Ok(())
    }
    pub(super) fn export(&self) -> Value {
        serde_json::to_value(Envelope {
            version: 1,
            saved: self.saved.clone(),
        })
        .expect("external store serialization")
    }
    pub(super) fn restore(raw: Option<&Value>) -> Self {
        let saved = match raw {
            None => Saved::Uninitialized,
            Some(raw) => match serde_json::from_value::<Envelope>(raw.clone()) {
                Ok(Envelope {
                    version: 1,
                    saved: Saved::Positive { owner, ids },
                }) if id(&owner.team) && id(&owner.user) && ids.iter().all(|v| id(v)) => {
                    Saved::Positive { owner, ids }
                }
                Ok(Envelope {
                    version: 1,
                    saved: Saved::Uninitialized,
                }) => Saved::Uninitialized,
                _ => Saved::Invalid,
            },
        };
        Self {
            saved,
            ..Default::default()
        }
    }
}

impl Metadata {
    /// Cache payload only: never includes bound authority, raw channels, tokens,
    /// cookies, roles or permission booleans. Cache remains account-local.
    pub fn cached_external_teams(&self) -> Value {
        self.external.export()
    }
    pub fn restore_cached_external_teams(&mut self, raw: Option<&Value>) {
        self.external = Store::restore(raw);
    }
    pub fn external_cache_snapshot(&self) -> Self {
        let mut out = Self::default();
        out.external = self.external.clone();
        out.external.retire();
        out
    }
    pub(in crate::state) fn replace_boot_preserving_external(&mut self, mut metadata: Self) {
        metadata.external = std::mem::take(&mut self.external);
        metadata.external.retire();
        *self = metadata;
    }
}

const INTERVAL: Duration = Duration::from_secs(900);
/// CZZ0's 15-minute validation interval. Callers retain original Retry-After and
/// must honor the larger of it and this local interval after a failed attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Validation {
    Ready,
    Required,
    Pending,
    Throttled(Duration),
}

// lahp initializes selfTeamIds; v7Vi.Kc selects enterprise, boot enterprise,
// then team. Evaluate both unknown-global-mode branches and require agreement.
pub(super) fn top(config: &Configuration, raw: &Value) -> Result<String> {
    let boot = match raw.get("enterprise_id") {
        None | Some(Value::Null) => None,
        Some(Value::String(v)) if v.is_empty() => None,
        Some(Value::String(v)) if id(v) => Some(v.as_str()),
        _ => return Err(Unknown::Shape),
    };
    if config.enterprise.as_deref().is_some_and(|v| !id(v)) {
        return Err(Unknown::Identity);
    }
    let branch = |unified: bool| {
        if config.team.starts_with('E') && !unified {
            config.team.clone()
        } else {
            config
                .enterprise
                .as_deref()
                .or(boot)
                .unwrap_or(&config.team)
                .to_owned()
        }
    };
    match config.unified {
        Some(mode) => Ok(branch(mode)),
        None if branch(false) == branch(true) => Ok(branch(false)),
        None => Err(Unknown::Configuration),
    }
}
pub(super) fn free(config: &Configuration, raw: &Value) -> Result<bool> {
    // With non-unified E, currentTeam is absent; with unified E it is an
    // enterprise. Neither satisfies 7MjS.tc. T uses its actual boot team.
    if config.team.starts_with('E') {
        return Ok(false);
    }
    let team = raw
        .get("team")
        .filter(|v| v.is_object())
        .ok_or(Unknown::Shape)?;
    if team.get("id").and_then(Value::as_str) != Some(&config.team) {
        return Err(Unknown::Identity);
    }
    let truthy = |value: Option<&Value>| match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(v)) => *v,
        Some(Value::String(v)) => !v.is_empty(),
        Some(Value::Number(v)) => v.as_f64().is_some_and(|v| v != 0.0),
        Some(_) => true,
    };
    Ok(!truthy(team.get("plan")) && !truthy(team.get("enterprise_id")))
}

#[cfg(test)]
mod tests;
