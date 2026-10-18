//! Runtime ownership of actual boot and ordered raw channel observations.
//! No cache restoration or picker-open configuration can create this authority.
use std::{collections::BTreeMap, fmt, sync::Arc};

use serde_json::Value;

use super::{Configuration, Metadata, Observed, Result, Unknown, id};
use crate::{CoreAppState, config::WorkspaceSession, slack::Transport};

mod boot;
mod events;
mod external;
mod stamp;
pub use super::external::Validation as ExternalValidation;
pub use boot::BootRead;
pub use events::{observe_event, retire};
pub use external::{ExternalTeam, ExternalValidationRead};
use stamp::Stamp;
#[cfg(test)]
mod tests;

#[derive(Clone, Default)]
pub(super) struct Runtime {
    bound: Option<Arc<Scope>>,
    pending_boot: Option<Arc<()>>,
    channels: BTreeMap<String, (Arc<()>, Option<Arc<Value>>)>,
    stamps: BTreeMap<String, Stamp>,
}
impl Runtime {
    pub(super) fn retire_channels(&mut self) {
        self.channels.clear();
        self.stamps.clear();
        self.pending_boot = None;
    }
}
impl fmt::Debug for Runtime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Runtime")
            .field("bound", &self.bound.is_some())
            .field("pending_boot", &self.pending_boot.is_some())
            .field("channel_count", &self.channels.len())
            .finish()
    }
}

// Secrets are retained only in memory to reject changed request credentials.
// Neither Scope nor its enclosing public witnesses expose them through Debug.
struct Scope {
    account: Option<String>,
    epoch: u64,
    transport: Arc<Transport>,
    session: WorkspaceSession,
    cookie: String,
    configuration: Option<Configuration>,
    // Bind the exact input set as well as its current effective projection.
    config_inputs: BTreeMap<String, bool>,
    credential_teams: Vec<String>,
}
impl Scope {
    fn capture(core: &CoreAppState, team: &str) -> Result<Self> {
        let session = core.session.as_ref().ok_or(Unknown::Identity)?;
        let selected = session.workspaces.get(team).ok_or(Unknown::Identity)?;
        let transport = core.transport.clone().ok_or(Unknown::Identity)?;
        if selected.team_id != team
            || !id(team)
            || !core.workspaces.contains_key(team)
            || transport.d_cookie() != session.d_cookie
        {
            return Err(Unknown::Identity);
        }
        Ok(Self {
            account: core.active_account.clone(),
            epoch: core.account_epoch,
            transport,
            session: selected.clone(),
            cookie: session.d_cookie.clone(),
            configuration: Configuration::from_session(session, team).ok(),
            config_inputs: session.unified_user_client_enabled.clone(),
            credential_teams: session.workspaces.keys().cloned().collect(),
        })
    }
    fn current(&self, core: &CoreAppState) -> bool {
        core.active_account == self.account
            && core.account_epoch == self.epoch
            && core
                .transport
                .as_ref()
                .is_some_and(|t| Arc::ptr_eq(t, &self.transport))
            && core.session.as_ref().is_some_and(|s| {
                s.d_cookie == self.cookie
                    && s.unified_user_client_enabled == self.config_inputs
                    && s.workspaces.keys().eq(self.credential_teams.iter())
                    && s.workspaces
                        .get(&self.session.team_id)
                        .is_some_and(|w| same_credentials(w, &self.session))
                    && Configuration::from_session(s, &self.session.team_id).ok()
                        == self.configuration
            })
    }
}
fn same_credentials(left: &WorkspaceSession, right: &WorkspaceSession) -> bool {
    left.team_id == right.team_id
        && left.user_id == right.user_id
        && left.enterprise_id == right.enterprise_id
        && left.token == right.token
        && left.url == right.url
}

