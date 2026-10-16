//! Workspace recovery inventory, including files no longer present in Files.
use super::{Actions, Target};
use crate::state::ShellState;
use dioxus::prelude::*;
use super_platinum_core::operation_receipts::{Journal, Operation, Receipt, Status};

#[derive(Clone, PartialEq, Eq)]
struct Scope {
    account: String,
    epoch: u64,
    team: String,
    user: String,
}
impl Scope {
    fn capture(shell: &ShellState) -> Option<Self> {
        let team = shell.core.active_team.as_ref()?;
        let user = &shell.core.workspaces.get(team)?.self_user_id;
        if user.is_empty() {
            return None;
        }
        Some(Self {
            account: shell.core.active_account.clone()?,
            epoch: shell.core.account_epoch,
            team: team.clone(),
            user: user.clone(),
        })
    }

    fn current(&self, shell: &ShellState) -> bool {
        Self::capture(shell).as_ref() == Some(self)
    }

    fn target(&self, receipt: &Receipt) -> Option<Target> {
        if receipt.scope.team_id != self.team
            || receipt.scope.user_id != self.user
            || receipt.status != Status::Uncertain
        {
            return None;
        }
        let title = match &receipt.operation {
            Operation::Rename { original_title, .. } | Operation::Delete { original_title } => {
                original_title
            }
        };
        Some(Target {
            id: receipt.scope.file_id.clone(),
            title: title.clone(),
            link: None,
            owned: true,
            renameable: matches!(receipt.operation, Operation::Rename { .. }),
            attachment: None,
            epoch: self.epoch,
            team: self.team.clone(),
        })
    }
}

#[derive(Default)]
struct Inventory {
    scope: Option<Scope>,
    generation: u64,
    loading: bool,
    rows: Vec<Receipt>,
    error: Option<&'static str>,
}

fn load(state: Signal<ShellState>, mut inventory: Signal<Inventory>, scope: Scope) {
    let generation = {
        let mut inventory = inventory.write();
        if inventory.scope.as_ref() == Some(&scope) && inventory.loading {
            return;
        }
        inventory.generation = inventory.generation.wrapping_add(1);
        inventory.scope = Some(scope.clone());
        inventory.loading = true;
        inventory.error = None;
        inventory.rows.clear();
        inventory.generation
    };
    // A disconnected fixture must never fall through to real account storage.
    let fixture = crate::fixture::is_fixture();
    let fixture_path = fixture
        .then(|| std::env::var_os("SUPER_PLATINUM_FILE_RECOVERY_DB"))
        .flatten();
    dioxus::core::spawn_forever(async move {
        let worker_scope = scope.clone();
        let result = tokio::task::spawn_blocking(move || {
            let journal = if fixture {
                let path = fixture_path.ok_or_else(|| {
                    std::io::Error::other("fixture recovery storage is not configured")
                })?;
                Journal::open(path, &worker_scope.account)?
            } else {
                Journal::open_default(&worker_scope.account)?
            };
            journal.unresolved(&worker_scope.team, &worker_scope.user)
        })
        .await;
        if !scope.current(&state.peek()) {
            return;
        }
        let Ok(mut inventory) = inventory.try_write() else {
            return;
        };
        if inventory.generation != generation || inventory.scope.as_ref() != Some(&scope) {
            return;
        }
        inventory.loading = false;
        match result {
            Ok(Ok(rows)) => inventory.rows = rows,
            _ => inventory.error = Some("Saved file changes could not be loaded. Try again."),
        }
    });
}

