//! List Texty context from the original root, canonical channel and observed
//! per-user membership. A first-N hint is never a channel membership census.
use super::*;
use crate::{app_home::channel_fetches::canonical, scheduled::Context};
use serde_json::json;
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
use super_platinum_core::slack::api::custom::texty::list_membership as membership;
mod channel;
mod read;
#[cfg(test)]
mod tests;

#[derive(Clone, Default)]
pub(super) struct State {
    slots: BTreeMap<String, Slot>,
}
#[derive(Clone)]
struct Slot {
    owner: Arc<Fence>,
    identity: Arc<()>,
    revision: u64,
    attempted: Option<Instant>,
    cache: membership::Cache,
}
struct Fence {
    context: Arc<Context>,
    account: String,
    cookie: String,
    socket: u64,
}
impl Fence {
    fn capture(shell: &ShellState) -> Result<Self, Error> {
        let context = Arc::new(Context::capture(shell).ok_or_else(unavailable)?);
        Ok(Self {
            account: shell.core.active_account.clone().ok_or_else(unavailable)?,
            cookie: shell
                .core
                .session
                .as_ref()
                .ok_or_else(unavailable)?
                .d_cookie
                .clone(),
            socket: shell
                .core
                .workspaces
                .get(&context.session.team_id)
                .ok_or_else(unavailable)?
                .rt_generation,
            context,
        })
    }
    fn current(&self, shell: &ShellState) -> bool {
        let c = &self.context;
        c.current(shell)
            && shell.core.active_account.as_deref() == Some(self.account.as_str())
            && shell.core.session.as_ref().is_some_and(|session| {
                session.d_cookie == self.cookie
                    && session
                        .workspaces
                        .get(&c.session.team_id)
                        .is_some_and(|actual| {
                            crate::emoji_preferences::same_session(actual, &c.session)
                        })
            })
            && shell
                .core
                .workspaces
                .get(&c.session.team_id)
                .is_some_and(|w| {
                    w.rt_generation == self.socket
                        && w.team_id == c.session.team_id
                        && w.self_user_id == c.session.user_id
                })
    }
}

pub(super) struct Observation {
    owner: Arc<Fence>,
    binding: draft::Binding,
    canonical: Arc<canonical::Canonical>,
    root: Value,
    slot: Arc<()>,
    revision: u64,
    value: Value,
}
impl Observation {
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        self.owner.current(shell)
            && self.binding.current(shell)
            && self.canonical.current(shell)
            && super::super::member_root::snapshot(shell).is_ok_and(|r| r == self.root)
            && shell
                .slack_lists
                .comments
                .completion
                .member_context
                .slots
                .get(self.value["channelId"].as_str().unwrap_or_default())
                .is_some_and(|s| {
                    Arc::ptr_eq(&s.identity, &self.slot) && s.revision == self.revision
                })
    }
    pub(super) fn value(&self) -> &Value {
        &self.value
    }
}

fn due(attempted: Option<Instant>, now: Instant) -> bool {
    attempted.is_none_or(|t| {
        now.saturating_duration_since(t)
            > Duration::from_millis(membership::ATTEMPT_INTERVAL_MILLIS)
    })
}
fn truth(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::String(s) => !s.is_empty(),
        Value::Number(n) => n.as_f64().is_some_and(|n| n != 0.),
        _ => true,
    }
}
fn context_value(root: &Value, channel: &Value, cache: &membership::Cache) -> Value {
    let members: serde_json::Map<_, _> = cache
        .entries()
        .iter()
        .map(|(id, entry)| {
            let (known, member) = match entry {
                membership::Entry::Unknown => (false, false),
                membership::Entry::Known(v) => (true, *v),
            };
            (id.clone(), json!({"isKnown":known,"isMember":member}))
        })
        .collect();
    let mut value = root.clone();
    value["channel"] = channel.clone();
    value["channelSelectors"] = json!({
        "isImOrMpim":truth(&channel["is_im"]) || truth(&channel["is_mpim"]),
        "unknownOrNonexistent":truth(&channel["isUnknown"]) || truth(&channel["isNonExistent"]),
        "externallyShared":truth(&channel["is_ext_shared"])});
    // QwsR's assistant selector is used only by the IM/MPIM branch. prepare
    // refuses that branch until its actual app observation exists; this null
    // denotes an inactive input, not observed absence of a workspace assistant.
    value["assistantUserId"] = Value::Null;
    value["membershipObserved"] = Value::Bool(true);
    value["membership"] = Value::Object(members);
    value
}

pub(super) async fn prepare(
    mut state: Signal<ShellState>,
    binding: &draft::Binding,
    current: impl Fn(&ShellState) -> bool,
) -> Result<Observation, Error> {
    let (owner, root) = {
        let shell = state.peek();
        if !binding.current(&shell) || !current(&shell) {
            return Err(unavailable());
        }
        (
            Arc::new(Fence::capture(&shell)?),
            super::super::member_root::snapshot(&shell)?,
        )
    };
    let id = root["channelId"]
        .as_str()
        .ok_or_else(unavailable)?
        .to_owned();
    let original = |s: &ShellState| {
        owner.current(s)
            && binding.current(s)
            && current(s)
            && super::super::member_root::snapshot(s).is_ok_and(|r| r == root)
    };
    let source = channel::acquire(state, &owner, &id, &original).await?;
    let row = source
        .row(&id)
        .cloned()
        .unwrap_or_else(|| json!({"id":id,"isUnknown":true}));
    if truth(&row["is_im"]) || truth(&row["is_mpim"]) {
        return Err(unavailable());
    }
    let admitted = |s: &ShellState| original(s) && source.current(s);
    if !admitted(&state.peek()) {
        return Err(unavailable());
    }
    let (slot, fetch) = {
        let mut shell = state.write();
        if !admitted(&shell) {
            return Err(unavailable());
        }
        let replace = shell
            .slack_lists
            .comments
            .completion
            .member_context
            .slots
            .get(&id)
            .is_none_or(|s| !s.owner.current(&shell));
        if replace {
            let cache = membership::Cache::capture(&owner.context.session, &id)?;
            shell
                .slack_lists
                .comments
                .completion
                .member_context
                .slots
                .insert(
                    id.clone(),
                    Slot {
                        owner: owner.clone(),
                        identity: Arc::new(()),
                        revision: 0,
                        attempted: None,
                        cache,
                    },
                );
        }
        let slot = shell
            .slack_lists
            .comments
            .completion
            .member_context
            .slots
            .get_mut(&id)
            .unwrap();
        let fetch =
            !truth(&row["is_im"]) && !truth(&row["is_mpim"]) && due(slot.attempted, Instant::now());
        if fetch {
            slot.attempted = Some(Instant::now());
        }
        (slot.identity.clone(), fetch)
    };
    if fetch {
        read::prefetch(state, &owner, &id, &row, &slot, &admitted).await?;
    }
    let shell = state.peek();
    if !admitted(&shell) {
        return Err(unavailable());
    }
    let stored = shell
        .slack_lists
        .comments
        .completion
        .member_context
        .slots
        .get(&id)
        .filter(|s| Arc::ptr_eq(&s.identity, &slot))
        .ok_or_else(unavailable)?;
    let value = context_value(&root, &row, &stored.cache);
    Ok(Observation {
        owner,
        binding: binding.clone(),
        canonical: source,
        root,
        slot,
        revision: stored.revision,
        value,
    })
}
