use super::*;
use super_platinum_core::slack_lists::comments::{
    OwnDeleteAccepted,
    journal::own_delete::{DeleteInventory, DeleteResolution},
};
#[derive(Clone)]
pub(super) enum Outcome {
    NotRequested,
    RateLimited,
    Rejected(Value),
    Uncertain(Option<Value>),
    Confirmed(Arc<OwnDeleteAccepted>),
    Observed(Page),
}
impl Outcome {
    fn borrowed(&self) -> DeleteResolution<'_> {
        match self {
            Self::NotRequested => DeleteResolution::NotRequested,
            Self::RateLimited => DeleteResolution::RateLimited,
            Self::Rejected(v) => DeleteResolution::Rejected(v),
            Self::Uncertain(v) => DeleteResolution::Uncertain(v.as_ref()),
            Self::Confirmed(v) => DeleteResolution::Confirmed(v),
            Self::Observed(p) => DeleteResolution::Observed(p),
        }
    }
}
pub(super) struct Recovery {
    pub disk: storage::Disk,
    pub ts: String,
    saved: tokio::sync::Mutex<Saved>,
}
struct Saved {
    inventory: DeleteInventory,
    pending: Option<(String, Outcome, u64)>,
}
impl Recovery {
    pub async fn load(disk: storage::Disk, ts: String) -> Result<Self, Error> {
        let scope = disk.scope.clone();
        let key = ts.clone();
        let inventory = disk.work(move |j| j.delete_inventory(&scope, &key)).await?;
        Ok(Self {
            disk,
            ts,
            saved: tokio::sync::Mutex::new(Saved {
                inventory,
                pending: None,
            }),
        })
    }
    pub async fn inventory(&self) -> DeleteInventory {
        self.saved.lock().await.inventory.clone()
    }
    pub async fn pending(&self) -> bool {
        self.saved.lock().await.pending.is_some()
    }
    pub async fn begin(&self, record: &OwnDeleteRecord) -> Result<String, Error> {
        let mut saved = self.saved.lock().await;
        if saved.pending.is_some() {
            return Err(unavailable());
        }
        let expected = saved.inventory.clone();
        let record = record.clone();
        let id = uuid::Uuid::new_v4().to_string();
        let key = id.clone();
        let result = self
            .disk
            .work(move |j| j.delete_begin(&expected, &key, &record))
            .await;
        match result {
            Ok(value) => {
                saved.inventory = value;
                Ok(id)
            }
            Err(error) => {
                // A failed local operation never grants permission to send. Reload
                // before exposing a new action, including ambiguous commit errors.
                let scope = self.disk.scope.clone();
                let ts = self.ts.clone();
                if let Ok(value) = self
                    .disk
                    .work(move |j| j.delete_inventory(&scope, &ts))
                    .await
                {
                    saved.inventory = value;
                }
                Err(error)
            }
        }
    }
    pub async fn resolve(
        &self,
        id: String,
        outcome: Outcome,
        retry_at: u64,
    ) -> Result<Option<Outcome>, Error> {
        let mut saved = self.saved.lock().await;
        if saved.pending.is_some() {
            return Err(unavailable());
        }
        saved.pending = Some((id, outcome, retry_at));
        self.flush_locked(&mut saved).await
    }
    pub async fn flush(&self) -> Result<Option<Outcome>, Error> {
        let mut saved = self.saved.lock().await;
        self.flush_locked(&mut saved).await
    }
    async fn flush_locked(&self, saved: &mut Saved) -> Result<Option<Outcome>, Error> {
        let Some((id, outcome, retry_at)) = saved.pending.clone() else {
            return Ok(None);
        };
        let scope = self.disk.scope.clone();
        let ts = self.ts.clone();
        let result = outcome.clone();
        saved.inventory = self
            .disk
            .work(move |j| j.delete_resolve(&scope, &ts, &id, result.borrowed(), retry_at))
            .await?;
        saved.pending = None;
        Ok(Some(outcome))
    }
}
