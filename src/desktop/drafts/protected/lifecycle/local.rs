use super::super::local_writer::LocalWriter;
use super::*;
pub(crate) struct Lane {
    id: String,
    pub(super) account: Arc<Account>,
    scope: Scope,
    writer: Arc<LocalWriter>,
}
impl Lane {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn scope(&self) -> &Scope {
        &self.scope
    }
    pub fn writer(&self) -> &Arc<LocalWriter> {
        &self.writer
    }
}
pub(super) struct Slot {
    pub account: Arc<Account>,
    pub scope: Scope,
    pub job: Arc<Job<Lane>>,
}
#[derive(Clone)]
pub(crate) struct LocalOpen {
    pub(super) id: String,
    wait: Wait<Lane>,
}
impl LocalOpen {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub async fn ready(&self) {
        self.wait.ready().await;
    }
}
impl Host {
    pub fn open_local(
        &mut self,
        shell: &ShellState,
        selected: &Selection,
        expected: durable::LocalInventory,
    ) -> Result<LocalOpen, durable::Error> {
        self.check(shell, selected)?;
        if expected.scope().account_id != selected.0.account.id() {
            return Err(durable::Error::Stale);
        }
        if self.locals.len() >= MAX_LOCAL_LANES || self.routes.len() >= MAX_LOCAL_LANES {
            return Err(durable::Error::Pending);
        }
        // Never silently replace a lease or pick the first same-scope owner.
        if self.locals.values().any(|s| s.scope == *expected.scope()) {
            return Err(durable::Error::ReviewRequired);
        }
        if self.routes.contains_key(&(
            expected.scope().account_id.clone(),
            expected.scope().key.clone(),
        )) {
            return Err(durable::Error::ReviewRequired);
        }
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| durable::Error::Storage)?;
        let id = barrier_id();
        let account = selected.0.account.clone();
        let scope = expected.scope().clone();
        let job = Job::new();
        let completion = job.completion();
        let opened = LocalOpen {
            id: id.clone(),
            wait: job.wait(),
        };
        self.locals.insert(
            id.clone(),
            Slot {
                account: account.clone(),
                scope: scope.clone(),
                job,
            },
        );
        self.routes.insert(
            (scope.account_id.clone(), scope.key.clone()),
            route::Route::Local(id.clone()),
        );
        runtime.spawn(Box::pin(async move {
            let result = LocalWriter::renew(account.clone(), expected)
                .await
                .map(|writer| {
                    Arc::new(Lane {
                        id,
                        account,
                        scope,
                        writer,
                    })
                });
            completion.finish(result);
        }));
        Ok(opened)
    }
    /// Lookup is by the retained identity, never by whichever lane has this key.
    pub fn local(&self, opened: &LocalOpen) -> Result<Arc<Lane>, durable::Error> {
        self.locals
            .get(&opened.id)
            .ok_or(durable::Error::Stale)?
            .job
            .result()
    }
    pub fn retained_locals(&self, account: &Arc<Account>) -> Vec<LocalOpen> {
        self.locals
            .iter()
            .filter(|(_, s)| Arc::ptr_eq(&s.account, account))
            .map(|(id, s)| LocalOpen {
                id: id.clone(),
                wait: s.job.wait(),
            })
            .collect()
    }
    /// Cancellation before completion leaves the slot retained for retry.
    /// Once removal succeeds this handle is stale. No extra job slot is needed,
    /// including when the Host is at capacity.
    pub async fn retire_local(&mut self, opened: &LocalOpen) -> Result<(), durable::Error> {
        let lane = self.local(opened)?;
        if self.adoptions.values().any(|s| Arc::ptr_eq(&s.lane, &lane)) {
            return Err(durable::Error::ReviewRequired);
        }
        lane.writer.retire_clean().await?;
        self.locals.remove(&opened.id);
        self.routes
            .remove(&(lane.scope.account_id.clone(), lane.scope.key.clone()));
        Ok(())
    }
}
