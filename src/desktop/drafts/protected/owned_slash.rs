//! One Account-retained Slash branch. The real Owned worker is the only executor.
use super::owned_route::{o, ui};
use super::ownership::Tracked as Mutex;
use super::*;
use super_platinum_core::slash_commands::controller::Record;

pub(crate) struct Slash {
    pub context: Arc<Context>,
    pub(super) capture: editor_proof::Capture,
    pub input: o::SlashInput,
    state: Mutex<State>,
}
struct State {
    reserved: Result<o::SlashReserved, o::AdmissionRefused<o::SlashInput>>,
    claimed: Option<o::SlashClaimed>,
    completion: Option<o::SlashCompletion>,
    execute_error: Option<o::SlashExecuteError>,
    storage_state: Option<o::JobState>,
    terminal: Option<Record>,
    fresh: Option<o::SlashFreshClear>,
    fresh_consumed: bool,
    canonical: Option<durable::Review>,
    native_clear: NativeClear,
    reconciled: bool,
    archive: Option<Arc<archive::Archive>>,
}
pub(super) struct Ownership {
    state: ownership::Stamp,
    archive: Option<(Arc<archive::Archive>, ownership::Stamp)>,
}
impl Slash {
    pub(super) fn new(
        ops: &o::Operations,
        context: Arc<Context>,
        capture: editor_proof::Capture,
        input: o::SlashInput,
    ) -> Arc<Self> {
        Arc::new(Self {
            context,
            capture,
            state: Mutex::new(State {
                reserved: ops.slash(input.clone()),
                claimed: None,
                completion: None,
                execute_error: None,
                storage_state: None,
                terminal: None,
                fresh: None,
                fresh_consumed: false,
                canonical: None,
                native_clear: NativeClear::NotAttempted,
                reconciled: false,
                archive: None,
            }),
            input,
        })
    }
    pub(super) fn ownership_stamp(&self) -> Result<Ownership, ownership::Error> {
        let (state, archive) = self.state.observe(|s| s.archive.clone())?;
        let archive = archive
            .map(|a| {
                let stamp = a.ownership_stamp()?;
                Ok::<_, ownership::Error>((a, stamp))
            })
            .transpose()?;
        Ok(Ownership { state, archive })
    }
    pub(super) fn validate_ownership(&self, stamp: &Ownership) -> Result<(), ownership::Error> {
        self.state.validate(&stamp.state)?;
        if let Some((archive, stamp)) = &stamp.archive {
            archive.validate_ownership(stamp)?;
        }
        Ok(())
    }
    pub(super) fn submit(&self) -> Result<(), durable::Error> {
        self.state
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .reserved
            .as_ref()
            .map_err(|e| ui(e.reason))?
            .submit()
            .map_err(ui)
    }
    /// Every handle is retained before submit/await. No native proof is inferred here.
    pub(crate) async fn prepare_dispatch(&self) -> Result<(), durable::Error> {
        let reserved = self
            .state
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .reserved
            .as_ref()
            .map_err(|e| ui(e.reason))?
            .clone();
        let wait = reserved.clone();
        ready(
            tokio::task::spawn_blocking(move || wait.wait())
                .await
                .map_err(|_| durable::Error::Storage)?
                .map_err(ui)?,
        )?;
        let claimed = reserved.claim().map_err(ui)?;
        self.state
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .claimed = Some(claimed.clone());
        claimed.submit().map_err(ui)?;
        let wait = claimed.clone();
        ready(
            tokio::task::spawn_blocking(move || wait.wait())
                .await
                .map_err(|_| durable::Error::Storage)?
                .map_err(ui)?,
        )?;
        let completion = claimed.completion().map_err(ui)?;
        self.state
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .completion = Some(completion);
        Ok(())
    }
    /// Diagnostic observation only; never an admission or accounting certificate.
    pub(crate) fn evidence(&self) -> serde_json::Value {
        let s = self.state.lock().unwrap();
        serde_json::json!({
            "reserved":s.reserved.is_ok(), "claimed":s.claimed.is_some(),
            "completion":s.completion.is_some(),
            "executeError":s.execute_error.as_ref().map(|e|format!("{e:?}")),
            "storageState":s.storage_state.map(|v|format!("{v:?}")),
            "reportRetained":s.completion.as_ref().is_some_and(|c|c.report().is_some()),
            "terminal":s.terminal, "freshHeld":s.fresh.is_some(),
            "freshConsumed":s.fresh_consumed, "canonicalObserved":s.canonical.is_some(),
            "nativeClear":format!("{:?}",s.native_clear), "reconciled":s.reconciled
        })
    }
    pub(crate) fn completion(&self) -> Result<o::SlashCompletion, durable::Error> {
        self.state
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .completion
            .clone()
            .ok_or(durable::Error::Pending)
    }
    /// Caller has a current short native lease at the first poll. The lease is
    /// dropped after this future first yields (the actual fixed result gate).
    pub(crate) async fn execute(&self) -> Result<(), durable::Error> {
        let completion = self.completion()?;
        let result = self.context.execute(&completion).await;
        if let Err(error) = result {
            self.state
                .lock()
                .map_err(|_| durable::Error::Storage)?
                .execute_error = Some(error);
            return Err(durable::Error::Storage);
        }
        self.wait_completion().await
    }
    pub(crate) async fn wait_completion(&self) -> Result<(), durable::Error> {
        let completion = self.completion()?;
        let wait = completion.clone();
        let observed = tokio::task::spawn_blocking(move || wait.wait())
            .await
            .map_err(|_| durable::Error::Storage)?
            .map_err(ui)?;
        let mut state = self.state.lock().map_err(|_| durable::Error::Storage)?;
        state.storage_state = Some(observed);
        ready(observed)?;
        state.terminal = completion.terminal();
        // Re-observation cannot destroy the already-retained once-only witness.
        // After consumption, no later storage observation may reacquire one.
        if state.fresh.is_none() && !state.fresh_consumed {
            state.fresh = completion.take_fresh_clear();
        }
        if state.terminal.is_none() {
            return Err(durable::Error::Pending);
        }
        Ok(())
    }
    /// Storage-only retry, never execute or a second Claim.
    pub(crate) async fn retry_storage(&self) -> Result<(), durable::Error> {
        self.completion()?.retry_storage().map_err(ui)?;
        self.wait_completion().await
    }
    pub(crate) fn canonical(&self, review: durable::Review) -> Result<(), durable::Error> {
        self.state
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .canonical = Some(review);
        Ok(())
    }
    pub(crate) fn has_fresh_clear(&self) -> Result<bool, durable::Error> {
        let s = self.state.lock().map_err(|_| durable::Error::Storage)?;
        Ok(s.fresh.is_some() && !s.fresh_consumed)
    }
    pub(crate) fn consume_fresh_clear(&self) -> Result<(), durable::Error> {
        let mut s = self.state.lock().map_err(|_| durable::Error::Storage)?;
        let c = s.completion.clone().ok_or(durable::Error::Pending)?;
        let terminal = s.terminal.clone().ok_or(durable::Error::Pending)?;
        let fresh = s.fresh.take().ok_or(durable::Error::Stale)?;
        if let Err(fresh) = fresh.consume(&c, self.input.prior(), &terminal) {
            s.fresh = Some(fresh);
            return Err(durable::Error::Stale);
        }
        s.fresh_consumed = true;
        Ok(())
    }
    pub(super) fn permits_reconcile(&self, after: &durable::Review) -> bool {
        self.state.lock().is_ok_and(|s| {
            s.fresh_consumed
                && !s.reconciled
                && s.storage_state == Some(o::JobState::Ready)
                && s.canonical.as_ref().is_some_and(|v| v.same(after))
        })
    }
    pub(crate) fn native_result(
        &self,
        value: NativeClear,
        reconciled: bool,
    ) -> Result<(), durable::Error> {
        let mut s = self.state.lock().map_err(|_| durable::Error::Storage)?;
        s.native_clear = value;
        s.reconciled = reconciled;
        Ok(())
    }
}
fn ready(value: o::JobState) -> Result<(), durable::Error> {
    if value == o::JobState::Ready {
        Ok(())
    } else {
        Err(durable::Error::Pending)
    }
}

#[cfg(test)]
#[path = "owned_slash/tests.rs"]
mod tests;

mod context;
pub(crate) use context::Context;

mod archive;
