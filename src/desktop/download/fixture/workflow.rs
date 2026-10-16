//! Separate actual-rfd acceptance fixture. The cancellation diagnostic stays inert.
use super::*;
use serde::Serialize;
use serde_json::json;

mod check;
mod paths;
pub(crate) use check::run;
pub(super) const ALIAS: &str = "original-download-save-workflow";
static WORKFLOW: OnceLock<Arc<Workflow>> = OnceLock::new();

struct Workflow {
    directory: paths::Directory,
    ledger: Mutex<Ledger>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct Returned {
    attempt: usize,
    kind: &'static str,
    accepted: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct Outcome {
    attempt: usize,
    kind: &'static str,
}
#[derive(Clone, Default)]
struct Ledger {
    issued: usize,
    active: Option<(usize, bool)>,
    failed: bool,
    returns: Vec<Returned>,
    outcomes: Vec<Outcome>,
}
impl Ledger {
    fn begin(&mut self) -> Option<usize> {
        if self.failed || self.active.is_some() || self.issued >= 3 {
            return None;
        }
        self.issued += 1;
        self.active = Some((self.issued, false));
        Some(self.issued)
    }
    fn returned(&mut self, attempt: usize, kind: &'static str) -> bool {
        let expected = match attempt {
            1 => "none",
            2 => "existing",
            3 => "new",
            _ => "",
        };
        let accepted = !self.failed && self.active == Some((attempt, false)) && kind == expected;
        if self.returns.len() < 4 {
            self.returns.push(Returned {
                attempt,
                kind,
                accepted,
            });
        }
        if !accepted {
            self.failed = true;
        }
        self.active = if accepted && attempt != 1 {
            Some((attempt, true))
        } else {
            None
        };
        accepted
    }
    fn completed(&mut self, attempt: usize, kind: &'static str) -> bool {
        let expected = match attempt {
            2 => "destination-exists",
            3 => "saved",
            _ => "",
        };
        let accepted = !self.failed && self.active == Some((attempt, true)) && kind == expected;
        if self.outcomes.len() < 3 {
            self.outcomes.push(Outcome { attempt, kind });
        }
        if !accepted {
            self.failed = true;
        }
        self.active = None;
        accepted
    }
}

pub(crate) fn enabled() -> bool {
    cfg!(target_os = "windows")
        && std::env::var("SUPER_PLATINUM_FIXTURE").as_deref() == Ok(ALIAS)
        && std::env::var("SUPER_PLATINUM_SAVE_WORKFLOW_CHECK").as_deref() == Ok("1")
}

impl Input {
    pub fn native_workflow(&self) -> bool {
        enabled() && self.run.alias == ALIAS && WORKFLOW.get().is_some()
    }
    pub fn begin_workflow(&self) -> bool {
        if !self.native_workflow()
            || self
                .attempt
                .compare_exchange(0, usize::MAX, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
        {
            return false;
        }
        let Some(workflow) = WORKFLOW.get() else {
            return false;
        };
        let Ok(mut ledger) = workflow.ledger.lock() else {
            return false;
        };
        let Some(attempt) = ledger.begin() else {
            return false;
        };
        self.attempt.store(attempt, Ordering::Release);
        self.run.issued.store(attempt, Ordering::Release);
        eprintln!(
            "workflow-requested: {}",
            json!({"version":1,"nonce":self.run.nonce,"pid":std::process::id(),"attempt":attempt,"actualRfd":true})
        );
        true
    }
    pub async fn workflow_returned(&self, destination: Option<PathBuf>) -> Option<PathBuf> {
        let attempt = self.attempt.load(Ordering::Acquire);
        let workflow = WORKFLOW.get()?.clone();
        let kind = match destination.as_ref() {
            None => "none",
            Some(path) => {
                let path = path.clone();
                let directory = workflow.clone();
                tokio::task::spawn_blocking(move || directory.directory.validate(&path, attempt))
                    .await
                    .ok()
                    .and_then(Result::ok)
                    .unwrap_or("invalid")
            }
        };
        let accepted = workflow.ledger.lock().ok()?.returned(attempt, kind);
        let transfer_allowed = accepted && matches!(kind, "existing" | "new");
        eprintln!(
            "workflow-returned: {}",
            json!({"version":1,"nonce":self.run.nonce,"pid":std::process::id(),"attempt":attempt,"kind":kind,"accepted":accepted,"actualRfd":true,"transferAllowed":transfer_allowed})
        );
        // Preserve the actual rfd path. This validation never substitutes one.
        if transfer_allowed { destination } else { None }
    }
    pub(super) fn workflow_completed(&self, result: &Result<(), DownloadError>) {
        let attempt = self.attempt.load(Ordering::Acquire);
        let kind = match result {
            Ok(()) => "saved",
            Err(DownloadError::DestinationExists) => "destination-exists",
            _ => "other",
        };
        if let Some(workflow) = WORKFLOW.get() {
            if let Ok(mut ledger) = workflow.ledger.lock() {
                ledger.completed(attempt, kind);
            }
        }
        eprintln!(
            "workflow-outcome: {}",
            json!({"version":1,"nonce":self.run.nonce,"pid":std::process::id(),"attempt":attempt,"kind":kind})
        );
    }
}

fn snapshot() -> Result<Ledger, String> {
    WORKFLOW
        .get()
        .ok_or("workflow not installed")?
        .ledger
        .lock()
        .map(|ledger| ledger.clone())
        .map_err(|_| "workflow ledger unavailable".into())
}

async fn install_workflow(state: Signal<ShellState>) -> Result<Arc<Run>, String> {
    if !enabled() {
        return Err("exact Windows workflow gate required".into());
    }
    let root = PathBuf::from(
        std::env::var("SUPER_PLATINUM_ORIGINAL_DOWNLOAD_DIR").map_err(|_| "workflow directory")?,
    );
    let nonce =
        std::env::var("SUPER_PLATINUM_ORIGINAL_DOWNLOAD_NONCE").map_err(|_| "workflow nonce")?;
    let directory = tokio::task::spawn_blocking(move || paths::Directory::capture(root, nonce))
        .await
        .map_err(|_| "workflow directory worker")??;
    // Existing installer requires empty downloads and no session/transport/account.
    let run = super::install(state).await?;
    WORKFLOW
        .set(Arc::new(Workflow {
            directory,
            ledger: Mutex::new(Ledger::default()),
        }))
        .map_err(|_| "workflow already installed")?;
    Ok(run)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_workflow_requires_exact_three_request_return_outcome_sequence() {
        let mut ledger = Ledger::default();
        assert_eq!(ledger.begin(), Some(1));
        assert_eq!(ledger.begin(), None);
        assert!(ledger.returned(1, "none"));
        assert_eq!(ledger.begin(), Some(2));
        assert!(ledger.returned(2, "existing"));
        assert_eq!(ledger.begin(), None);
        assert!(ledger.completed(2, "destination-exists"));
        assert_eq!(ledger.begin(), Some(3));
        assert!(ledger.returned(3, "new"));
        assert!(ledger.completed(3, "saved"));
        assert_eq!(ledger.begin(), None);
        assert!(!ledger.failed);
    }
    #[test]
    fn wrong_duplicate_or_unrequested_returns_poison_without_replay() {
        for (attempt, kind) in [(1, "existing"), (1, "invalid"), (2, "none"), (3, "new")] {
            let mut ledger = Ledger::default();
            ledger.begin();
            assert!(!ledger.returned(attempt, kind));
            assert!(ledger.failed);
            assert_eq!(ledger.begin(), None);
        }
        let mut ledger = Ledger::default();
        ledger.begin();
        assert!(ledger.returned(1, "none"));
        assert!(!ledger.returned(1, "none"));
        assert_eq!(ledger.begin(), None);
    }
    #[test]
    fn canceled_or_wrong_transfer_outcomes_cannot_advance_workflow() {
        let mut ledger = Ledger::default();
        ledger.begin();
        ledger.returned(1, "none");
        ledger.begin();
        ledger.returned(2, "existing");
        assert!(!ledger.completed(2, "saved"));
        assert_eq!(ledger.begin(), None);
        let mut ledger = Ledger::default();
        ledger.begin();
        assert!(!ledger.completed(1, "other"));
        assert_eq!(ledger.begin(), None);
    }
}