/// Acquired at actual boot ingress. Keep this witness with a picker/selection
/// and recheck it before every request, adoption and final action.
#[derive(Clone)]
pub struct Authority {
    scope: Arc<Scope>,
    epoch: Arc<()>,
}
impl fmt::Debug for Authority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Authority { redacted }")
    }
}
impl Authority {
    pub fn capture(core: &CoreAppState, team: &str) -> Result<Self> {
        let metadata = &core
            .workspaces
            .get(team)
            .ok_or(Unknown::Identity)?
            .posting_metadata;
        let out = Self {
            scope: metadata.runtime.bound.clone().ok_or(Unknown::Stale)?,
            epoch: metadata.epoch.clone(),
        };
        out.current(core).then_some(out).ok_or(Unknown::Stale)
    }
    pub fn current(&self, core: &CoreAppState) -> bool {
        self.scope.current(core)
            && self.scope.configuration.is_some()
            && core
                .workspaces
                .get(&self.scope.session.team_id)
                .is_some_and(|w| {
                    w.self_user_id == self.scope.session.user_id
                        && Arc::ptr_eq(&w.posting_metadata.epoch, &self.epoch)
                        && w.posting_metadata
                            .runtime
                            .bound
                            .as_ref()
                            .is_some_and(|s| Arc::ptr_eq(s, &self.scope))
                })
    }
    pub fn configuration(&self) -> &Configuration {
        self.scope
            .configuration
            .as_ref()
            .expect("bound authority configuration")
    }
    pub fn metadata<'a>(&self, core: &'a CoreAppState) -> Result<&'a Metadata> {
        if !self.current(core) {
            return Err(Unknown::Stale);
        }
        Ok(&core.workspaces[&self.scope.session.team_id].posting_metadata)
    }
    pub fn metadata_mut<'a>(&self, core: &'a mut CoreAppState) -> Result<&'a mut Metadata> {
        if !self.current(core) {
            return Err(Unknown::Stale);
        }
        Ok(&mut core
            .workspaces
            .get_mut(&self.scope.session.team_id)
            .unwrap()
            .posting_metadata)
    }
    /// Actual acquired internal membership; no credential/enterprise inference.
    pub fn internal_workspaces<'a>(&self, core: &'a CoreAppState) -> Result<&'a [String]> {
        self.metadata(core)?
            .internal_workspaces
            .as_deref()
            .ok_or(Unknown::Membership)
    }
    pub fn raw_channel(&self, core: &CoreAppState, channel: &str) -> Result<RawChannel> {
        let metadata = self.metadata(core)?;
        if metadata.runtime.stamps.get(channel)
            != Some(&Stamp::capture(core, &self.scope.session.team_id, channel))
        {
            return Err(Unknown::Stale);
        }
        let (revision, raw) = self
            .metadata(core)?
            .runtime
            .channels
            .get(channel)
            .ok_or(Unknown::Stale)?;
        Ok(RawChannel {
            authority: self.clone(),
            channel: channel.into(),
            revision: revision.clone(),
            raw: raw.clone().ok_or(Unknown::Stale)?,
        })
    }
    /// Partial collection of currently owned raw observations (boot and later
    /// owned reads). Each witness must still be rechecked before use. Omitted,
    /// invalidated or history-only observations are not an absence proof: this
    /// is NEVER a complete channel inventory or a complete external-team store.
    pub fn current_raw_channels(&self, core: &CoreAppState) -> Result<Vec<RawChannel>> {
        let metadata = self.metadata(core)?;
        Ok(metadata
            .runtime
            .channels
            .keys()
            .filter_map(|id| self.raw_channel(core, id).ok())
            .collect())
    }
    /// A fresh HTTP channel read must reserve its order before the await.
    pub fn begin_channel_read(
        &self,
        core: &mut CoreAppState,
        channel: &str,
    ) -> Result<ChannelRead> {
        if !id(channel) {
            return Err(Unknown::Identity);
        }
        let stamp = Stamp::capture(core, &self.scope.session.team_id, channel);
        let metadata = self.metadata_mut(core)?;
        if metadata
            .runtime
            .stamps
            .get(channel)
            .is_some_and(|previous| previous != &stamp)
        {
            invalidate_channel(metadata, self.configuration(), channel);
        }
        let runtime = &mut metadata.runtime;
        let entry = runtime
            .channels
            .entry(channel.into())
            .or_insert_with(|| (Arc::new(()), None));
        // Newest HTTP request wins; retained raw data remains usable until an
        // actual invalidation or response changes it. Old selection witnesses
        // are retired by this reservation and must be recaptured/rechecked.
        entry.0 = Arc::new(());
        Ok(ChannelRead {
            authority: self.clone(),
            channel: channel.into(),
            revision: entry.0.clone(),
            stamp,
        })
    }
}

