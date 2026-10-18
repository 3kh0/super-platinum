//! Fresh Open Release root; only its prebound StartLocal may cross retirement.
use super::owned_route::{Held, Route, o, ui};
use super::ownership::Tracked as Mutex;
use super::*;
mod retire;
#[derive(Default)]
struct Child {
    completion: Option<o::LocalCompletion>,
    acknowledgment: Option<o::LocalHandoffAcknowledgment>,
    acknowledged: Option<durable::LocalWriterLease>,
}
pub(super) struct Release {
    pub held: Arc<Held<o::ReleaseInput, durable::Release>>,
    child: Mutex<Child>,
}
impl Release {
    /// Desktop holder membership only. Core job progression is a separate edge.
    pub(super) fn ownership_stamp(
        &self,
    ) -> Result<super::ownership::Stamp, super::ownership::Error> {
        self.child.observe(|_| ()).map(|(stamp, ())| stamp)
    }
    pub(super) fn validate_ownership(
        &self,
        stamp: &super::ownership::Stamp,
    ) -> Result<(), super::ownership::Error> {
        self.child.validate(stamp)
    }

    pub fn new(route: &Route, input: o::ReleaseInput) -> Arc<Self> {
        Arc::new(Self {
            held: Held::new(route.operations.release(input)),
            child: Mutex::default(),
        })
    }
    pub async fn finish(
        &self,
        parent: &operation::Operation,
    ) -> Result<durable::LocalWriterLease, durable::Error> {
        self.held.wait().await.map_err(ui)?;
        let child = {
            let mut child = self.child.lock().map_err(|_| durable::Error::Storage)?;
            if child.completion.is_none() {
                // start_local transfers the Release result into this exact
                // child. Later retry/reattachment must use that retained child,
                // never reread the consumed parent result or create a Release.
                let release = self
                    .held
                    .job()
                    .map_err(ui)?
                    .result()
                    .ok_or(durable::Error::Pending)??;
                parent.retain(|s| s.release = Some(release))?;
                child.completion = Some(self.held.job().map_err(ui)?.start_local().map_err(ui)?);
            }
            child.completion.clone().ok_or(durable::Error::Pending)?
        };
        // Evidence phase may be stale while this exact storage continuation is
        // still admitted. Never resurrect root currentness or substitute Release.
        let _ = parent.phase(operation::Phase::StartLocal);
        child.submit().map_err(ui)?;
        let lease = tokio::task::spawn_blocking(move || child.wait())
            .await
            .map_err(|_| durable::Error::Storage)?
            .map_err(ui)??;
        parent.retain(|s| s.local_lease = Some(lease.clone()))?;
        Ok(lease)
    }
    pub async fn retry_storage(
        &self,
        parent: &operation::Operation,
    ) -> Result<durable::LocalWriterLease, durable::Error> {
        // No Release or child creation here: only the exact already registered
        // StartLocal may be repaired, including after its original waiter left.
        let child = self
            .child
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .completion
            .clone()
            .ok_or(durable::Error::Pending)?;
        match child.result().map_err(ui)? {
            Some(Err(durable::Error::Storage)) => child.retry_storage().map_err(ui)?,
            Some(Ok(_)) => (),
            Some(Err(error)) => return Err(error),
            None => return Err(durable::Error::Pending),
        }
        self.finish(parent).await
    }
}
