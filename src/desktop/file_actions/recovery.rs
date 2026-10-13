//! Persist before dispatch; reopening only restores a read-only status check.
use super::*;
use super_platinum_core::operation_receipts::{
    Begin, FileScope, Journal, Operation, Outcome, Receipt, Status,
};

#[derive(Clone)]
struct Storage {
    account: String,
    scope: FileScope,
    fixture_path: Option<std::path::PathBuf>,
}
impl Storage {
    fn capture(shell: &ShellState, target: &Target) -> Option<Self> {
        if !target.current(shell) {
            return None;
        }
        let fixture_path = if crate::fixture::is_fixture() {
            Some(std::env::var_os("SUPER_PLATINUM_FILE_RECOVERY_DB")?.into())
        } else {
            None
        };
        Some(Self {
            account: shell.core.active_account.clone()?,
            scope: FileScope {
                team_id: target.team.clone(),
                user_id: shell
                    .core
                    .workspaces
                    .get(&target.team)?
                    .self_user_id
                    .clone(),
                file_id: target.id.clone(),
            },
            fixture_path,
        })
    }

    fn current(&self, shell: &ShellState, target: &Target) -> bool {
        target.current(shell)
            && shell.core.active_account.as_ref() == Some(&self.account)
            && target.id == self.scope.file_id
            && target.team == self.scope.team_id
            && shell
                .core
                .workspaces
                .get(&self.scope.team_id)
                .is_some_and(|workspace| workspace.self_user_id == self.scope.user_id)
    }

    async fn run<T: Send + 'static>(
        self,
        action: impl FnOnce(&mut Journal, &FileScope) -> Result<T, super_platinum_core::error::AppError>
        + Send
        + 'static,
    ) -> Result<T, &'static str> {
        tokio::task::spawn_blocking(move || {
            let mut journal = if let Some(path) = self.fixture_path {
                Journal::open(path, &self.account)
            } else {
                Journal::open_default(&self.account)
            }.map_err(|_| "Saved file changes could not be opened. Try again before making another change.")?;
            action(&mut journal, &self.scope).map_err(|_| "This file change could not be saved for recovery.")
        }).await.map_err(|_| "Saved file changes could not be checked. Please try again.")?
    }

    async fn retire(self, operation_id: String) -> Result<bool, &'static str> {
        self.run(move |journal, scope| {
            if journal.retire(scope, &operation_id, now())? {
                return Ok(true);
            }
            // Another view may already have checked or retired this exact
            // operation. Never retire a replacement operation for the same file.
            Ok(journal.get(scope)?.is_some_and(|receipt| {
                receipt.operation_id == operation_id && receipt.status != Status::Uncertain
            }))
        })
        .await
    }
}

/// Immutable write identity retained by the async operation, independently of
/// the menu, mounted row, currently selected account and current workspace.
#[derive(Clone)]
pub(super) struct Pending {
    storage: Storage,
    receipt: Receipt,
}

enum Readiness {
    Ready(Pending),
    Waiting(Pending),
    Changed(Receipt),
    Missing,
}

impl Pending {
    async fn refresh(&self, at: u64) -> Result<Readiness, &'static str> {
        let receipt = self
            .storage
            .clone()
            .run(|journal, scope| journal.get(scope))
            .await?;
        let Some(receipt) = receipt else {
            return Ok(Readiness::Missing);
        };
        if receipt.operation_id != self.receipt.operation_id || receipt.status != Status::Uncertain
        {
            return Ok(Readiness::Changed(receipt));
        }
        let current = Self {
            storage: self.storage.clone(),
            receipt,
        };
        Ok(if current.receipt.retry_at > at {
            Readiness::Waiting(current)
        } else {
            Readiness::Ready(current)
        })
    }

    async fn resolve(&self, outcome: Outcome, retry_at: u64) -> Result<bool, &'static str> {
        let operation_id = self.receipt.operation_id.clone();
        self.storage
            .clone()
            .run(move |journal, scope| {
                if journal.resolve(scope, &operation_id, outcome, now(), retry_at)? {
                    return Ok(true);
                }
                let expected = match outcome {
                    Outcome::Confirmed => Status::Confirmed,
                    Outcome::Rejected => Status::Rejected,
                };
                Ok(journal.get(scope)?.is_some_and(|receipt| {
                    receipt.operation_id == operation_id && receipt.status == expected
                }))
            })
            .await
    }

    async fn defer_check(&self, retry_at: u64) -> Result<bool, &'static str> {
        let operation_id = self.receipt.operation_id.clone();
        self.storage
            .clone()
            .run(move |journal, scope| journal.defer_check(scope, &operation_id, now(), retry_at))
            .await
    }

    fn current(&self, state: Signal<ShellState>, target: &Target, ui: Signal<ActionState>) -> bool {
        state
            .try_peek()
            .ok()
            .is_some_and(|shell| self.storage.current(&shell, target))
            && ui.try_peek().ok().is_some_and(|ui| {
                ui.receipt
                    .as_ref()
                    .is_some_and(|receipt| receipt.operation_id == self.receipt.operation_id)
            })
    }
}

