//! Receipt metadata can advance without replacing input or releasing its writer.
use super::*;
impl Editor {
    pub(crate) async fn refresh_receipt(self: &Arc<Self>) -> Result<Snapshot, Fault> {
        let editor = self.clone();
        self.runtime
            .spawn(async move {
                let _guard = editor.gate.lock().await;
                {
                    let state = editor.state.lock().unwrap();
                    if state.released
                        || state.settling
                        || state.observing
                        || state.projection.is_some()
                    {
                        return Err(Fault::Stale);
                    }
                }
                // rebased requires unchanged paired content and semantic revision.
                // A pending submit is independent and is deliberately left intact.
                let current = editor.rebased().await?;
                let snapshot = {
                    let mut state = editor.state.lock().unwrap();
                    state.base = current;
                    Snapshot::from_state(&state)
                };
                editor.changed.notify_waiters();
                Ok(snapshot)
            })
            .await
            .map_err(|_| Fault::Storage)?
    }
}
