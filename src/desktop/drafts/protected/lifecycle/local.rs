mod handoff;
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
    pub pending: Arc<std::sync::Mutex<Option<LocalDraft>>>,
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
        let scope = expected.scope().clone();
        self.begin_local(shell, selected, scope, Some(expected))
    }
    /// Read unknown destinations through the exact selected account. The Slot
    /// owns unadmitted input while the actual read/renew is outstanding or fails.
    pub fn open_local_current(
        &mut self,
        shell: &ShellState,
        selected: &Selection,
        key: DraftKey,
    ) -> Result<LocalOpen, durable::Error> {
        let scope = Scope {
            account_id: selected.0.account.id().into(),
            key,
        };
        self.begin_local(shell, selected, scope, None)
    }
    fn begin_local(
        &mut self,
        shell: &ShellState,
        selected: &Selection,
        scope: Scope,
        expected: Option<durable::LocalInventory>,
    ) -> Result<LocalOpen, durable::Error> {
        self.check(shell, selected)?;
        if scope.account_id != selected.0.account.id()
            || scope.key.team.is_empty()
            || scope.key.channel.is_empty()
        {
            return Err(durable::Error::Stale);
        }
        if self.locals.len() >= MAX_LOCAL_LANES || self.routes.len() >= MAX_LOCAL_LANES {
            return Err(durable::Error::Pending);
        }
        if self.locals.values().any(|s| s.scope == scope)
            || self
                .routes
                .contains_key(&(scope.account_id.clone(), scope.key.clone()))
        {
            return Err(durable::Error::ReviewRequired);
        }
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| durable::Error::Storage)?;
        let id = barrier_id();
        let account = selected.0.account.clone();
        let pending = Arc::new(std::sync::Mutex::new(None));
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
                pending: pending.clone(),
            },
        );
        self.routes.insert(
            (scope.account_id.clone(), scope.key.clone()),
            route::Route::Local(id.clone()),
        );
        runtime.spawn(Box::pin(async move {
            let inventory = match expected {
                Some(expected) => Ok(expected),
                None => match &account.owned {
                    Some(route) => route.local(scope.clone()).await,
                    None => Err(durable::Error::Unsupported),
                },
            };
            let result = match inventory {
                Ok(inventory) => LocalWriter::renew(account.clone(), inventory).await,
                Err(error) => Err(error),
            }
            .map(|writer| {
                Arc::new(Lane {
                    id,
                    account,
                    scope,
                    writer,
                })
            });
            // Serialize final pending transfer and publication with submit.
            // There is no await under this lock. A refused draft stays retained.
            let mut pending = pending.lock().unwrap();
            if let (Ok(lane), Some(local)) = (&result, pending.as_ref()) {
                if lane.writer.submit(local).is_ok() {
                    *pending = None;
                }
            }
            completion.finish(result);
        }));
        Ok(opened)
    }
    /// Preserve account-owned in-flight/refused text across shell replacement.
    /// This is presentation only, never a new local inventory or write lease.
    pub fn local_presentations(
        &self,
        account: &Arc<Account>,
    ) -> Result<BTreeMap<DraftKey, LocalDraft>, durable::Error> {
        let mut values = BTreeMap::new();
        for ((owner, key), route) in &self.routes {
            if owner != account.id() {
                continue;
            }
            let route::Route::Local(id) = route else {
                continue;
            };
            let slot = self.locals.get(id).ok_or(durable::Error::Stale)?;
            if !Arc::ptr_eq(&slot.account, account) {
                return Err(durable::Error::Stale);
            }
            let pending = slot.pending.lock().map_err(|_| durable::Error::Storage)?;
            if let Some(value) = pending.as_ref() {
                values.insert(key.clone(), value.clone());
            } else if let Ok(lane) = slot.job.result() {
                let snapshot = lane.writer.snapshot();
                if let Some(value) = snapshot.pending {
                    values.insert(key.clone(), value);
                } else if let Some(inventory) = snapshot.inventory {
                    values.insert(key.clone(), inventory.local().clone());
                }
            }
        }
        Ok(values)
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
