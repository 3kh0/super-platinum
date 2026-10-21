//! Explicit known-editor repair. The original Canvas witness deliberately has
//! no native member projection: that is precisely what the fresh read repairs.
use super::*;
use crate::directory_member_fetches::known;
use std::{future::Future, task::Poll, time::Duration};
use super_platinum_core::{
    block_actions::name_policy as policy, state::posting_metadata::authority::Authority,
};
#[cfg(test)]
mod tests;

pub(super) struct Failure {
    pub error: Error,
    pub retry_at: Option<Instant>,
    pub blocked: bool,
}
impl Failure {
    pub(super) fn policy(error: Error) -> Self {
        let retry_at = match &error {
            Error::RateLimited {
                retry_after_secs: Some(seconds),
            } => Instant::now().checked_add(std::time::Duration::from_secs(*seconds)),
            _ => None,
        };
        let blocked = matches!(error, Error::RateLimited { .. }) && retry_at.is_none();
        Self {
            error,
            retry_at,
            blocked,
        }
    }
    fn shared(error: Error, status: known::RecoveryStatus) -> Self {
        Self {
            error,
            retry_at: status.retry_at.flatten(),
            blocked: matches!(status.retry_at, Some(None)),
        }
    }
}

struct Witness {
    ticket: Ticket,
    token: Arc<()>,
    root: Message,
    editor: String,
    socket: u64,
    revision: u64,
    authority: Authority,
    membership: policy::Membership,
    enterprise: Option<String>,
}
impl Witness {
    fn capture(
        shell: &ShellState,
        model: &Comments,
        ticket: Ticket,
        root: Message,
        token: Arc<()>,
    ) -> Result<Self, Error> {
        if !slot_current(shell, model, &ticket, &token) {
            return Err(unavailable());
        }
        let editor = source::editor(&root, source::Context::Thread, false)
            .map_err(|_| unavailable())?
            .ok_or_else(unavailable)?
            .id()
            .to_owned();
        let authority =
            Authority::capture(&shell.core, &ticket.owner.scope.team).map_err(|_| unavailable())?;
        let (membership, enterprise) = authority
            .name_policy_membership(&shell.core, policy::OrdinaryClientV2)
            .map_err(|_| unavailable())?;
        let value = Self {
            socket: shell
                .core
                .workspaces
                .get(&ticket.owner.scope.team)
                .ok_or_else(unavailable)?
                .rt_generation,
            revision: crate::app_home::autocomplete::preferences_revision(shell),
            ticket,
            token,
            root,
            editor,
            authority,
            membership,
            enterprise,
        };
        value
            .current(shell, model)
            .then_some(value)
            .ok_or_else(unavailable)
    }
    fn current(&self, shell: &ShellState, model: &Comments) -> bool {
        acquire::current(
            shell,
            model,
            &self.ticket,
            &self.token,
            &self.root,
            self.socket,
            self.revision,
            &self.authority,
            &self.membership,
            &self.enterprise,
        )
    }
}

fn cooling(status: &known::RecoveryStatus, now: Instant) -> bool {
    status
        .retry_at
        .is_some_and(|deadline| deadline.is_none_or(|at| at > now))
}

async fn guarded<T>(
    work: impl Future<Output = Result<T, Error>>,
    current: impl Fn() -> bool,
) -> Result<T, Error> {
    let mut work = std::pin::pin!(work);
    let checked = std::future::poll_fn(|cx| {
        if !current() {
            return Poll::Ready(Err(unavailable()));
        }
        match work.as_mut().poll(cx) {
            Poll::Ready(_) if !current() => Poll::Ready(Err(unavailable())),
            result => result,
        }
    });
    tokio::select! {
        biased;
        _ = async {
            loop {
                if !current() { break; }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        } => Err(unavailable()),
        result = checked => result,
    }
}

pub(super) async fn capture(
    state: Signal<ShellState>,
    model: Signal<Comments>,
    ticket: Ticket,
    root: Message,
    token: Arc<()>,
) -> Result<acquire::Observation, Failure> {
    let witness = Witness::capture(&state.peek(), &model.peek(), ticket, root, token)
        .map_err(Failure::policy)?;
    let needs_repair =
        member::Member::capture(&state.peek(), &witness.ticket.owner, &witness.editor).is_err();
    if needs_repair {
        member::Member::recoverable(&state.peek(), &witness.ticket.owner, &witness.editor)
            .map_err(Failure::policy)?;
        let team = &witness.ticket.owner.scope.team;
        let status = known::recovery_status(&state.peek(), team).map_err(Failure::policy)?;
        if cooling(&status, Instant::now()) {
            let error = status.failure.clone().unwrap_or(Error::RateLimited {
                retry_after_secs: None,
            });
            return Err(Failure::shared(error, status));
        }
        let ids = [witness.editor.clone()];
        let current = |shell: &ShellState| {
            model
                .try_read()
                .is_ok_and(|model| witness.current(shell, &model))
        };
        let result = guarded(known::recover(state, team, &ids, current), || {
            current(&state.peek())
        })
        .await;
        if !model
            .try_read()
            .is_ok_and(|model| witness.current(&state.peek(), &model))
        {
            return Err(Failure::policy(unavailable()));
        }
        // A successful shared aggregate may contain a retained request error
        // and no fresh selected row. Never turn that into policy/name authority.
        if result.is_err()
            || member::Member::capture(&state.peek(), &witness.ticket.owner, &witness.editor)
                .is_err()
        {
            let status = known::recovery_status(&state.peek(), team).map_err(Failure::policy)?;
            let error = status
                .failure
                .clone()
                .or_else(|| result.err())
                .unwrap_or_else(unavailable);
            return Err(Failure::shared(error, status));
        }
    }
    if !model
        .try_read()
        .is_ok_and(|model| witness.current(&state.peek(), &model))
    {
        return Err(Failure::policy(unavailable()));
    }
    // Repaired source is recaptured, then policy is acquired anew. The original
    // Canvas witness remains pinned across both phases; no old policy is reused.
    let observed = acquire::capture(
        state,
        model,
        witness.ticket.clone(),
        witness.root.clone(),
        witness.token.clone(),
    )
    .await
    .map_err(Failure::policy)?;
    if !model
        .try_read()
        .is_ok_and(|model| witness.current(&state.peek(), &model))
    {
        return Err(Failure::policy(unavailable()));
    }
    Ok(observed)
}
