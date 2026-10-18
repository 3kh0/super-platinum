//! Exact existing hosted set reads; no recipient creation or write capability.
use super::*;
use super_platinum_core::drafts::store::protected::{self as p, owned_store as o};
mod picker;
pub(crate) use picker::{Choice, Page, Picker};

pub(crate) struct Observed {
    context: admission::Context,
    generation: u64,
    revision: u64,
    channel: String,
    input: o::HostedInput,
    policy: policy_owner::Snapshot,
}
impl Observed {
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        self.context.current(shell)
            && shell
                .core
                .workspaces
                .get(&self.context.key.team)
                .is_some_and(|w| w.rt_generation == self.generation)
            && self.input.sources().iter().all(|raw| {
                raw["id"].as_str().is_some_and(|id| {
                    shell
                        .core
                        .workspaces
                        .get(&self.context.key.team)
                        .is_some_and(|w| !w.deleted_files.contains(id))
                        && !crate::file_events::changed_since(
                            shell,
                            &self.context.key.team,
                            id,
                            self.revision,
                        )
                })
            })
            && shell
                .file_share
                .policies
                .get(&self.channel)
                .is_some_and(|owner| owner.current(shell, &self.policy))
    }
    /// The final caller must also fence its original Editor, native lease and
    /// socket. This value cannot manufacture those independent observations.
    pub(crate) fn fresh(
        &self,
        shell: &ShellState,
        input: Arc<o::HostedPrepared>,
    ) -> Result<o::HostedFresh, p::Error> {
        if !self.current(shell)
            || input.input() != &self.input
            || input.review().scope().account_id != self.context.key.account
            || input.review().scope().key.team != self.context.key.team
            || input.review().scope().key.channel != self.channel
        {
            return Err(p::Error::Stale);
        }
        self.policy
            .hosted_fresh(input, &self.input)
            .map_err(|_| p::Error::Stale)
    }
}

pub(crate) async fn observe(
    mut state: Signal<ShellState>,
    input: &o::HostedInput,
) -> Result<Observed, p::Error> {
    let (context, generation, revision, channel) = {
        let shell = state.try_peek().map_err(|_| p::Error::Stale)?;
        let context = admission::Context::capture(&shell).ok_or(p::Error::Stale)?;
        let generation = shell
            .core
            .workspaces
            .get(&context.key.team)
            .ok_or(p::Error::Stale)?
            .rt_generation;
        let [destination] = input.snapshot().destinations.as_slice() else {
            return Err(p::Error::Unsupported);
        };
        let channel = destination["channel_id"]
            .as_str()
            .ok_or(p::Error::Unsupported)?
            .to_owned();
        if input.sources().is_empty() || input.sources().len() > 32 {
            return Err(p::Error::Unsupported);
        }
        (
            context,
            generation,
            crate::file_events::snapshot_revision(&shell),
            channel,
        )
    };
    let current = |shell: &ShellState| {
        context.current(shell)
            && shell
                .core
                .workspaces
                .get(&context.key.team)
                .is_some_and(|w| w.rt_generation == generation)
            && input.sources().iter().all(|raw| {
                raw["id"].as_str().is_some_and(|id| {
                    shell
                        .core
                        .workspaces
                        .get(&context.key.team)
                        .is_some_and(|w| !w.deleted_files.contains(id))
                        && !crate::file_events::changed_since(
                            shell,
                            &context.key.team,
                            id,
                            revision,
                        )
                })
            })
    };
    for expected in input.sources() {
        if !current(&state.peek()) {
            return Err(p::Error::Stale);
        }
        let id = expected["id"].as_str().ok_or(p::Error::Unsupported)?;
        let response = context.file_info(id).await.map_err(|_| p::Error::Gap)?;
        if !current(&state.peek()) {
            return Err(p::Error::Stale);
        }
        // Preserve the entire selected files.info object, not a default-filled
        // File projection. Any changed source must be explicitly reinserted.
        if response.get("ok") != Some(&Value::Bool(true))
            || response.get("error").is_some_and(|v| !v.is_null())
            || response.get("file") != Some(expected)
        {
            return Err(p::Error::ReviewRequired);
        }
    }
    let (read, custody) = {
        let mut shell = state.write();
        if !current(&shell) {
            return Err(p::Error::Stale);
        }
        let mut share = std::mem::take(&mut shell.file_share);
        let read = share.begin_policy(&shell, &channel);
        let custody = share.custody.clone();
        shell.file_share = share;
        (read.map_err(|_| p::Error::Stale)?, custody)
    };
    let mut ticket = delivery::Ticket {
        custody,
        channel: channel.clone(),
        read: Some(read),
    };
    let response = ticket
        .read
        .as_ref()
        .ok_or(p::Error::Stale)?
        .fetch()
        .await
        .map_err(|_| p::Error::Gap)?;
    let policy = {
        let mut shell = state.write();
        if !current(&shell) {
            return Err(p::Error::Stale);
        }
        let mut share = std::mem::take(&mut shell.file_share);
        let result = share
            .policies
            .get_mut(&channel)
            .ok_or(p::Error::Stale)
            .and_then(|owner| {
                owner
                    .finish_response(&shell, ticket.read.take().ok_or(p::Error::Stale)?, response)
                    .map_err(|_| p::Error::Stale)
            });
        shell.file_share = share;
        result?
    };
    let value = Observed {
        context,
        generation,
        revision,
        channel,
        input: input.clone(),
        policy,
    };
    if !value.current(&state.peek()) {
        return Err(p::Error::Stale);
    }
    Ok(value)
}
