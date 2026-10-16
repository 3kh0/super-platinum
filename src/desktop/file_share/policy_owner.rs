//! Selected-destination raw posting facts. No write or result-custody authority.
//! This owner must survive dialog close within its exact credential scope.
use crate::state::ShellState;
use serde_json::Value;
use std::sync::Arc;
use super_platinum_core::slack::{
    Error, PreparedRequest, api,
    events::RtEvent,
    file_share_policy::{self as policy, Eligibility, PostingFacts},
    models::User,
};
#[cfg(test)]
use super_platinum_core::{config::WorkspaceSession, slack::Transport};
mod scope;
use scope::Scope;
const RAW_LIMIT: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refusal {
    Owner,
    Changed,
    Pending,
    Envelope,
    Limit,
    Exhausted,
}

/// One channel, at most one in-flight read and one bounded raw info object.
/// Desktop registry caps these at 32, reclaiming idle owners under pressure.
pub(crate) struct Owner {
    identity: Arc<()>,
    scope: Scope,
    channel: String,
    serial: Option<u64>,
    pending: Option<u64>,
    facts: PostingFacts,
    // Provenance of retained facts survives clearing the presentation snapshot.
    // A later read cannot silently rebase old facts onto newer local metadata.
    fact_observation: Option<(u64, Value, Value)>,
    latest: Option<Snapshot>,
}
/// Owned original read context; cannot be created from a raw endpoint response.
pub(crate) struct Read {
    identity: Arc<()>,
    scope: Scope,
    serial: u64,
    channel: String,
    revision: u64,
    channel_snapshot: Value,
    member_snapshot: Value,
}
#[derive(Clone)]
pub(crate) struct Snapshot {
    identity: Arc<()>,
    serial: u64,
    revision: u64,
    channel_snapshot: Value,
    member_snapshot: Value,
    raw: Arc<Value>,
    facts: PostingFacts,
    eligibility: Eligibility,
}
impl Snapshot {
    pub(crate) fn eligibility(&self) -> Eligibility {
        self.eligibility
    }
    pub(crate) fn raw(&self) -> &Value {
        &self.raw
    }
}
impl Owner {
    pub(crate) fn capture(shell: &ShellState, channel: &str) -> Option<Self> {
        // No lookup/create of a user, new DM or channel membership mutation.
        let scope = Scope::capture(shell)?;
        let c = shell
            .core
            .workspaces
            .get(&scope.context.session.team_id)?
            .channels
            .get(channel)?;
        if c.id != channel
            || !(9..=64).contains(&channel.len())
            || !matches!(channel.as_bytes().first(), Some(b'C' | b'G'))
            || !channel
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
            || !(c.is_channel || c.is_private || c.is_group)
            || c.is_im
            || c.is_mpim
            || c.extra.get("is_member").and_then(Value::as_bool) != Some(true)
        {
            return None;
        }
        Some(Self {
            identity: Arc::new(()),
            scope,
            channel: channel.into(),
            serial: Some(0),
            pending: None,
            facts: PostingFacts::new(channel.into()),
            fact_observation: None,
            latest: None,
        })
    }
    /// Retained map reuse only; not current policy or write admission. This
    /// performs no read, serial change or canonical snapshot replacement.
    pub(crate) fn same_scope(&self, shell: &ShellState, channel: &str) -> bool {
        self.channel == channel && self.scope.current(shell)
    }
    /// Pressure-only eviction. The owner and its internal latest snapshot are
    /// the only permitted holders. Reads, returned/cloned snapshots and queued
    /// canceled tickets retain identity even after scope invalidation.
    pub(super) fn reclaimable(&self) -> bool {
        self.pending.is_none()
            && Arc::strong_count(&self.identity) == 1 + usize::from(self.latest.is_some())
    }
    fn next(&mut self) -> Result<u64, Refusal> {
        self.serial = self.serial.and_then(|v| v.checked_add(1));
        self.latest = None;
        self.serial.ok_or(Refusal::Exhausted)
    }
    pub(crate) fn begin(&mut self, shell: &ShellState) -> Result<Read, Refusal> {
        if !self.scope.current(shell) {
            return Err(Refusal::Owner);
        }
        if self.pending.is_some() {
            return Err(Refusal::Pending);
        }
        let observed = self.observed(shell)?;
        if self
            .fact_observation
            .as_ref()
            .is_some_and(|old| old != &observed)
        {
            // Conservative invalidation, not a permission update from a
            // default-filled Channel/User. Only fresh raw info may re-seed.
            self.facts = PostingFacts::new(self.channel.clone());
            self.fact_observation = None;
        }
        let (revision, channel_snapshot, member_snapshot) = observed;
        let serial = self.next()?;
        self.pending = Some(serial);
        Ok(Read {
            identity: self.identity.clone(),
            scope: self.scope.clone(),
            serial,
            channel: self.channel.clone(),
            revision,
            channel_snapshot,
            member_snapshot,
        })
    }
    /// A genuine failed read retires only its ticket. Never convert it into raw
    /// success, erase known restrictions, or keep an earlier ready snapshot.
    pub(crate) fn failed(&mut self, read: &Read) {
        if Arc::ptr_eq(&self.identity, &read.identity) && self.pending == Some(read.serial) {
            self.pending = None;
            self.latest = None;
        }
    }
    pub(crate) fn finish(
        &mut self,
        shell: &ShellState,
        read: Read,
        response: Value,
    ) -> Result<Snapshot, Refusal> {
        if !Arc::ptr_eq(&self.identity, &read.identity) || self.pending != Some(read.serial) {
            return Err(Refusal::Changed);
        }
        self.pending = None;
        self.latest = None;
        if !self.scope.current(shell) || !read.scope.current(shell) {
            return Err(Refusal::Owner);
        }
        let observed = self.observed(shell)?;
        if observed
            != (
                read.revision,
                read.channel_snapshot.clone(),
                read.member_snapshot.clone(),
            )
        {
            return Err(Refusal::Changed);
        }
        if !bounded(&response) {
            return Err(Refusal::Limit);
        }
        if response.get("ok") != Some(&Value::Bool(true))
            || response.get("error").is_some_and(|v| !v.is_null())
        {
            return Err(Refusal::Envelope);
        }
        let raw = response
            .get("channel")
            .filter(|v| v.is_object())
            .filter(|v| v["id"].as_str() == Some(self.channel.as_str()))
            .ok_or(Refusal::Envelope)?;
        let member: User =
            serde_json::from_value(read.member_snapshot.clone()).map_err(|_| Refusal::Changed)?;
        if !self.facts.observe(raw, Some(&member)) {
            return Err(Refusal::Envelope);
        }
        let eligibility = policy::destination(raw, &self.facts, None);
        self.fact_observation = Some((
            read.revision,
            read.channel_snapshot.clone(),
            read.member_snapshot.clone(),
        ));
        let snapshot = Snapshot {
            identity: self.identity.clone(),
            serial: read.serial,
            revision: read.revision,
            channel_snapshot: read.channel_snapshot,
            member_snapshot: read.member_snapshot,
            raw: Arc::new(raw.clone()),
            facts: self.facts.clone(),
            eligibility,
        };
        self.latest = Some(snapshot.clone());
        Ok(snapshot)
    }
    /// Recheck immediately before each new write admission, including after
    /// journal reservation. Never call this to discard a dispatched outcome.
    pub(crate) fn current(&self, shell: &ShellState, snapshot: &Snapshot) -> bool {
        self.scope.current(shell)
            && self.pending.is_none()
            && Arc::ptr_eq(&self.identity, &snapshot.identity)
            && self.serial == Some(snapshot.serial)
            && self.facts == snapshot.facts
            && self
                .latest
                .as_ref()
                .is_some_and(|s| Arc::ptr_eq(&s.raw, &snapshot.raw))
            && self.observed(shell).is_ok_and(|current| {
                current
                    == (
                        snapshot.revision,
                        snapshot.channel_snapshot.clone(),
                        snapshot.member_snapshot.clone(),
                    )
            })
    }
    fn observed(&self, shell: &ShellState) -> Result<(u64, Value, Value), Refusal> {
        let workspace = shell
            .core
            .workspaces
            .get(&self.scope.context.session.team_id)
            .ok_or(Refusal::Owner)?;
        let channel = workspace
            .channels
            .get(&self.channel)
            .ok_or(Refusal::Changed)?;
        if channel.extra.get("is_member").and_then(Value::as_bool) != Some(true)
            || channel.is_im
            || channel.is_mpim
        {
            return Err(Refusal::Changed);
        }
        let member = workspace
            .users
            .get(&self.scope.context.session.user_id)
            .ok_or(Refusal::Changed)?;
        // Serialization here is comparison evidence only. It NEVER seeds policy.
        let channel = serde_json::to_value(channel).map_err(|_| Refusal::Changed)?;
        let member = serde_json::to_value(member).map_err(|_| Refusal::Changed)?;
        if !bounded(&channel) || !bounded(&member) {
            return Err(Refusal::Limit);
        }
        Ok((
            workspace.channel_lifecycle.channel_revision(&self.channel),
            channel,
            member,
        ))
    }
    /// Called at the already epoch/Transport/generation-guarded realtime ingress
    /// before normal reducers. channel_updated is not covered by ChannelLifecycle.
    /// Its permission changes retire old grants until fresh explicit raw info.
    pub(crate) fn event(
        &mut self,
        shell: &ShellState,
        team: &str,
        generation: u64,
        event: &RtEvent,
    ) {
        if !self.scope.current(shell)
            || team != self.scope.context.session.team_id
            || generation != self.scope.generation
        {
            return;
        }
        let RtEvent::Unknown(raw) = event else { return };
        if raw.kind != "channel_updated" {
            return;
        }
        let target = raw.rest.get("channel").and_then(Value::as_str);
        let targets = raw.rest.get("channels").and_then(Value::as_array);
        let names_self = target == Some(self.channel.as_str())
            || targets.is_some_and(|a| a.iter().any(|v| v.as_str() == Some(self.channel.as_str())));
        let malformed_target = (target.is_none() && targets.is_none())
            || targets.is_some_and(|a| a.iter().any(|v| !v.is_string()));
        if !names_self && !malformed_target {
            return;
        }
        let updates = raw.rest.get("updates");
        let reset = updates.and_then(Value::as_object).is_none_or(|o| {
            o.keys().any(|key| {
                key == "permissions"
                    || key == "is_read_only"
                    || key == "is_thread_only"
                    || key == "is_ext_shared"
                    || key.starts_with("properties")
            })
        });
        self.invalidate(reset);
    }
    /// Required on disconnect/reconnect or invalidated permission coverage.
    /// This changes no durable write state and starts no read or timer.
    pub(crate) fn invalidate(&mut self, reset_facts: bool) {
        let _ = self.next();
        self.pending = None;
        if reset_facts {
            self.facts = PostingFacts::new(self.channel.clone());
            self.fact_observation = None;
        }
    }
}
impl Read {
    pub(crate) fn request(&self) -> PreparedRequest {
        api::conversations_info(
            &self.scope.context.client,
            &self.scope.context.session,
            self.channel.clone(),
        )
    }
    /// Caller checks its dialog/source owner before invoking and after await.
    /// Existing read transport honors health/timeout/Retry-After. No write here.
    pub(crate) async fn fetch(&self) -> Result<Value, Error> {
        self.scope.context.channel_info(&self.channel).await
    }
}
fn bounded(value: &Value) -> bool {
    fn depth(v: &Value, n: usize) -> bool {
        n <= 32
            && match v {
                Value::Array(a) => a.iter().all(|v| depth(v, n + 1)),
                Value::Object(o) => o.values().all(|v| depth(v, n + 1)),
                _ => true,
            }
    }
    struct Count(usize);
    impl std::io::Write for Count {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0 = self.0.saturating_add(b.len());
            if self.0 > RAW_LIMIT {
                Err(std::io::ErrorKind::FileTooLarge.into())
            } else {
                Ok(b.len())
            }
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    depth(value, 0) && serde_json::to_writer(Count(0), value).is_ok()
}
#[cfg(test)]
mod tests;