/// Capture before any await. Terminal receipts have no unresolved write to
/// resolve; a subsequent explicit write will obtain its own handle from begin.
pub(super) fn pending(
    state: Signal<ShellState>,
    target: &Target,
    ui: Signal<ActionState>,
) -> Option<Pending> {
    let receipt = ui.try_peek().ok()?.receipt.clone()?;
    if receipt.status != Status::Uncertain {
        return None;
    }
    let storage = state
        .try_peek()
        .ok()
        .and_then(|shell| Storage::capture(&shell, target))?;
    (receipt.scope == storage.scope).then_some(Pending { storage, receipt })
}

/// Another mounted file menu may have resolved this receipt or saved a longer
/// cooldown. Refresh before a status read without turning a stale check into a
/// newly authorized write or retargeting its immutable account/file scope.
pub(super) async fn refresh(
    state: Signal<ShellState>,
    target: &Target,
    mut ui: Signal<ActionState>,
    pending: &Pending,
) -> Option<Pending> {
    let result = pending.refresh(now()).await;
    if !pending.current(state, target, ui) {
        return None;
    }
    match result {
        Ok(Readiness::Ready(current)) => {
            if let Ok(mut ui) = ui.try_write() {
                ui.retry_at = ui.retry_at.max(current.receipt.retry_at);
                ui.receipt = Some(current.receipt.clone());
            }
            Some(current)
        }
        Ok(Readiness::Waiting(current)) => {
            if let Ok(mut ui) = ui.try_write() {
                ui.retry_at = ui.retry_at.max(current.receipt.retry_at);
                ui.receipt = Some(current.receipt);
            }
            finish(
                ui,
                "Slack asked us to wait before checking this change again.",
                true,
            );
            None
        }
        Ok(Readiness::Changed(receipt)) => {
            let next = if receipt.status == Status::Uncertain {
                restored(receipt)
            } else {
                let message = match receipt.status {
                    Status::Confirmed => "This earlier file change was already confirmed.",
                    Status::Rejected => {
                        "This earlier file change was rejected. Review the file before making a new change."
                    }
                    Status::Retired => {
                        "Tracking for this earlier change has already stopped. Its outcome in Slack was not confirmed."
                    }
                    Status::Uncertain => unreachable!(),
                };
                ActionState {
                    retry_at: receipt.retry_at,
                    receipt: Some(receipt),
                    error: Some(message.into()),
                    ..Default::default()
                }
            };
            if let Ok(mut ui) = ui.try_write() {
                let retry_at = ui.retry_at.max(next.retry_at);
                *ui = next;
                ui.retry_at = retry_at;
            }
            None
        }
        Ok(Readiness::Missing) | Err(_) => {
            finish(
                ui,
                "The saved change could not be checked. Reopen file actions before trying again.",
                true,
            );
            None
        }
    }
}

pub(super) fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn restored(receipt: Receipt) -> ActionState {
    let uncertain = receipt.status == Status::Uncertain;
    let (rename, confirm) = match &receipt.operation {
        Operation::Rename {
            original_title,
            requested_title,
        } => (
            Some(rename::Edit {
                original: original_title.clone(),
                draft: requested_title.clone(),
            }),
            false,
        ),
        Operation::Delete { .. } => (None, true),
    };
    ActionState {
        rename,
        confirm,
        uncertain,
        retry_at: receipt.retry_at,
        error: Some(
            if uncertain {
                "An earlier change is not confirmed. Check its status before making another change."
            } else {
                "Slack asked us to wait before trying this change again."
            }
            .into(),
        ),
        receipt: Some(receipt),
        ..Default::default()
    }
}

