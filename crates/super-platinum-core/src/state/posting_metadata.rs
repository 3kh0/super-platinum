//! Fresh, account-local inputs for the current client posting selectors.
//!
//! Boot retention is wired into Workspace. Callers still own transport/account
//! epochs, rate limits, ordered channel ingress and scheduling missing reads.
//! This module never turns warm-cache/default-filled models into permission.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde_json::{Map, Value};

use crate::block_actions::posting_policy::{self, ChannelAdmin, Context, Flags, Observed};
use crate::config::Session;
use crate::slack::models::BootData;

pub mod authority;
mod external;
mod reads;
mod roles;
pub use reads::Read;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unknown {
    Identity,
    Configuration,
    Membership,
    Role,
    Shape,
    Stale,
    Policy(posting_policy::Unknown),
}
pub type Result<T> = std::result::Result<T, Unknown>;
impl From<posting_policy::Unknown> for Unknown {
    fn from(value: posting_policy::Unknown) -> Self {
        Self::Policy(value)
    }
}

/// A projection of the retained auth configuration, never an entitlement.
/// The current auth path discards T credentials when E credentials exist.
/// Therefore retained true proves the any-config OR, but all-false E config
/// cannot prove false. Legacy/missing flags stay unknown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Configuration {
    team: String,
    user: String,
    enterprise: Option<String>,
    unified: Option<bool>,
}
impl Configuration {
    pub fn from_session(session: &Session, team: &str) -> Result<Self> {
        let selected = session.workspaces.get(team).ok_or(Unknown::Identity)?;
        if selected.team_id != team || !id(team) || !id(&selected.user_id) {
            return Err(Unknown::Identity);
        }
        let enabled = session
            .workspaces
            .keys()
            .any(|key| session.unified_user_client_enabled.get(key) == Some(&true));
        let complete_non_grid = !session.workspaces.is_empty()
            && session.workspaces.keys().all(|key| {
                key.starts_with('T') && session.unified_user_client_enabled.get(key) == Some(&false)
            });
        Ok(Self {
            team: team.to_owned(),
            user: selected.user_id.clone(),
            enterprise: selected.enterprise_id.clone(),
            unified: enabled
                .then_some(true)
                .or(complete_non_grid.then_some(false)),
        })
    }

    pub fn unified(&self) -> Option<bool> {
        self.unified
    }
}

/// Fields are private so a caller cannot substitute a global is_admin fact for
/// channel-scoped authority. Clones are snapshots of the same boot owner.
#[derive(Clone, Debug, Default)]
pub struct Metadata {
    external: external::Store,
    runtime: authority::Runtime,
    epoch: Arc<()>,
    team_id: String,
    user_id: String,
    team: Value,
    member: Value,
    boot_enterprise: Option<String>,
    read_only: Option<BTreeSet<String>>,
    thread_only: Option<BTreeSet<String>>,
    observations: BTreeMap<String, (Configuration, Observed, [bool; 2])>,
    workspaces: BTreeMap<String, Value>,
    internal_workspaces: Option<Vec<String>>,
    workspace_admin: BTreeMap<String, bool>,
    /// Failed refreshes keep the last known role only as a change-detection
    /// baseline. These keys must never admit through that retained value.
    invalid_workspace_roles: BTreeSet<String>,
    manage_posting: Option<bool>,
    reads: BTreeMap<String, Arc<()>>,
}

impl Metadata {
    /// Retire all acquired facts when the runtime loses account/role/channel
    /// event ordering. Recovery requires a fresh complete boot, not cache reuse.
    pub fn invalidate(&mut self) {
        let mut external = std::mem::take(&mut self.external);
        external.retire();
        *self = Self {
            external,
            ..Self::default()
        };
    }

    fn retire_channel_facts(&mut self) {
        self.runtime.retire_channels();
        self.read_only = None;
        self.thread_only = None;
        self.observations.clear();
    }

    fn retire_derived_flags(&mut self) {
        for (_, observed, derived) in self.observations.values_mut() {
            observed.forget_values(derived[0], derived[1]);
        }
    }

