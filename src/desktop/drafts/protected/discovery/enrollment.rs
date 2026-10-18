//! Explicit first enrollment from a displayed full local-only inventory.
//! No remote baseline, verification, Editor creation, or composer projection.
use super::*;

pub(crate) struct EnrollmentRequest {
    selection: Arc<Selection>,
    generation: u64,
    expected: durable::LocalInventory,
    owner: Owner,
    visit: Visit,
}
pub(crate) struct PreparedEnrollment {
    request: EnrollmentRequest,
    gate: tokio::sync::OwnedMutexGuard<()>,
}
pub(super) struct Pending {
    selection: Arc<Selection>,
    generation: u64,
    expected: durable::LocalInventory,
    job: Arc<Job<durable::Review>>,
    _owned: Option<Arc<super::super::owned_route::Held<durable::LocalInventory, durable::Review>>>,
}
pub(crate) struct EnrollmentOutcome {
    selection: Arc<Selection>,
    generation: u64,
    expected: durable::LocalInventory,
    result: Result<durable::Review, durable::Error>,
}
impl EnrollmentOutcome {
    pub fn account(&self) -> &str {
        self.selection.account.id()
    }
    pub fn expected(&self) -> &durable::LocalInventory {
        &self.expected
    }
    pub fn result(&self) -> Result<&durable::Review, durable::Error> {
        self.result.as_ref().map_err(|e| *e)
    }
    pub fn current(&self, discovery: &Discovery, shell: &ShellState) -> bool {
        discovery.generation == self.generation
            && discovery
                .selection(shell)
                .is_ok_and(|selected| Arc::ptr_eq(selected, &self.selection))
    }
}
fn no_retained(account: &Account, key: &DraftKey) -> Result<(), durable::Error> {
    if account
        .retained_operations()?
        .iter()
        .any(|op| &op.record().scope.key == key)
    {
        return Err(durable::Error::ReviewRequired);
    }
    let editors = account
        .editors
        .lock()
        .map_err(|_| durable::Error::Storage)?;
    // Editor::open reserves before its detached registration. Empty slots have
    // no scope yet; fail closed for the whole Account while any is unresolved.
    if editors.values().any(Option::is_none) {
        return Err(durable::Error::Pending);
    }
    if editors
        .values()
        .flatten()
        .any(|editor| &editor.scope().key == key)
    {
        // This includes released, parked and duplicate same-scope Editors.
        // Their immutable inputs must be explicitly recovered, never hidden by
        // a first-by-key choice or replaced through first enrollment.
        return Err(durable::Error::ReviewRequired);
    }
    Ok(())
}
impl Discovery {
    /// The captured-account result remains reachable even after a selection
    /// switch or canceled waiter. This never resubmits enrollment.
    pub fn pending_enrollment(&self) -> Option<Wait<durable::Review>> {
        self.enrollment
            .as_ref()
            .map(|pending| Wait::new(pending.job.clone()))
    }
    pub fn enrollment(
        &self,
        shell: &ShellState,
        key: &DraftKey,
        visit: Visit,
    ) -> Result<EnrollmentRequest, durable::Error> {
        if self.enrollment.is_some() {
            return Err(durable::Error::Pending);
        }
        let selected = self.selection(shell)?.clone();
        let view = self
            .entry(shell, key)
            .ok_or(durable::Error::ReviewRequired)?;
        let Inventory::Local(expected) = view.inventory() else {
            return Err(durable::Error::ReviewRequired);
        };
        if !visit.current()
            || view.uncommitted().is_some()
            || expected.uncommitted().is_some()
            || expected.row().is_some()
            || expected.blocked().is_some()
            || !expected.alternates().is_empty()
        {
            return Err(durable::Error::ReviewRequired);
        }
        no_retained(&selected.account, key)?;
        Ok(EnrollmentRequest {
            owner: selected.account.owner(key.clone())?,
            selection: selected,
            generation: self.generation,
            expected: expected.clone(),
            visit,
        })
    }
    /// A completed result belongs to its captured Account even after selection
    /// changes. Taking it does not publish it into either Session or DraftBook.
    pub fn take_enrollment(
        &mut self,
        wait: &Wait<durable::Review>,
    ) -> Result<EnrollmentOutcome, durable::Error> {
        let pending = self.enrollment.as_ref().ok_or(durable::Error::Stale)?;
        if !wait.is(&pending.job) {
            return Err(durable::Error::Stale);
        }
        let result = pending.job.take()?;
        let pending = self.enrollment.take().expect("checked pending receipt");
        Ok(EnrollmentOutcome {
            selection: pending.selection,
            generation: pending.generation,
            expected: pending.expected,
            result,
        })
    }
}
impl EnrollmentRequest {
    pub fn expected(&self) -> &durable::LocalInventory {
        &self.expected
    }
    pub async fn prepare(self) -> Result<PreparedEnrollment, durable::Error> {
        self.owner.account.initialize().await?;
        let gate = self.owner.gate.clone().lock_owned().await;
        let fresh = if let Some(route) = &self.owner.account.owned {
            route.local(self.owner.scope().clone()).await?
        } else {
            account::wait(
                self.owner
                    .client()?
                    .review_local(self.owner.scope().clone()),
            )
            .await?
        };
        if !self.visit.current() || !fresh.same(&self.expected) {
            return Err(durable::Error::Stale);
        }
        no_retained(&self.selection.account, &self.owner.scope().key)?;
        Ok(PreparedEnrollment {
            request: self,
            gate,
        })
    }
}
impl PreparedEnrollment {
    pub fn expected(&self) -> &durable::LocalInventory {
        &self.request.expected
    }
    /// Final caller admission is synchronous after every await and before the
    /// typed FIFO command is enqueued. Future renderer integration must supply
    /// its real native admission proof; this inactive storage adapter does not
    /// equate a Rust local value with undelivered native input.
    pub fn enqueue<G>(
        self,
        discovery: &mut Discovery,
        shell: &ShellState,
        current: G,
    ) -> Result<Wait<durable::Review>, durable::Error>
    where
        G: FnOnce(&ShellState) -> bool,
    {
        let PreparedEnrollment { request, gate } = self;
        let selected = discovery.selection(shell)?;
        if !Arc::ptr_eq(selected, &request.selection)
            || discovery.generation != request.generation
            || discovery.status != Status::Ready
            || discovery.enrollment.is_some()
            || !matches!(discovery.entry(shell, &request.owner.scope().key).map(View::inventory), Some(Inventory::Local(local)) if local.same(&request.expected))
        {
            return Err(durable::Error::Stale);
        }
        no_retained(&selected.account, &request.owner.scope().key)?;
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| durable::Error::Storage)?;
        let generation = discovery
            .generation
            .checked_add(1)
            .expect("discovery generation exhausted");
        if !current(shell) || !request.visit.claim() {
            return Err(durable::Error::Stale);
        }
        // The command itself atomically compares the full local clock,
        // generation and all inventory. A later queued write cannot be erased.
        let owned = request.owner.account.owned.as_ref().map(|route| {
            super::super::owned_route::Held::new(route.operations.enroll(request.expected.clone()))
        });
        let reply = if owned.is_none() {
            Some(
                request
                    .owner
                    .client()?
                    .enroll_local(request.owner.scope().clone(), request.expected.clone()),
            )
        } else {
            None
        };
        let job = Job::new();
        discovery.enrollment = Some(Pending {
            selection: request.selection,
            generation,
            expected: request.expected,
            job: job.clone(),
            _owned: owned.clone(),
        });
        discovery.status = Status::NeedsLoad;
        discovery.generation = generation;
        let completion = job.completion();
        runtime.spawn(Box::pin(async move {
            let _gate = gate;
            completion.finish(match owned {
                Some(held) => held.observed_success().await,
                None => account::wait(reply.expect("legacy enrollment reply")).await,
            });
        }));
        Ok(Wait::new(job))
    }
}
