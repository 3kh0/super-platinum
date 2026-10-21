//! Account-local edit custody. Disk records never carry native or send authority.
use super::*;
use super_platinum_core::slack_lists::comments::{
    OwnEditAccepted,
    journal::own_edit::{EditInventory, EditPhase, EditResolution},
};

pub(super) struct Durable {
    disk: storage::Disk,
    state: tokio::sync::Mutex<Saved>,
}
struct Saved {
    inventory: EditInventory,
    pending: Option<(String, Outcome, i64)>,
    publication: Option<OwnEditAccepted>,
}
#[derive(Clone)]
pub(super) enum Outcome {
    NotSent,
    Rejected(Value),
    RateLimited,
    Uncertain(Option<Value>),
    Confirmed(OwnEditAccepted),
    Observed(Page),
}
impl Durable {
    pub(super) async fn load(disk: storage::Disk, ts: String) -> Result<Arc<Self>, Error> {
        let scope = disk.scope.clone();
        let inventory = disk.work(move |j| j.edit_inventory(&scope, &ts)).await?;
        Ok(Arc::new(Self {
            disk,
            state: tokio::sync::Mutex::new(Saved {
                inventory,
                pending: None,
                publication: None,
            }),
        }))
    }
    pub(super) async fn inventory(&self) -> EditInventory {
        self.state.lock().await.inventory.clone()
    }
    async fn settle_pending(&self, saved: &mut Saved) -> Result<(), Error> {
        let Some((id, outcome, retry_at)) = saved.pending.clone() else {
            return Ok(());
        };
        let retry_at = u64::try_from(retry_at).map_err(|_| unavailable())?;
        let scope = saved.inventory.scope().clone();
        let ts = saved.inventory.ts().to_owned();
        let publication = match &outcome {
            Outcome::Confirmed(value) => Some(value.clone()),
            _ => None,
        };
        let inventory = self
            .disk
            .work(move |j| {
                let resolution = match &outcome {
                    Outcome::NotSent => EditResolution::NotRequested,
                    Outcome::Rejected(raw) => EditResolution::Rejected(raw),
                    Outcome::RateLimited => EditResolution::RateLimited,
                    Outcome::Uncertain(raw) => EditResolution::Uncertain(raw.as_ref()),
                    Outcome::Confirmed(accepted) => EditResolution::Confirmed(accepted),
                    Outcome::Observed(page) => EditResolution::Observed(page),
                };
                j.edit_resolve(&scope, &ts, &id, resolution, retry_at)
            })
            .await?;
        saved.inventory = inventory;
        saved.pending = None;
        if publication.is_some() {
            saved.publication = publication;
        }
        Ok(())
    }
    pub(super) async fn capture(
        &self,
        seed: &OwnEditSeed,
        blocks: &[Value],
    ) -> Result<Option<OwnEditAccepted>, Error> {
        let mut saved = self.state.lock().await;
        self.settle_pending(&mut saved).await?;
        if saved.inventory.seed() == Some(seed) && saved.inventory.blocks() == blocks {
            return Ok(saved.publication.take());
        }
        let expected = saved.inventory.clone();
        let seed = seed.clone();
        let blocks = blocks.to_vec();
        saved.inventory = self
            .disk
            .work(move |j| j.edit_save(&expected, &seed, &uuid::Uuid::new_v4().to_string(), blocks))
            .await?;
        Ok(saved.publication.take())
    }
    pub(super) async fn begin(&self, record: OwnEditRecord) -> Result<String, Error> {
        let mut saved = self.state.lock().await;
        self.settle_pending(&mut saved).await?;
        let expected = saved.inventory.clone();
        let id = uuid::Uuid::new_v4().to_string();
        let attempt_id = id.clone();
        saved.inventory = self
            .disk
            .work(move |j| j.edit_begin(&expected, &attempt_id, &record))
            .await?;
        Ok(id)
    }
    pub(super) async fn resolve(
        &self,
        id: String,
        outcome: Outcome,
        retry_at: i64,
    ) -> Result<(), Error> {
        let mut saved = self.state.lock().await;
        // An earlier terminal proof must be committed before later readback.
        self.settle_pending(&mut saved).await?;
        saved.pending = Some((id, outcome, retry_at));
        self.settle_pending(&mut saved).await?;
        // The successful resolve caller publishes its original receipt. If
        // persistence failed, retain that receipt for the next native capture.
        saved.publication = None;
        Ok(())
    }
    pub(super) async fn clear(&self) -> Result<(), Error> {
        let mut saved = self.state.lock().await;
        self.settle_pending(&mut saved).await?;
        let expected = saved.inventory.clone();
        saved.inventory = self.disk.work(move |j| j.edit_clear(&expected)).await?;
        Ok(())
    }
}

pub(super) fn restored_attempt(
    inventory: &EditInventory,
    account: &str,
) -> Option<Arc<Mutex<Attempt>>> {
    let a = inventory.attempt()?;
    Some(Arc::new(Mutex::new(Attempt {
        id: next_id(),
        disk_id: Some(a.id().to_owned()),
        account: Some(account.to_owned()),
        record: a.record().clone(),
        phase: match a.phase() {
            EditPhase::Pending | EditPhase::Uncertain => Phase::Unknown,
            EditPhase::NotSent => Phase::NotSent,
            EditPhase::Rejected | EditPhase::RateLimited => Phase::Rejected,
            EditPhase::Confirmed => Phase::Confirmed,
            EditPhase::Observed => Phase::Observed,
        },
        receipt: a.raw().cloned(),
        retry_at: i64::try_from(inventory.retry_at().max(a.retry_at())).unwrap_or(i64::MAX),
    })))
}

/// Native capture is acknowledged only after this exact buffer is durable.
/// The original Disk survives forced route/account retirement.
pub(crate) async fn persist_capture(
    mut state: Signal<ShellState>,
    binding: &Binding,
) -> Result<(), Error> {
    let (disk, seed, blocks) = {
        let shell = state.peek();
        let s = session(&shell, binding).ok_or_else(unavailable)?;
        (
            s.durable.clone().ok_or_else(unavailable)?,
            s.seed.clone(),
            s.blocks.clone(),
        )
    };
    let publication = disk.capture(&seed, &blocks).await?;
    if let Some(accepted) = publication {
        // Publish the original accepted result only after its local retry
        // commits. Existing owner/source guards preserve any newer comment.
        publish::accepted(&mut state.write(), binding, &accepted);
    }
    let inventory = disk.inventory().await;
    // A prior Check may have proved content but failed its local commit. The
    // retried capture can finish that commit without performing another read.
    // Do not turn an in-flight Pending send into a restored Unknown attempt.
    if let Some(restored) = restored_attempt(&inventory, &disk.disk.account) {
        let restored = restored.lock().unwrap();
        if !restored.phase.blocks() {
            if let Some(s) = session_mut(&mut state.write(), binding) {
                if let Some(original) = &s.attempt {
                    let mut original = original.lock().unwrap();
                    if original.disk_id == restored.disk_id && original.record == restored.record {
                        let changed = original.phase != restored.phase;
                        original.phase = restored.phase;
                        original.receipt = restored.receipt.clone();
                        original.retry_at = restored.retry_at;
                        if changed && restored.phase == Phase::Observed {
                            s.notice = Some("The edited content was found and local recovery is saved. Cancel returns to your reply; any newer edit remains here until then.".into());
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