pub(super) async fn load(
    state: Signal<ShellState>,
    target: Target,
    mut ui: Signal<ActionState>,
    expanded: Signal<bool>,
    serial: Signal<u64>,
    expected: u64,
) {
    let storage = state
        .try_peek()
        .ok()
        .and_then(|shell| Storage::capture(&shell, &target));
    let result = if let Some(storage) = storage.as_ref() {
        storage
            .clone()
            .run(|journal, scope| journal.get(scope))
            .await
    } else {
        Ok(None)
    };
    let current = state.try_peek().ok().is_some_and(|shell| {
        target.current(&shell)
            && storage
                .as_ref()
                .is_none_or(|storage| storage.current(&shell, &target))
    });
    if !current
        || !expanded.try_peek().ok().is_some_and(|expanded| *expanded)
        || !serial
            .try_peek()
            .ok()
            .is_some_and(|serial| *serial == expected)
    {
        return;
    }
    let next = match result {
        Ok(Some(receipt)) if receipt.status == Status::Uncertain || receipt.retry_at > now() => {
            restored(receipt)
        }
        Ok(_) => ActionState::default(),
        Err(message) => ActionState {
            blocked: true,
            error: Some(message.into()),
            ..Default::default()
        },
    };
    if let Ok(mut current) = ui.try_write() {
        let retry_at = current.retry_at.max(next.retry_at);
        *current = next;
        current.retry_at = retry_at;
    }
}

/// Returns a handle only after this exact new operation commits to disk. The
/// caller must recheck its transport/account guard before its single dispatch.
pub(super) async fn begin(
    state: Signal<ShellState>,
    target: &Target,
    mut ui: Signal<ActionState>,
    operation: Operation,
) -> Option<Pending> {
    let Some(storage) = state
        .try_peek()
        .ok()
        .and_then(|shell| Storage::capture(&shell, target))
    else {
        finish(
            ui,
            "Reconnect to your account before making this file change.",
            false,
        );
        return None;
    };
    let result = storage
        .clone()
        .run(move |journal, scope| journal.begin(scope.clone(), operation, now()))
        .await;
    let current = state
        .try_peek()
        .ok()
        .is_some_and(|shell| storage.current(&shell, target));
    match result {
        Ok(Begin::Started(receipt)) => {
            if current && let Ok(mut ui) = ui.try_write() {
                ui.receipt = Some(receipt.clone());
            }
            Some(Pending { storage, receipt })
        }
        Ok(Begin::Blocked(receipt)) => {
            if current && let Ok(mut ui) = ui.try_write() {
                *ui = restored(receipt);
            }
            None
        }
        Err(message) => {
            if current {
                finish(ui, message, false);
            }
            None
        }
    }
}

/// Persists to the captured account even when the originating UI has gone away.
/// A successful result is durable acknowledgement, not permission to update a
/// different account's UI; callers still apply their usual current-context guard.
pub(super) async fn resolve(
    state: Signal<ShellState>,
    target: &Target,
    mut ui: Signal<ActionState>,
    pending: Option<&Pending>,
    outcome: Outcome,
    retry_at: u64,
) -> bool {
    let Some(pending) = pending else { return true };
    match pending.resolve(outcome, retry_at).await {
        Ok(true) => {
            if pending.current(state, target, ui)
                && let Ok(mut ui) = ui.try_write()
            {
                if let Some(receipt) = ui.receipt.as_mut() {
                    receipt.status = match outcome {
                        Outcome::Confirmed => Status::Confirmed,
                        Outcome::Rejected => Status::Rejected,
                    };
                    receipt.retry_at = receipt.retry_at.max(retry_at);
                    receipt.updated_at = receipt.updated_at.max(now());
                }
                ui.retry_at = ui.retry_at.max(retry_at);
                ui.uncertain = false;
            }
            true
        }
        _ => {
            if pending.current(state, target, ui) {
                finish(
                    ui,
                    "Slack's response could not be saved. Check the status again before making another change.",
                    true,
                );
            }
            false
        }
    }
}

