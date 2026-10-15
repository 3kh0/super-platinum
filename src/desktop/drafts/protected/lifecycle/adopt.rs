//! Registration retention only. A result cannot install a renderer route.
use super::super::editor::Editor;
use super::*;
pub(super) struct Slot {
    pub(super) selection: Selection,
    pub lane: Arc<Lane>,
    pub(super) serial: u64,
    pub(super) expected: durable::Review,
    pub(super) job: Arc<Job<Editor>>,
    pub(super) installed: Option<Installed>,
}
#[derive(Clone)]
pub(crate) struct Adoption {
    pub(super) id: String,
    wait: Wait<Editor>,
}
impl Adoption {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub async fn ready(&self) {
        self.wait.ready().await;
    }
}
pub(crate) struct AdoptionResult {
    pub editor: Arc<Editor>,
    pub expected: durable::Review,
    pub lane: Arc<Lane>,
    /// Memory freshness only; NOT native/current durable publication authority.
    pub current: bool,
}
fn clean(lane: &Lane, expected: &durable::Review) -> Result<u64, durable::Error> {
    let snapshot = lane.writer().snapshot();
    if snapshot.running
        || snapshot.pending.is_some()
        || snapshot.error.is_some()
        || !snapshot.failed_inputs.is_empty()
        || !snapshot
            .inventory
            .as_ref()
            .is_some_and(|i| same_local(i.local(), expected.local()))
    {
        return Err(durable::Error::ReviewRequired);
    }
    Ok(snapshot.serial)
}
impl Host {
    pub fn begin_adoption(
        &mut self,
        shell: &ShellState,
        selected: &Selection,
        opened: &LocalOpen,
        expected: durable::Review,
        visit: Visit,
    ) -> Result<Adoption, durable::Error> {
        self.check(shell, selected)?;
        if self.adoptions.len() >= MAX_ADOPTIONS {
            return Err(durable::Error::Pending);
        }
        let lane = self.local(opened)?;
        if !Arc::ptr_eq(&lane.account, &selected.0.account) || expected.scope() != lane.scope() {
            return Err(durable::Error::Stale);
        }
        let view = self
            .discovery
            .entry(shell, &expected.scope().key)
            .ok_or(durable::Error::Stale)?;
        if !matches!(view.inventory(), Inventory::Managed(review) if review.same(&expected)) {
            return Err(durable::Error::Stale);
        }
        if self.adoptions.values().any(|s| Arc::ptr_eq(&s.lane, &lane)) {
            return Err(durable::Error::Pending);
        }
        // Sampled absence only. register_writer's full CAS is the final durable
        // authority; this does not claim atomic absence of unrelated opens.
        if selected
            .0
            .account
            .editors
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .values()
            .any(|e| e.as_ref().is_none_or(|e| e.scope() == expected.scope()))
        {
            return Err(durable::Error::ReviewRequired);
        }
        if expected.row().lineage().basis() != durable::Basis::Unverified
            || !matches!(
                expected.row().state().workflow(),
                super_platinum_core::drafts::sync::Workflow::Idle
            )
            || expected.row().state().consumption().is_some()
            || expected.uncommitted().is_some()
            || expected.blocked().is_some()
            || !expected.alternates().is_empty()
            || expected.row().state().pending().is_some()
        {
            return Err(durable::Error::ReviewRequired);
        }
        let serial = clean(&lane, &expected)?;
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| durable::Error::Storage)?;
        let owner = selected.0.account.owner(expected.scope().key.clone())?;
        if !visit.claim() {
            return Err(durable::Error::Stale);
        }
        let job = Job::new();
        let completion = job.completion();
        let id = barrier_id();
        let request = Adoption {
            id: id.clone(),
            wait: job.wait(),
        };
        self.adoptions.insert(
            id,
            Slot {
                selection: selected.clone(),
                lane: lane.clone(),
                serial,
                expected: expected.clone(),
                job,
                installed: None,
            },
        );
        let selected = selected.clone();
        runtime.spawn(Box::pin(async move {
            let result = async {
                if !selected.0.live.load(Ordering::Acquire) || clean(&lane, &expected)? != serial {
                    return Err(durable::Error::Stale);
                }
                Editor::open_expected(owner, expected).await
            }
            .await;
            completion.finish(result);
        }));
        Ok(request)
    }
    pub fn retained_adoptions(&self, account: &Arc<Account>) -> Vec<Adoption> {
        self.adoptions
            .iter()
            .filter(|(_, s)| Arc::ptr_eq(&s.selection.0.account, account))
            .map(|(id, s)| Adoption {
                id: id.clone(),
                wait: s.job.wait(),
            })
            .collect()
    }
    /// Repeated recovery reads the same Arc/result; it cannot re-register.
    /// No completion is evicted, including Storage/unavailable registration.
    pub fn adoption(
        &self,
        shell: &ShellState,
        request: &Adoption,
    ) -> Result<AdoptionResult, durable::Error> {
        let slot = self
            .adoptions
            .get(&request.id)
            .ok_or(durable::Error::Stale)?;
        let editor = slot.job.result()?;
        let snapshot = editor.snapshot();
        let retired = editor
            .state
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .retired;
        let current = slot.installed.is_none()
            && !retired
            && self.check(shell, &slot.selection).is_ok()
            && clean(&slot.lane, &slot.expected).is_ok_and(|serial| serial == slot.serial)
            && matches!(snapshot.view.inventory(), Inventory::Managed(r) if r.same(&slot.expected))
            && snapshot.serial == 0
            && snapshot.pending.is_none()
            && !snapshot.running
            && snapshot.error.is_none()
            && !snapshot.parked
            && snapshot.projection.is_none()
            && snapshot.failed_inputs.is_empty()
            && !snapshot.choice_retry_available;
        Ok(AdoptionResult {
            editor,
            expected: slot.expected.clone(),
            lane: slot.lane.clone(),
            current,
        })
    }
}
