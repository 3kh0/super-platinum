//! Query-owned Directory responses enter the existing shared source queue.
//! A dispatched page keeps its original publication, not a recaptured inventory.
use super::*;
use super_platinum_core::slack::models::Channel;
pub(crate) mod direct;

#[cfg(test)]
mod tests;

struct Receipt {
    origin: Origin,
    requested: Option<String>,
    owner: Owner,
    publication: Arc<Observation>,
    transformations: Arc<members::Transformations>,
    lifecycle: crate::app_home::cfm_lifecycle::Snapshot,
}
impl Receipt {
    fn capture(
        shell: &mut ShellState,
        team: &str,
        current: &impl Fn(&ShellState) -> bool,
    ) -> Result<Self, Error> {
        if !current(shell) {
            return Err(unavailable());
        }
        let context = Context::capture(shell, team)?;
        let lifecycle = crate::app_home::cfm_lifecycle::observed(shell, team)?;
        let slot = shell
            .app_home
            .channel_fetches
            .slots
            .get(&context.key())
            .ok_or_else(unavailable)?;
        let captured = Self {
            origin: Origin::Directory,
            requested: None,
            owner: Owner {
                context: slot.context.clone(),
                token: slot.token.clone(),
            },
            publication: slot.publication.clone(),
            transformations: slot.transformations.clone(),
            lifecycle,
        };
        if !captured.current(shell) {
            return Err(unavailable());
        }
        Ok(captured)
    }
    fn current(&self, shell: &ShellState) -> bool {
        self.owner.publication_current(shell)
            && self.lifecycle.current(shell)
            && shell
                .app_home
                .channel_fetches
                .slots
                .get(&self.owner.context.key())
                .is_some_and(|slot| {
                    Arc::ptr_eq(&self.publication, &slot.publication)
                        && Arc::ptr_eq(&self.transformations, &slot.transformations)
                })
            && observed_current(shell, &self.owner.context, &self.publication)
    }
    /// Acceptance is one serial source action. Member facts and bootstrap phase
    /// are frozen here, before renderer work; no query-wide membership inference.
    fn accept(
        &self,
        shell: &mut ShellState,
        rows: &[Value],
        current: &impl Fn(&ShellState) -> bool,
    ) -> Result<Vec<Channel>, Error> {
        if !current(shell) || !self.current(shell) {
            return Err(unavailable());
        }
        if self
            .requested
            .as_ref()
            .is_some_and(|id| rows.len() != 1 || rows[0]["id"].as_str() != Some(id.as_str()))
        {
            return Err(unavailable());
        }
        let typed = rows
            .iter()
            .cloned()
            .map(serde_json::from_value)
            .collect::<Result<Vec<Channel>, _>>()
            .map_err(|_| unavailable())?;
        if rows.is_empty() {
            return Ok(typed);
        }
        let owner = &self.owner;
        let ctx = &owner.context;
        // All wire rows participate, including nonordinary search results.
        // Missing actual peer facts refuse the page rather than silently filter.
        let receipt = members::receipt(shell, owner, rows)?;
        let fresh = conditional::source_values(shell, owner, rows);
        let w = shell.core.workspaces.get_mut(&ctx.session.team_id).unwrap();
        let accepted = w
            .directory_channels
            .capture_receipt_ingress(
                &self.publication,
                &ctx.account,
                &ctx.session,
                ctx.epoch,
                GENERATION,
                &w.channels,
                self.origin,
                rows.to_vec(),
                receipt.facts.clone(),
            )
            .map_err(|_| unavailable())?;
        let boot = accepted.boot_in_flight();
        // Admission errors cannot leave a partially enqueued page behind.
        let mut store = w.directory_channels.clone();
        store
            .enqueue_ingress(accepted, &ctx.account, &ctx.session, ctx.epoch)
            .map_err(|_| unavailable())?;
        w.directory_channels = store;
        let continuation = match fresh
            .and_then(|input| conditional::source_fresh(shell, owner, input, &receipt.facts, boot))
        {
            Ok(next) => next,
            Err(error) => {
                // Match other accepted source ingress: freshness observation
                // failure never discards already accepted raw channel metadata.
                failed(shell, owner, error);
                None
            }
        };
        conditional::continuation::advance(shell, owner, continuation)?;
        Ok(typed)
    }
}

/// Capture separately before EACH actual dispatched page. Failed/pending/invalid
/// response outcomes remain with the query reader; only decoded rows enter here.
pub(crate) struct Read {
    receipt: Receipt,
    posting: crate::bootstrap::posting_ingress::Ingress,
}
impl Read {
    pub(crate) fn capture(
        mut state: Signal<ShellState>,
        team: &str,
        current: &impl Fn(&ShellState) -> bool,
    ) -> Result<Self, Error> {
        if !current(&state.peek()) {
            return Err(unavailable());
        }
        let receipt = Receipt::capture(&mut state.write(), team, current)?;
        let ctx = &receipt.owner.context;
        let posting =
            crate::bootstrap::posting_ingress::capture(&state, &ctx.transport, &ctx.session);
        Ok(Self { receipt, posting })
    }
    /// Use for transport pre-send/current checks. This receipt authorizes only
    /// the original source publication; it is consumed when the page is accepted.
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        self.receipt.current(shell)
    }
    pub(crate) async fn publish(
        self,
        mut state: Signal<ShellState>,
        rows: Vec<Value>,
        current: impl Fn(&ShellState) -> bool,
    ) -> Result<Arc<canonical::Canonical>, Error> {
        if !current(&state.peek()) || !self.current(&state.peek()) {
            return Err(unavailable());
        }
        let typed = self.receipt.accept(&mut state.write(), &rows, &current)?;
        self.posting.channels(&state, &typed);
        let owner = &self.receipt.owner;
        // Once accepted, this is shared source work. Closing the query does not
        // delete valid cache ingress or cancel another consumer's queued rows.
        drain::wait(state, owner).await?;
        if !current(&state.peek()) || !owner.publication_current(&state.peek()) {
            return Err(unavailable());
        }
        let mut shell = state.write();
        if !current(&shell)
            || !owner.publication_current(&shell)
            || !self.receipt.lifecycle.current(&shell)
            || !Arc::ptr_eq(
                &self.receipt.transformations,
                &shell.app_home.channel_fetches.slots[&owner.context.key()].transformations,
            )
        {
            return Err(unavailable());
        }
        canonical::capture(&mut shell, &owner.context.session.team_id)
    }
}