#[derive(Clone)]
pub struct RawChannel {
    authority: Authority,
    channel: String,
    revision: Arc<()>,
    raw: Arc<Value>,
}
impl fmt::Debug for RawChannel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RawChannel { redacted }")
    }
}
impl RawChannel {
    pub fn current(&self, core: &CoreAppState) -> bool {
        channel_current(&self.authority, core, &self.channel, &self.revision)
    }
    pub fn value(&self) -> &Value {
        &self.raw
    }
    /// Re-evaluate this same actual raw observation after a necessary role or
    /// feature dependency completes. This does not fabricate a new ingress.
    pub fn observe(&self, core: &mut CoreAppState) -> Result<()> {
        if !self.current(core) {
            return Err(Unknown::Stale);
        }
        self.authority
            .metadata_mut(core)?
            .observe_channel(self.authority.configuration(), &self.raw)
    }
}

pub struct ChannelRead {
    authority: Authority,
    channel: String,
    revision: Arc<()>,
    stamp: Stamp,
}
impl ChannelRead {
    pub fn current(&self, core: &CoreAppState) -> bool {
        self.authority.current(core)
            && self.stamp
                == Stamp::capture(core, &self.authority.scope.session.team_id, &self.channel)
            && self.authority.metadata(core).is_ok_and(|m| {
                m.runtime
                    .channels
                    .get(&self.channel)
                    .is_some_and(|(r, _)| Arc::ptr_eq(r, &self.revision))
            })
    }
    /// Caller validates its query owner and response envelope first. The value
    /// must be the actual complete response channel, never a serialized model.
    /// Policy may remain unknown; raw data is still retained for dependencies.
    pub fn apply(self, core: &mut CoreAppState, raw: Value) -> Result<RawChannel> {
        if !self.current(core) {
            return Err(Unknown::Stale);
        }
        if !raw.is_object() || raw.get("id").and_then(Value::as_str) != Some(&self.channel) {
            invalidate_channel(
                self.authority.metadata_mut(core)?,
                self.authority.configuration(),
                &self.channel,
            );
            return Err(Unknown::Identity);
        }
        let metadata = self.authority.metadata_mut(core)?;
        metadata.runtime.pending_boot = None;
        let _ = metadata.external.channel(&raw);
        metadata
            .runtime
            .channels
            .insert(self.channel.clone(), (Arc::new(()), Some(Arc::new(raw))));
        metadata
            .runtime
            .stamps
            .insert(self.channel.clone(), self.stamp);
        let witness = self.authority.raw_channel(core, &self.channel)?;
        let _ = witness.observe(core);
        Ok(witness)
    }
}
fn channel_current(
    authority: &Authority,
    core: &CoreAppState,
    channel: &str,
    revision: &Arc<()>,
) -> bool {
    authority.metadata(core).is_ok_and(|m| {
        m.runtime.stamps.get(channel)
            == Some(&Stamp::capture(
                core,
                &authority.scope.session.team_id,
                channel,
            ))
            && m.runtime
                .channels
                .get(channel)
                .is_some_and(|(r, _)| Arc::ptr_eq(r, revision))
    })
}
fn invalidate_channel(metadata: &mut Metadata, config: &Configuration, channel: &str) {
    metadata.runtime.pending_boot = None;
    metadata.runtime.stamps.remove(channel);
    metadata
        .runtime
        .channels
        .insert(channel.into(), (Arc::new(()), None));
    // Mask boot inventory fallback as well as the raw witness.
    metadata.observations.insert(
        channel.into(),
        (
            config.clone(),
            Observed::unknown(channel.into()),
            [false; 2],
        ),
    );
}

mod name_policy;