#[component]
pub(crate) fn RecoveryList(state: Signal<ShellState>) -> Element {
    let mut expanded = use_signal(|| false);
    let inventory = use_signal(Inventory::default);
    let scope = Scope::capture(&state.read());
    let Some(scope) = scope else { return rsx! {} };
    let snapshot = inventory.read();
    let visible = expanded() && snapshot.scope.as_ref() == Some(&scope);
    let toggle_scope = scope.clone();
    let refresh_scope = scope.clone();
    let panel_id = format!("file-recovery-{}-{}", scope.epoch, scope.team);
    rsx! {
        section {class:"file-recovery","aria-label":"Unconfirmed file changes",
            div {class:"file-recovery-controls",
                button {
                    class:"later-refresh", "aria-expanded":visible, "aria-controls":"{panel_id}",
                    onclick:move |_| {
                        if visible {expanded.set(false);} else {expanded.set(true);load(state,inventory,toggle_scope.clone());}
                    },
                    "Unconfirmed file changes"
                }
                if visible {
                    button {class:"later-refresh",disabled:snapshot.loading,onclick:move |_|load(state,inventory,refresh_scope.clone()),"Refresh changes"}
                }
            }
            if visible {
                div {id:"{panel_id}",class:"file-recovery-list",role:"region","aria-label":"Saved file changes","aria-busy":snapshot.loading,
                    p {class:"file-action-note","These changes may already be applied in Slack. Check their status before making another change."}
                    if snapshot.loading {p {class:"files-status",role:"status","Loading saved changes…"}}
                    if let Some(error)=snapshot.error {p {class:"file-action-error",role:"alert","{error}"}}
                    if !snapshot.loading && snapshot.error.is_none() && snapshot.rows.is_empty() {
                        p {class:"files-status",role:"status","No unconfirmed file changes."}
                    }
                    div {role:"list","aria-label":"Unconfirmed changes",
                        for receipt in &snapshot.rows {
                            if let Some(target)=scope.target(receipt) {
                                div {class:"file-result",role:"listitem",key:"{receipt.operation_id}",
                                    div {class:"files-row-body file-recovery-description",
                                        span {class:"files-title","{target.title}"}
                                        match &receipt.operation {
                                            Operation::Rename {requested_title,..} => rsx! {span {class:"file-action-note","Requested name: {requested_title}"}},
                                            Operation::Delete {..} => rsx! {span {class:"file-action-note","Deletion not confirmed"}},
                                        }
                                    }
                                    Actions {state,target,recovery_list:true}
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super_platinum_core::operation_receipts::FileScope;

    fn shell() -> ShellState {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        shell.core.active_account = Some("00000000-0000-4000-8000-000000000001".into());
        shell
    }

    #[test]
    fn inventory_scope_rejects_account_epoch_workspace_and_user_changes() {
        let mut shell = shell();
        let scope = Scope::capture(&shell).unwrap();
        assert!(scope.current(&shell));
        shell.core.account_epoch += 1;
        assert!(!scope.current(&shell));
        shell.core.account_epoch = scope.epoch;
        shell.core.active_account = Some("another-account".into());
        assert!(!scope.current(&shell));
        shell.core.active_account = Some(scope.account.clone());
        shell.core.active_team = Some("another-team".into());
        assert!(!scope.current(&shell));
        shell.core.active_team = Some(scope.team.clone());
        shell
            .core
            .workspaces
            .get_mut(&scope.team)
            .unwrap()
            .self_user_id = "another-user".into();
        assert!(!scope.current(&shell));
    }

    #[test]
    fn inventory_targets_keep_original_intent_even_after_file_disappears() {
        let mut shell = shell();
        let scope = Scope::capture(&shell).unwrap();
        let mut receipt = Receipt {
            operation_id: "operation".into(),
            scope: FileScope {
                team_id: scope.team.clone(),
                user_id: scope.user.clone(),
                file_id: "F-gone".into(),
            },
            operation: Operation::Rename {
                original_title: "Original".into(),
                requested_title: "Requested · 日本語".into(),
            },
            status: Status::Uncertain,
            created_at: 1,
            updated_at: 1,
            retry_at: 0,
        };
        shell
            .core
            .workspaces
            .get_mut(&scope.team)
            .unwrap()
            .deleted_files
            .insert("F-gone".into());
        let target = scope.target(&receipt).unwrap();
        assert_eq!(target.title, "Original");
        assert_eq!(target.id, "F-gone");
        assert!(target.owned && target.renameable);
        assert!(target.link.is_none());
        assert_eq!(target.epoch, scope.epoch);
        receipt.operation = Operation::Delete {
            original_title: "Original".into(),
        };
        assert!(!scope.target(&receipt).unwrap().renameable);
        receipt.scope.user_id = "another-user".into();
        assert!(scope.target(&receipt).is_none());
        receipt.scope.user_id = scope.user.clone();
        receipt.status = Status::Confirmed;
        assert!(scope.target(&receipt).is_none());
    }
}
