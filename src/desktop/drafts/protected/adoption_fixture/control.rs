//! Fixture phase gates delay real lifecycle work. They do not fabricate outcomes.
use super::*;
impl Binding {
    pub fn arm_gate(&self, phase: &str) -> Result<(), String> {
        if !["registering", "prepared", "locked", "installed"].contains(&phase) {
            return Err("Unknown adoption phase".into());
        }
        self.releases.lock().unwrap().remove(phase);
        self.armed.lock().unwrap().insert(phase.into());
        Ok(())
    }
    pub(super) fn armed(&self, phase: &str) -> bool {
        self.armed.lock().unwrap().contains(phase)
    }
    pub(super) async fn hold(&self, phase: &str) {
        if !self.armed(phase) {
            return;
        }
        *self.gate.lock().unwrap() = Some(phase.into());
        self.record(json!({"kind":"gate","phase":phase}));
        self.changed.notify_waiters();
        let mut waiting = GateWait {
            binding: self,
            phase: phase.into(),
            released: false,
        };
        loop {
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.releases.lock().unwrap().remove(phase) {
                break;
            }
            notified.await;
        }
        waiting.released = true;
        self.armed.lock().unwrap().remove(phase);
        *self.gate.lock().unwrap() = None;
        self.record(json!({"kind":"gate-released","phase":phase}));
    }
    pub async fn wait_gate(&self, phase: &str) -> Result<(), String> {
        tokio::time::timeout(std::time::Duration::from_secs(15), async {
            loop {
                let notified = self.changed.notified();
                tokio::pin!(notified);
                notified.as_mut().enable();
                if self.gate.lock().unwrap().as_deref() == Some(phase) {
                    return;
                }
                notified.await;
            }
        })
        .await
        .map_err(|_| "Adoption phase gate timed out".into())
    }
    pub fn release_gate(&self, phase: &str) {
        self.releases.lock().unwrap().insert(phase.into());
        self.changed.notify_waiters();
    }
    pub async fn wait_result(&self) -> Result<Value, String> {
        tokio::time::timeout(std::time::Duration::from_secs(15), async {
            loop {
                let notified = self.changed.notified();
                tokio::pin!(notified);
                notified.as_mut().enable();
                if let Some(v) = self.result.lock().unwrap().clone() {
                    return v;
                }
                notified.await;
            }
        })
        .await
        .map_err(|_| "Adoption result timed out".into())
    }
    pub fn registry_label(&self, shell: &ShellState) -> Result<String, String> {
        let registry = self
            .host
            .lock()
            .unwrap()
            .discovery()
            .registry(shell)
            .map_err(err)?;
        let mut labels = self.registry_labels.lock().unwrap();
        if let Some((_, label)) = labels.iter().find(|(old, _)| Arc::ptr_eq(old, &registry)) {
            return Ok(label.clone());
        }
        if labels.len() >= 32 {
            return Err("Registry evidence bound exceeded".into());
        }
        let label = barrier_id();
        labels.push((registry, label.clone()));
        Ok(label)
    }
}
pub(crate) fn route(shell: &ShellState, changes: BTreeMap<DraftKey, LocalDraft>) {
    let Some(b) = binding(shell) else {
        return;
    };
    let selection = b.selected.lock().unwrap().clone();
    for (key, value) in changes {
        let result = b
            .host
            .lock()
            .unwrap()
            .submit(shell, &selection, &key, &value);
        match result {
            Ok(serial) => {
                b.routed.fetch_add(1, Ordering::Relaxed);
                b.record(json!({"kind":"input","key":key,"serial":serial,"local":value}));
            }
            Err(error) => {
                b.refused.fetch_add(1, Ordering::Relaxed);
                b.record(
                    json!({"kind":"input-refused","key":key,"local":value,"error":err(error)}),
                );
            }
        }
    }
}
pub(crate) async fn flush_inputs(b: &Binding) -> Result<(), String> {
    for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
        let installed = b
            .host
            .lock()
            .unwrap()
            .installation(&b.account, &key(target));
        if let Some(installed) = installed {
            installed.editor().flush().await.map_err(err)?;
        } else {
            b.original_lane(target)?
                .writer()
                .flush()
                .await
                .map_err(err)?;
        }
    }
    Ok(())
}
pub(crate) async fn arm_failure(b: &Binding) -> Result<(), String> {
    if b.scenario != "failed-managed-write-channel" {
        return Err("Wrong adoption fault case".into());
    }
    account::wait(
        b.isolated
            .fail_next(durable::test_support::FaultPoint::WriterCommit),
    )
    .await
    .map_err(err)
}
pub(crate) fn old_lane_refuses(b: &Binding) -> Result<Value, String> {
    let registered = b
        .registered
        .lock()
        .unwrap()
        .clone()
        .ok_or("No retained registration")?;
    let before = registered.lane.writer().snapshot();
    let result = registered
        .lane
        .writer()
        .submit(&local("Old cloned lane must refuse", b.target()));
    if !matches!(result, Err(durable::Error::Stale)) {
        return Err("Retired lane accepted an input".into());
    }
    let after = registered.lane.writer().snapshot();
    if before.serial != after.serial
        || !match (&before.pending, &after.pending) {
            (None, None) => true,
            (Some(a), Some(b)) => same_local(a, b),
            _ => false,
        }
    {
        return Err("Retired lane changed on refusal".into());
    }
    Ok(
        json!({"laneId":registered.lane.id(),"beforeSerial":before.serial,"afterSerial":after.serial,"result":"Stale"}),
    )
}

impl Binding {
    // A fixture observation of successful acknowledgment, never route authority.
    // The Host still validates the exact completed receipt before freeing its job.
    pub(super) fn acknowledge(&self, receipt: &lifecycle::Installed) -> Result<(), durable::Error> {
        let mut seen = self
            .acknowledged
            .lock()
            .map_err(|_| durable::Error::Storage)?;
        if !seen.contains(receipt.adoption_id()) {
            self.host
                .lock()
                .map_err(|_| durable::Error::Storage)?
                .acknowledge_installation(receipt)?;
            seen.insert(receipt.adoption_id().into());
            self.record(json!({"kind":"installation-acknowledged","receiptId":receipt.id()}));
        }
        Ok(())
    }
}

// Dropping a canceled waiter releases only fixture timing state. Actual Owner
// and native guards are owned by the canceled future and follow their own Drop.
struct GateWait<'a> {
    binding: &'a Binding,
    phase: String,
    released: bool,
}
impl Drop for GateWait<'_> {
    fn drop(&mut self) {
        if !self.released {
            self.binding.armed.lock().unwrap().remove(&self.phase);
            let mut gate = self.binding.gate.lock().unwrap();
            if gate.as_deref() == Some(self.phase.as_str()) {
                *gate = None;
            }
            drop(gate);
            self.binding
                .record(json!({"kind":"gate-canceled","phase":self.phase}));
            self.binding.changed.notify_waiters();
        }
    }
}