/// Handle rate limits from status reads too; no automatic write or read retry.
pub(super) async fn cooldown(
    state: Signal<ShellState>,
    target: &Target,
    mut ui: Signal<ActionState>,
    pending: Option<&Pending>,
    error: &Error,
) -> bool {
    let Error::RateLimited { retry_after_secs } = error else {
        return false;
    };
    let retry_at = now().saturating_add(retry_after_secs.unwrap_or(30));
    let saved = match pending {
        Some(pending) => matches!(pending.defer_check(retry_at).await, Ok(true)),
        None => true,
    };
    let current = match pending {
        Some(pending) => pending.current(state, target, ui),
        None => state
            .try_peek()
            .ok()
            .is_some_and(|shell| target.current(&shell)),
    };
    if current && let Ok(mut ui) = ui.try_write() {
        ui.retry_at = ui.retry_at.max(retry_at);
        ui.busy = false;
        if pending.is_some() {
            ui.uncertain = true;
            if let Some(receipt) = ui.receipt.as_mut() {
                receipt.retry_at = receipt.retry_at.max(retry_at);
            }
        }
        ui.error = Some(if saved {
            "Slack is limiting checks. Wait a moment before checking this file again."
        } else {
            "Slack asked us to wait, but the retry time could not be saved. Check the status again before making another change."
        }.into());
    }
    true
}

