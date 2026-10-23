//! Display authority for one original edit, independent of reply/query receipts.
use super::*;
use crate::profile_edit::sections::rich::search::FileTitles;
use std::collections::BTreeMap;

#[derive(Clone)]
pub(super) struct Job {
    binding: Binding,
    seed: Arc<OwnEditSeed>,
    blocks: Vec<Value>,
    revision: u64,
    durable: Arc<durable::Durable>,
    suspension: Arc<Mutex<Option<draft::ReplySuspension>>>,
    tokens: Vec<Value>,
    serial: Arc<()>,
    loading: bool,
    retry_at: i64,
    error: Option<String>,
    observed: Option<Arc<super::super::title_users::Observed>>,
}
impl Job {
    fn current(&self, shell: &ShellState) -> bool {
        self.binding.current(shell)
            && session(shell, &self.binding).is_some_and(|s| {
                // The saved seed may be older than the source captured by Edit.
                // This is display custody, never fresh Save permission.
                Arc::ptr_eq(&self.seed, &s.seed)
                    && self.blocks == s.blocks
                    && self.revision == s.revision
                    && s.durable
                        .as_ref()
                        .is_some_and(|d| Arc::ptr_eq(d, &self.durable))
                    && Arc::ptr_eq(&self.suspension, &s.suspension)
                    && s.suspension
                        .lock()
                        .unwrap()
                        .as_ref()
                        .is_some_and(|r| r.current(shell))
                    && !s.finishing
            })
    }
    fn active(&self, shell: &ShellState) -> bool {
        self.current(shell)
            && session(shell, &self.binding)
                .and_then(|s| s.titles.as_ref())
                .is_some_and(|j| Arc::ptr_eq(&j.serial, &self.serial))
    }
}
pub(super) fn can_cancel_unmounted(shell: &ShellState, s: &Session) -> bool {
    s.binding.visible_mount(shell)
        && s.native.is_none()
        && s.native_sequence == 0
        && !s.native_ready
        && !s.capture_failed
        && !s.busy
        && !s.finishing
        && s.durable.is_some()
        && attempt_phase(s).is_none_or(|p| !p.blocks())
        && s.suspension
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|r| r.current(shell))
}
pub(super) fn readiness(shell: &ShellState, s: &Session) -> (bool, bool, Option<String>, bool) {
    // A mounted editor retains its own labels. Metadata does not remount it.
    if s.native.is_some() || s.native_sequence > 0 {
        return (true, false, None, false);
    }
    if s.finishing {
        return (false, false, None, false);
    }
    if s.plain.is_some() {
        let ready = s.plain_mount.as_ref().is_some_and(|m| m.current(shell));
        return (ready, false, (!ready).then(|| "Editor preferences changed. Cancel and reopen this edit; both drafts are retained.".into()), false);
    }
    if s.titles.is_none()
        && FileTitles::reply_tokens(shell, s.binding.owner_team(), &s.blocks).is_empty()
    {
        return (true, false, None, false);
    }
    if let Some(j) = s.titles.as_ref().filter(|j| j.current(shell)) {
        let ready = j.observed.as_ref().is_some_and(|o| o.current(shell));
        return (
            ready,
            j.loading,
            j.error.clone().or_else(|| {
                (!ready && !j.loading)
                    .then(|| "Title details changed. Retry before opening this edit.".into())
            }),
            !ready && !j.loading && j.retry_at <= now() && !s.finishing,
        );
    }
    (
        false,
        false,
        Some("Title details are unavailable. Your saved edit and reply are retained.".into()),
        s.binding.current(shell) && !s.finishing,
    )
}
pub(crate) fn labels(shell: &ShellState, b: &Binding) -> BTreeMap<String, String> {
    session(shell, b)
        .and_then(|s| s.titles.as_ref())
        .filter(|j| j.current(shell))
        .and_then(|j| j.observed.as_ref())
        .filter(|o| o.current(shell))
        .and_then(|o| o.files.labels(shell).ok())
        .unwrap_or_default()
}
pub(crate) fn prepare(mut state: Signal<ShellState>, b: Binding) {
    let job = {
        let shell = state.peek();
        let Some(s) = session(&shell, &b).filter(|s| {
            b.current(&shell)
                && s.plain.is_none()
                && s.native.is_none()
                && s.native_sequence == 0
                && !s.finishing
                && !s.busy
        }) else {
            return;
        };
        let tokens = s
            .titles
            .as_ref()
            .filter(|j| j.blocks == s.blocks && j.revision == s.revision)
            .map(|j| j.tokens.clone())
            // Reuse the closed whole-Canvas election, not another editor's
            // observation. This Job retains the original edit and suspension.
            .unwrap_or_else(|| FileTitles::reply_tokens(&shell, b.owner_team(), &s.blocks));
        if tokens.is_empty() {
            return;
        }
        if s.titles.as_ref().is_some_and(|j| {
            j.retry_at > now()
                || (j.current(&shell)
                    && (j.loading || j.observed.as_ref().is_some_and(|o| o.current(&shell))))
        }) {
            return;
        }
        let Some(durable) = s.durable.clone() else {
            return;
        };
        let job = Job {
            binding: b.clone(),
            seed: s.seed.clone(),
            blocks: s.blocks.clone(),
            revision: s.revision,
            durable,
            suspension: s.suspension.clone(),
            tokens,
            serial: Arc::new(()),
            loading: true,
            retry_at: 0,
            error: None,
            observed: None,
        };
        if !job.current(&shell) {
            return;
        }
        job
    };
    session_mut(&mut state.write(), &b).unwrap().titles = Some(job.clone());
    dioxus::core::spawn_forever(async move {
        let owner = job.clone();
        let guard = Arc::new(move |s: &ShellState| owner.active(s));
        let generation = format!("list-own-edit-title-{}", job.binding.scope());
        let work = super::super::title_users::acquire_edit(
            state,
            job.binding.owner_team(),
            &generation,
            job.tokens.clone(),
            guard,
        );
        let result = file_acquisition::run::guarded(work, || job.active(&state.peek())).await;
        let mut shell = state.write();
        if !job.active(&shell) {
            return;
        }
        let slot = session_mut(&mut shell, &job.binding)
            .unwrap()
            .titles
            .as_mut()
            .unwrap();
        slot.loading = false;
        match result {
            Ok(observed) => {
                slot.observed = Some(observed);
                slot.error = None;
            }
            Err(error) => {
                slot.retry_at = retry_deadline(&error);
                slot.error = Some(
                    if slot.retry_at > now() {
                        "Slack asked us to wait before retrying title details."
                    } else {
                        "Title details could not load. Your saved edit and reply are retained."
                    }
                    .into(),
                );
            }
        }
    });
}
pub(super) fn snapshot(shell: &ShellState, s: &Session) -> Value {
    s.titles.as_ref().map(|j| json!({"scope":j.binding.scope(),"generation":j.binding.generation(),
        "ts":j.binding.source.ts,"revision":j.revision,"blocks":j.blocks,"tokens":j.tokens,
        "current":j.current(shell),"loading":j.loading,"retryAt":j.retry_at,"error":j.error,
        "status":if j.loading {"loading"}else if j.observed.as_ref().is_some_and(|o|o.current(shell)){"ready"}else{"failed"},
        "observed":j.observed.as_ref().map(|o|o.snapshot(shell))})).unwrap_or(Value::Null)
}

#[cfg(test)]
#[path = "tests/titles.rs"]
mod tests;