    /// Accept only the actual complete boot envelope, before conversion to User
    /// with default boolean fields. Do not call with a reconstructed cache boot.
    pub fn from_boot(team_id: &str, user_id: &str, boot: &BootData) -> Self {
        let mut out = Self::default();
        let Some(team) = boot.team.as_ref() else {
            return out;
        };
        if team.id != team_id || boot.self_user.id != user_id || !id(team_id) || !id(user_id) {
            return out;
        }
        out.team_id = team_id.into();
        out.user_id = user_id.into();
        // Whitelists retain policy shapes, never tokens, profile or message data.
        let mut raw_team = Map::new();
        raw_team.insert("id".into(), Value::String(team.id.clone()));
        if let Some(plan) = team.extra.get("plan") {
            raw_team.insert("plan".into(), plan.clone());
        }
        if let Some(enterprise) = &team.enterprise_id {
            raw_team.insert("enterprise_id".into(), Value::String(enterprise.clone()));
        }
        out.team = Value::Object(raw_team);
        out.member = selected(
            &Value::Object(boot.self_user.extra.clone().into_iter().collect()),
            &[
                "team_id",
                "enterprise_id",
                "is_admin",
                "is_owner",
                "is_primary_owner",
                "is_restricted",
                "isUnknown",
                "isNonExistent",
            ],
        );
        out.member
            .as_object_mut()
            .unwrap()
            .insert("id".into(), Value::String(user_id.into()));
        if let Some(enterprise) = boot.self_user.extra.get("enterprise_user") {
            let value = if enterprise.is_object() {
                selected(
                    enterprise,
                    &[
                        "id",
                        "enterprise_id",
                        "teams",
                        "is_admin",
                        "is_owner",
                        "is_primary_owner",
                    ],
                )
            } else {
                enterprise.clone()
            };
            out.member
                .as_object_mut()
                .unwrap()
                .insert("enterprise_user".into(), value);
        }
        out.boot_enterprise = boot
            .extra
            .get("enterprise_id")
            .and_then(Value::as_str)
            .filter(|v| id(v))
            .map(str::to_owned);
        out.read_only = boot
            .extra
            .get("read_only_channels")
            .and_then(strings)
            .map(|v| v.into_iter().collect());
        out.thread_only = boot
            .extra
            .get("thread_only_channels")
            .and_then(strings)
            .map(|v| v.into_iter().collect());
        out.workspaces.insert(team_id.into(), out.team.clone());
        out.retain_workspaces(boot.extra.get("workspaces"));
        out.retain_account_types(boot.extra.get("account_types"));
        // kH+p first adopts account_types, then kzmv applies the local member.
        // This is an ingress ordering rule, not a permanent query override:
        // subsequent users.info observations must be able to replace it.
        if out.member.get("enterprise_id").is_none_or(Value::is_null)
            && let Some(local) = out
                .member
                .get("team_id")
                .and_then(Value::as_str)
                .filter(|v| v.starts_with('T') && id(v))
        {
            match flag(&out.member, "is_admin") {
                Ok(admin) => {
                    out.workspace_admin.insert(local.into(), admin);
                }
                Err(_) => {
                    out.workspace_admin.remove(local);
                }
            }
        }
        out
    }