pub(super) async fn retire(state: Signal<ShellState>, target: Target, mut ui: Signal<ActionState>) {
    let Some((receipt, retry_at)) = ui
        .try_peek()
        .ok()
        .and_then(|ui| ui.receipt.clone().map(|receipt| (receipt, ui.retry_at)))
    else {
        return;
    };
    let Some(storage) = state
        .try_peek()
        .ok()
        .and_then(|shell| Storage::capture(&shell, &target))
    else {
        return;
    };
    if receipt.scope != storage.scope {
        return;
    }
    if let Ok(mut ui) = ui.try_write() {
        ui.busy = true;
    }
    let operation_id = receipt.operation_id;
    let result = storage.clone().retire(operation_id.clone()).await;
    if !state
        .try_peek()
        .ok()
        .is_some_and(|shell| storage.current(&shell, &target))
    {
        return;
    }
    if !ui.try_peek().ok().is_some_and(|ui| {
        ui.receipt
            .as_ref()
            .is_some_and(|receipt| receipt.operation_id == operation_id)
    }) {
        return;
    }
    match result {
        Ok(true) => {
            if let Ok(mut ui) = ui.try_write() {
                *ui = ActionState {
                    retry_at,
                    ..Default::default()
                };
            }
        }
        _ => finish(
            ui,
            "The saved change could not be cleared. You can still check its status.",
            true,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Store {
        dir: std::path::PathBuf,
        storage: Storage,
    }
    impl Store {
        fn new() -> Self {
            static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
            let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir = std::env::temp_dir().join(format!(
                "file-recovery-test-{}-{nanos}-{id}",
                std::process::id()
            ));
            Self {
                storage: Storage {
                    account: format!("00000000-0000-4000-8000-{id:012x}"),
                    scope: FileScope {
                        team_id: "T1".into(),
                        user_id: "U1".into(),
                        file_id: "F1".into(),
                    },
                    fixture_path: Some(dir.join("receipts.sqlite")),
                },
                dir,
            }
        }
        fn open(&self) -> Journal {
            Journal::open(
                self.storage.fixture_path.as_ref().unwrap(),
                &self.storage.account,
            )
            .unwrap()
        }
        fn begin(&self) -> Pending {
            let Begin::Started(receipt) = self
                .open()
                .begin(
                    self.storage.scope.clone(),
                    Operation::Delete {
                        original_title: "Original".into(),
                    },
                    now(),
                )
                .unwrap()
            else {
                panic!("new receipt expected")
            };
            Pending {
                storage: self.storage.clone(),
                receipt,
            }
        }
    }
    impl Drop for Store {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[tokio::test]
    async fn immutable_handle_resolves_after_caller_state_is_gone_and_is_idempotent() {
        let store = Store::new();
        let original = store.begin();
        let retained = original.clone();
        drop(original);
        let other_account = Store::new();
        let other = other_account.begin();
        assert!(retained.resolve(Outcome::Confirmed, 0).await.unwrap());
        assert!(retained.resolve(Outcome::Confirmed, 0).await.unwrap());
        assert!(!retained.resolve(Outcome::Rejected, 0).await.unwrap());
        assert_eq!(
            store
                .open()
                .get(&store.storage.scope)
                .unwrap()
                .unwrap()
                .status,
            Status::Confirmed
        );
        assert_eq!(
            other_account
                .open()
                .get(&other.storage.scope)
                .unwrap()
                .unwrap()
                .status,
            Status::Uncertain
        );
    }

    #[tokio::test]
    async fn stale_resolve_retire_and_cooldown_cannot_touch_replacement() {
        let store = Store::new();
        let first = store.begin();
        assert!(
            first
                .storage
                .clone()
                .retire(first.receipt.operation_id.clone())
                .await
                .unwrap()
        );
        assert!(
            first
                .storage
                .clone()
                .retire(first.receipt.operation_id.clone())
                .await
                .unwrap()
        );
        let replacement = store.begin();
        assert!(!first.resolve(Outcome::Confirmed, 0).await.unwrap());
        assert!(!first.defer_check(now() + 100).await.unwrap());
        assert!(
            !first
                .storage
                .clone()
                .retire(first.receipt.operation_id.clone())
                .await
                .unwrap()
        );
        assert_eq!(
            store.open().get(&store.storage.scope).unwrap().unwrap(),
            replacement.receipt
        );
    }

    #[tokio::test]
    async fn stale_view_refresh_observes_other_views_cooldown_and_terminal_result() {
        let store = Store::new();
        let first_view = store.begin();
        let stale_view = first_view.clone();
        let deadline = now() + 120;
        assert_eq!(stale_view.receipt.retry_at, 0);
        assert!(first_view.defer_check(deadline).await.unwrap());
        let Readiness::Waiting(refreshed) = stale_view.refresh(now()).await.unwrap() else {
            panic!("a stale view must not authorize a status read during the saved cooldown");
        };
        assert_eq!(refreshed.receipt.retry_at, deadline);
        assert_eq!(
            refreshed.receipt.operation_id,
            stale_view.receipt.operation_id
        );
        assert!(
            first_view
                .resolve(Outcome::Rejected, deadline)
                .await
                .unwrap()
        );
        let Readiness::Changed(receipt) = stale_view.refresh(now()).await.unwrap() else {
            panic!("a definite rejection must stop an old uncertain check");
        };
        assert_eq!(receipt.status, Status::Rejected);
        assert_eq!(receipt.retry_at, deadline);
        let Begin::Started(replacement) = store
            .open()
            .begin(
                store.storage.scope.clone(),
                Operation::Rename {
                    original_title: "Original".into(),
                    requested_title: "New".into(),
                },
                deadline + 1,
            )
            .unwrap()
        else {
            panic!("explicit new intent after cooldown")
        };
        let Readiness::Changed(receipt) = stale_view.refresh(deadline + 2).await.unwrap() else {
            panic!("an old status check must not proceed against replacement intent");
        };
        assert_eq!(receipt.operation_id, replacement.operation_id);
        assert_ne!(receipt.operation_id, stale_view.receipt.operation_id);
    }

    #[tokio::test]
    async fn cooldown_remains_uncertain_across_reopen_and_already_resolved_retirement_is_safe() {
        let store = Store::new();
        let pending = store.begin();
        let deadline = now() + 120;
        assert!(pending.defer_check(deadline).await.unwrap());
        let receipt = store.open().get(&store.storage.scope).unwrap().unwrap();
        assert_eq!(receipt.status, Status::Uncertain);
        assert_eq!(receipt.retry_at, deadline);
        assert!(pending.resolve(Outcome::Rejected, deadline).await.unwrap());
        assert!(
            pending
                .storage
                .clone()
                .retire(pending.receipt.operation_id.clone())
                .await
                .unwrap()
        );
        assert_eq!(
            store
                .open()
                .get(&store.storage.scope)
                .unwrap()
                .unwrap()
                .status,
            Status::Rejected
        );
    }

    #[test]
    fn terminal_receipt_restoration_never_claims_uncertainty() {
        let store = Store::new();
        let pending = store.begin();
        let mut receipt = pending.receipt;
        for status in [Status::Confirmed, Status::Rejected, Status::Retired] {
            receipt.status = status;
            receipt.retry_at = now() + 30;
            let ui = restored(receipt.clone());
            assert!(!ui.uncertain);
            assert_eq!(ui.retry_at, receipt.retry_at);
        }
    }
}
