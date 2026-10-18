//! Synchronous source classification while accepted renderer work remains queued.
//! This is neither Ready nor request authority; every carry names its exact queue.
use super::*;

#[derive(Clone)]
pub(in crate::app_home::channel_fetches) struct Continuation {
    owner: Owner,
    identity: Arc<()>,
    publication: Arc<Observation>,
    shadow: ClassificationState,
}
impl Continuation {
    // Accepted enqueue changes Store before advance; the exact old slot receipt
    // still identifies that serial mutation. Callers separately prove ingress.
    fn receipt_current(&self, shell: &ShellState, owner: &Owner) -> bool {
        owner.current(shell)
            && Arc::ptr_eq(&self.owner.context, &owner.context)
            && Arc::ptr_eq(&self.owner.token, &owner.token)
            && shell
                .app_home
                .channel_fetches
                .conditional
                .slots
                .get(&owner.context.key())
                .is_some_and(|r| Arc::ptr_eq(&r.identity, &self.identity))
            && Arc::ptr_eq(
                &self.publication,
                &shell.app_home.channel_fetches.slots[&owner.context.key()].publication,
            )
    }
    fn current(&self, shell: &ShellState, owner: &Owner) -> bool {
        self.receipt_current(shell, owner)
            && observed_current(shell, &owner.context, &self.publication)
    }
}
pub(in crate::app_home::channel_fetches) struct Input {
    prior: Continuation,
    rows: Vec<Value>,
    fresh: Vec<Fresh>,
}
/// No initial inventory seeding: only an existing lazy manager can retain a chain.
pub(in crate::app_home::channel_fetches) fn capture(
    shell: &ShellState,
    owner: &Owner,
) -> Option<Continuation> {
    shell
        .app_home
        .channel_fetches
        .conditional
        .slots
        .get(&owner.context.key())?
        .continuation
        .as_ref()
        .filter(|c| c.current(shell, owner))
        .cloned()
}
pub(super) fn input(
    shell: &ShellState,
    owner: &Owner,
    rows: &[Value],
) -> Result<Option<Input>, Error> {
    let ctx = &owner.context;
    let Some(runtime) = shell
        .app_home
        .channel_fetches
        .conditional
        .slots
        .get(&ctx.key())
    else {
        return Ok(None);
    };
    if !runtime.context.account_current(shell) || !owner.publication_current(shell) {
        return Err(unavailable());
    }
    let prior = if let Some(prior) = capture(shell, owner) {
        prior
    } else {
        let publication = shell.app_home.channel_fetches.slots[&ctx.key()]
            .publication
            .clone();
        let w = &shell.core.workspaces[&ctx.session.team_id];
        let shadow = publication
            .classification(
                &w.directory_channels,
                &ctx.account,
                &ctx.session,
                ctx.epoch,
                GENERATION,
                &w.channels,
            )
            .map_err(|_| unavailable())?;
        Continuation {
            owner: owner.clone(),
            identity: runtime.identity.clone(),
            publication,
            shadow,
        }
    };
    let ids = rows
        .iter()
        .map(|r| r["id"].as_str().map(str::to_owned).ok_or_else(unavailable))
        .collect::<Result<Vec<_>, _>>()?;
    let fresh = fresh(&prior.shadow, &ids)?;
    Ok(Some(Input {
        prior,
        rows: rows.to_vec(),
        fresh,
    }))
}
pub(super) fn processed(
    shell: &mut ShellState,
    owner: &Owner,
    input: Option<Input>,
    facts: &Facts,
    boot: bool,
) -> Result<Option<Continuation>, Error> {
    let Some(mut input) = input else {
        return Ok(None);
    };
    if !input.prior.receipt_current(shell, owner) {
        return Err(unavailable());
    }
    // Same source batch preimage for duplicate IDs; exact receipt facts and phase.
    input
        .prior
        .shadow
        .step(&input.rows, facts, boot)
        .map_err(|_| unavailable())?;
    mark_source_fresh(shell, owner, &input.fresh)?;
    Ok(Some(input.prior))
}
/// Only accepted source action callsites may carry a shadow over publication.
pub(in crate::app_home::channel_fetches) fn advance(
    shell: &mut ShellState,
    owner: &Owner,
    next: Option<Continuation>,
) -> Result<(), Error> {
    let next = next.filter(|c| c.receipt_current(shell, owner));
    super::super::advance(shell, owner)?;
    if let Some(runtime) = shell
        .app_home
        .channel_fetches
        .conditional
        .slots
        .get_mut(&owner.context.key())
    {
        runtime.continuation = next.map(|mut c| {
            c.publication = shell.app_home.channel_fetches.slots[&owner.context.key()]
                .publication
                .clone();
            c
        });
    }
    Ok(())
}
pub(super) fn conditional(shell: &mut ShellState, attempt: &Attempt) -> Result<(), Error> {
    let ctx = &attempt.owner.context;
    let runtime = shell
        .app_home
        .channel_fetches
        .conditional
        .slots
        .get(&ctx.key())
        .ok_or_else(unavailable)?;
    let next = Continuation {
        owner: attempt.owner.clone(),
        identity: runtime.identity.clone(),
        publication: attempt.publication.borrow().clone(),
        shadow: attempt.shadow.borrow().clone(),
    };
    if !next.receipt_current(shell, &attempt.owner) {
        return Err(unavailable());
    }
    advance(shell, &attempt.owner, Some(next))
}
pub(in crate::app_home::channel_fetches) fn clear(shell: &mut ShellState, owner: &Owner) {
    if owner.current(shell)
        && let Some(runtime) = shell
            .app_home
            .channel_fetches
            .conditional
            .slots
            .get_mut(&owner.context.key())
    {
        runtime.continuation = None;
    }
}
/// Preserve only identical accepted pending source across socket replacement.
/// The request's old owner/token/publication are never updated by this method.
pub(in crate::app_home::channel_fetches) fn before_rebind(
    shell: &ShellState,
    context: &Arc<Context>,
    old: &Slot,
) -> Option<Continuation> {
    let runtime = shell
        .app_home
        .channel_fetches
        .conditional
        .slots
        .get(&context.key())?;
    let prior = runtime.continuation.as_ref()?;
    (runtime.context.account_current(shell)
        && prior.owner.context.account_current(shell)
        && Arc::ptr_eq(&runtime.identity, &prior.identity)
        && Arc::ptr_eq(&prior.owner.context, &old.context)
        && Arc::ptr_eq(&prior.owner.token, &old.token)
        && Arc::ptr_eq(&prior.publication, &old.publication)
        && observed_current(shell, context, &prior.publication))
    .then(|| prior.clone())
}
pub(in crate::app_home::channel_fetches) fn rebound(
    shell: &mut ShellState,
    owner: &Owner,
    prior: Option<Continuation>,
) {
    let prior = prior.filter(|c| {
        owner.publication_current(shell) && observed_current(shell, &owner.context, &c.publication)
    });
    if let Some(runtime) = shell
        .app_home
        .channel_fetches
        .conditional
        .slots
        .get_mut(&owner.context.key())
    {
        runtime.continuation = prior
            .filter(|c| Arc::ptr_eq(&c.identity, &runtime.identity))
            .map(|mut c| {
                c.owner = owner.clone();
                c.publication = shell.app_home.channel_fetches.slots[&owner.context.key()]
                    .publication
                    .clone();
                c
            });
    }
}

#[cfg(test)]
pub(super) fn version(shell: &ShellState, owner: &Owner, id: &str) -> Result<u64, Error> {
    capture(shell, owner)
        .ok_or_else(unavailable)?
        .shadow
        .conditional_version(id)
        .map_err(|_| unavailable())
}