    pub fn context<'a>(
        &'a self,
        config: &'a Configuration,
        channel: &'a Value,
    ) -> Result<Context<'a>> {
        self.check(config)?;
        let unified = config.unified.ok_or(Unknown::Configuration)?;
        let channel_id = channel
            .get("id")
            .and_then(Value::as_str)
            .filter(|v| id(v))
            .ok_or(Unknown::Identity)?;
        let top = config
            .enterprise
            .as_deref()
            .or(self.boot_enterprise.as_deref())
            .unwrap_or(&self.team_id);
        let admin = self.channel_admin(config, channel).ok();
        Ok(Context {
            team_id: &self.team_id,
            top_level_team_id: top,
            user_id: &self.user_id,
            team: &self.team,
            member: &self.member,
            unified,
            manage_posting_access: self.manage_posting,
            channel_admin: admin.map(|allowed| ChannelAdmin {
                channel_id,
                user_id: &self.user_id,
                allowed,
            }),
        })
    }

    /// Call at ordered *raw* channel ingress. A cached Channel serialized with
    /// defaults is not a new effective-flag observation. A failed observation
    /// retires the old channel facts rather than continuing with stale grants.
    pub fn observe_channel(&mut self, config: &Configuration, channel: &Value) -> Result<()> {
        self.check(config)?;
        let key = channel
            .get("id")
            .and_then(Value::as_str)
            .filter(|v| id(v))
            .ok_or(Unknown::Identity)?
            .to_owned();
        let mut observed = self.observation(config, &key);
        let mut derived = self
            .observations
            .get(&key)
            .map(|(_, _, derived)| *derived)
            .unwrap_or([false; 2]);
        let flags = [channel.get("is_read_only"), channel.get("is_thread_only")];
        if flags.iter().any(|v| v.is_some_and(Value::is_boolean)) {
            // 340b's direct branch updates each present companion independently;
            // an omitted companion keeps its prior acquisition dependency.
            for (index, value) in flags.iter().enumerate() {
                if value.is_some() {
                    derived[index] = false;
                }
            }
        } else if channel.get("properties").is_some_and(|props| {
            ["posting_restricted_to", "threads_restricted_to"]
                .iter()
                .any(|key| props.get(*key).is_some_and(|v| !v.is_null()))
        }) {
            derived = [true; 2];
        }
        let result = self
            .context(config, channel)
            .and_then(|ctx| observed.observe(channel, &ctx).map_err(Into::into));
        if result.is_err() {
            observed = Observed::unknown(key.clone());
        }
        self.observations
            .insert(key, (config.clone(), observed, derived));
        result
    }

    pub fn effective(&self, config: &Configuration, channel: &Value) -> Result<Flags> {
        let ctx = self.context(config, channel)?;
        let key = channel
            .get("id")
            .and_then(Value::as_str)
            .ok_or(Unknown::Identity)?;
        self.observation(config, key)
            .effective(channel, &ctx)
            .map_err(Into::into)
    }

    /// Actual gq7X read-only selector for thread admission; no file root-post override.
    pub fn thread_read_only(&self, config: &Configuration, channel: &Value) -> Result<bool> {
        let ctx = self.context(config, channel)?;
        let key = channel
            .get("id")
            .and_then(Value::as_str)
            .ok_or(Unknown::Identity)?;
        self.observation(config, key)
            .thread_read_only(channel, &ctx)
            .map_err(Into::into)
    }

    fn observation(&self, config: &Configuration, key: &str) -> Observed {
        self.observations
            .get(key)
            .map(|(observed_config, observed, _)| {
                if observed_config == config {
                    observed.clone()
                } else {
                    Observed::unknown(key.into())
                }
            })
            .unwrap_or_else(|| match (&self.read_only, &self.thread_only) {
                (Some(read), Some(thread)) => {
                    Observed::from_inventory(key.into(), read.contains(key), thread.contains(key))
                }
                _ => Observed::unknown(key.into()),
            })
    }

    fn check(&self, config: &Configuration) -> Result<()> {
        if self.team_id != config.team || self.user_id != config.user || self.team_id.is_empty() {
            return Err(Unknown::Identity);
        }
        if config
            .enterprise
            .as_deref()
            .is_some_and(|v| !v.starts_with('E') || !id(v))
        {
            return Err(Unknown::Identity);
        }
        Ok(())
    }
}

fn id(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
}
fn strings(value: &Value) -> Option<Vec<String>> {
    value
        .as_array()?
        .iter()
        .map(|v| v.as_str().filter(|v| id(v)).map(str::to_owned))
        .collect()
}
fn selected(value: &Value, keys: &[&str]) -> Value {
    Value::Object(
        keys.iter()
            .filter_map(|key| value.get(*key).map(|v| ((*key).into(), v.clone())))
            .collect(),
    )
}
fn flag(value: &Value, key: &str) -> Result<bool> {
    match value.get(key) {
        None => Ok(false),
        Some(v) => v.as_bool().ok_or(Unknown::Shape),
    }
}
