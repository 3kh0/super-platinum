//! Explicit field/saved-view name edits; retained receipts never resend a write.
use super::*;
use serde_json::json;
use super_platinum_core::slack_lists::{ColumnRename, ColumnRenameAttempt, ColumnRenamePermit};
pub(crate) mod fixture;
mod network;
mod owner;
mod registry;
mod target;
pub(crate) mod view_create;
pub(crate) use target::Target;
use target::{Intent, Permit};
#[cfg(test)]
mod tests;
use owner::Guard;
pub(crate) use registry::Stage;
use registry::{Key, entry, with_registry};

pub(super) fn poll(shell: &mut ShellState) -> bool {
    poll_at(shell, std::time::Instant::now())
}
fn poll_at(shell: &mut ShellState, now: std::time::Instant) -> bool {
    let blocked = Key::current(shell).is_some_and(|k| registry::blocked_at(&k.owner, now));
    let changed = shell.slack_lists.rename_cooldown_visible != blocked;
    shell.slack_lists.rename_cooldown_visible = blocked;
    changed
}

#[derive(Clone)]
pub(crate) struct Presentation {
    pub serial: u64,
    pub column: String,
    pub view: Option<String>,
    pub creation: Option<super_platinum_core::slack_lists::ViewLayout>,
    pub grouping: String,
    pub grouping_options: Vec<(String, String)>,
    pub can_navigate: bool,
    pub noun: &'static str,
    pub limit: usize,
    pub name: String,
    pub before: String,
    pub stage: Stage,
    pub message: String,
    pub blocked: bool,
    pub current_review: bool,
    pub can_check: bool,
    pub can_confirm: bool,
    pub current_fields: String,
}
pub(super) fn creation_cooldown(scope: &Scope) -> bool {
    registry::blocked(&scope.stable_owner())
}
pub(super) fn record_creation_limit(scope: &Scope, error: &Error) {
    registry::record(&scope.stable_owner(), error);
}
pub(crate) fn blocks_mutation(shell: &ShellState) -> bool {
    Key::current(shell).is_some_and(|key| with_registry(|r| r.entries.contains_key(&key)))
}
pub(super) fn blocks_read(shell: &ShellState) -> bool {
    Key::current(shell).is_some_and(|key| {
        with_registry(|r| {
            r.entries.get(&key).is_some_and(|e| {
                matches!(e.stage, Stage::Preparing | Stage::Saving | Stage::Checking)
            })
        })
    })
}
pub(crate) fn presentation(shell: &ShellState) -> Option<Presentation> {
    let key = Key::current(shell)?;
    let blocked = registry::blocked(&key.owner);
    let current_fields = shell
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
        .map(|l| {
            let columns = &l.document.columns;
            let mut text = columns
                .iter()
                .take(12)
                .map(|c| format!("{} ({})", c.name, c.kind))
                .collect::<Vec<_>>()
                .join(", ");
            let names = l
                .document
                .views
                .iter()
                .take(8)
                .filter_map(|v| v["name"].as_str())
                .collect::<Vec<_>>();
            if !names.is_empty() {
                text.push_str(&format!("; views: {}", names.join(", ")));
            }
            if columns.len() > 12 {
                text.push_str(&format!("; {} more fields", columns.len() - 12));
            }
            text
        })
        .unwrap_or_default();
    with_registry(|r| {
        let e = r.entries.get(&key)?;
        let original_current = shell
            .slack_lists
            .view
            .as_ref()
            .and_then(|v| v.loaded.as_ref())
            .is_some_and(|l| l.document == e.original);
        let editing_ready =
            owner::available(shell) && original_current && selection_current(shell, e);
        let message = if e.stage == Stage::Editing && !editing_ready {
            "This List changed or is no longer ready. Cancel, close the List, refresh it, then reopen the editor. Your entered name is kept until you cancel.".into()
        } else {
            e.message.clone()
        };
        Some(Presentation {
            serial: e.serial,
            column: if e.target.is_view() {
                String::new()
            } else {
                e.target.id().unwrap_or_default().into()
            },
            view: if e.target.creation().is_none() && e.target.is_view() {
                e.target.id().map(str::to_owned)
            } else {
                None
            },
            creation: e.target.creation(),
            grouping: e.target.grouping().unwrap_or_default().into(),
            grouping_options: if e.target.creation().is_some() {
                super_platinum_core::slack_lists::ViewCreate::grouping_columns(&e.original)
                    .map(|c| (c.key.clone(), c.name.clone()))
                    .collect()
            } else {
                Vec::new()
            },
            can_navigate: e.stage == Stage::Available
                && e.review
                    .as_ref()
                    .is_some_and(|p| p.navigation_current(shell)),
            noun: e.target.noun(),
            limit: e.target.limit(),
            name: e.name.clone(),
            before: e.before.clone(),
            stage: e.stage,
            message,
            blocked,
            can_check: !blocked && owner::readable(shell),
            can_confirm: e.stage == Stage::Editing && editing_ready && !blocked,
            current_fields,
            current_review: e.review.as_ref().is_some_and(|p| {
                if e.target.creation().is_some() {
                    p.navigation_current(shell)
                } else {
                    p.current(shell)
                }
            }),
        })
    })
}
/// Presentation only. Actual admission reparses the raw document through core.
pub(crate) fn supports_column(shell: &ShellState, column: &str) -> bool {
    let Some(loaded) = shell
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
    else {
        return false;
    };
    loaded.document.column(column).is_some_and(|c| {
        c.kind == "text" && c.raw.get("options") == Some(&json!({"format":"text"}))
    })
}
pub(crate) fn can_open(shell: &ShellState, column: &str) -> bool {
    can_target(shell, &Target::Column(column.into()))
}
pub(crate) fn supports_view(shell: &ShellState) -> bool {
    selected_view(shell).is_some_and(|id| {
        shell
            .slack_lists
            .view
            .as_ref()
            .and_then(|v| v.loaded.as_ref())
            .is_some_and(|l| Target::View(id.into()).supports(&l.document))
    })
}
pub(crate) fn selection(shell: &ShellState) -> Option<u64> {
    Some(shell.slack_lists.view.as_ref()?.layout.selection_generation)
}
pub(crate) fn can_view(shell: &ShellState) -> bool {
    selected_view(shell).is_some_and(|id| can_target(shell, &Target::View(id.into())))
}
pub(crate) fn selected_view(shell: &ShellState) -> Option<&str> {
    shell.slack_lists.view.as_ref()?.layout.selected.as_deref()
}
fn can_target(shell: &ShellState, target: &Target) -> bool {
    if !owner::available(shell)
        || blocks_mutation(shell)
        || !shell
            .slack_lists
            .view
            .as_ref()
            .and_then(|v| v.loaded.as_ref())
            .is_some_and(|l| target.supports(&l.document))
    {
        return false;
    }
    let Some(key) = Key::current(shell) else {
        return false;
    };
    if registry::blocked(&key.owner) || !with_registry(|r| r.entries.len() < registry::MAX_ENTRIES)
    {
        return false;
    }
    let loaded = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap();
    loaded
        .document
        .read_only_reason(loaded.allow_lists.as_deref())
        .is_none()
        && loaded
            .personal_task_lists
            .as_ref()
            .is_some_and(|p| !p.contains(&loaded.document.id))
}
pub(crate) fn open(mut state: Signal<ShellState>, column: String, visit: u64) {
    open_shell(&mut state.write(), &column, visit);
}
pub(crate) fn open_view(mut state: Signal<ShellState>, visit: u64, selection: u64) {
    let shell = &mut state.write();
    if shell
        .slack_lists
        .view
        .as_ref()
        .is_none_or(|v| v.layout.selection_generation != selection)
    {
        return;
    }
    let Some(id) = selected_view(shell).map(str::to_owned) else {
        return;
    };
    open_target(shell, Target::View(id), visit);
}
fn open_shell(shell: &mut ShellState, column: &str, visit: u64) -> bool {
    open_target(shell, Target::Column(column.into()), visit)
}
fn selection_current(shell: &ShellState, e: &registry::Entry) -> bool {
    e.view_selection.is_none_or(|generation| {
        shell.slack_lists.view.as_ref().is_some_and(|v| {
            v.layout.selection_generation == generation && v.layout.selected == e.selected
        })
    })
}
fn open_target(shell: &mut ShellState, target: Target, visit: u64) -> bool {
    if generation(shell) != Some(visit) || !can_target(shell, &target) {
        return false;
    }
    let Some(key) = Key::current(shell) else {
        return false;
    };
    let document = &shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap()
        .document;
    if serde_json::to_vec(&document.raw).map_or(true, |b| b.len() > registry::MAX_DOCUMENT) {
        return false;
    }
    let before = target.name(document).unwrap().to_owned();
    let inserted = with_registry(|r| {
        if r.entries.len() >= registry::MAX_ENTRIES || r.entries.contains_key(&key) {
            return false;
        }
        r.entries.insert(
            key,
            registry::Entry {
                serial: next_id(),
                job: None,
                target: target.clone(),
                view_selection: target.is_view().then(|| {
                    shell
                        .slack_lists
                        .view
                        .as_ref()
                        .unwrap()
                        .layout
                        .selection_generation
                }),
                selected: selected_view(shell).map(str::to_owned),
                available: None,
                name: before.clone(),
                before,
                original: document.clone(),
                intent: None,
                stage: Stage::Editing,
                message: format!(
                    "Choose a {} name, then confirm.{}",
                    target.noun(),
                    if target.is_view() {
                        " This changes the shared view for everyone with access."
                    } else {
                        ""
                    }
                ),
                review: None,
                raw_reply: None,
            },
        );
        true
    });
    inserted
}
pub(crate) fn input(mut state: Signal<ShellState>, serial: u64, value: String) {
    if value.encode_utf16().count() > 256 {
        return;
    }
    let shell = state.peek();
    let Some(key) = Key::current(&shell) else {
        return;
    };
    with_registry(|r| {
        if let Some(e) = r
            .entries
            .get_mut(&key)
            .filter(|e| e.serial == serial && e.stage == Stage::Editing)
        {
            if value.encode_utf16().count() <= e.target.limit() {
                e.name = value;
            }
        }
    });
    drop(shell);
    state.write();
}
pub(crate) fn close(mut state: Signal<ShellState>, serial: u64) {
    let shell = state.peek();
    let Some(key) = Key::current(&shell) else {
        return;
    };
    let can = entry(&shell).is_some_and(|e| {
        e.serial == serial
            && (matches!(
                e.stage,
                Stage::Editing | Stage::NotRequested | Stage::Observed
            ) || (matches!(e.stage, Stage::Conflict | Stage::Available)
                && e.review.as_ref().is_some_and(|p| {
                    if e.target.creation().is_some() {
                        p.navigation_current(&shell)
                    } else {
                        p.current(&shell)
                    }
                })))
    });
    if can {
        with_registry(|r| {
            r.entries.remove(&key);
        });
    }
    drop(shell);
    state.write();
}
pub(crate) fn confirm(mut state: Signal<ShellState>, serial: u64) {
    let claimed = prepare(&mut state.write(), serial);
    if let Some((guard, intent)) = claimed {
        network::save(state, guard, intent);
    }
}
fn prepare(shell: &mut ShellState, serial: u64) -> Option<(Guard, Intent)> {
    let key = Key::current(shell)?;
    let old = entry(shell)?;
    if old.serial != serial
        || old.stage != Stage::Editing
        || !selection_current(shell, &old)
        || !owner::available(shell)
        || registry::blocked(&key.owner)
    {
        return None;
    }
    let loaded = shell.slack_lists.view.as_ref()?.loaded.as_ref()?;
    if loaded.document != old.original {
        return None;
    }
    let intent = match old.target.capture(
        &old.original,
        &old.name,
        loaded.allow_lists.as_deref(),
        loaded.personal_task_lists.as_ref(),
    ) {
        Ok(i) => i,
        Err(e) => {
            with_registry(|r| r.entries.get_mut(&key).unwrap().message = e.to_string());
            return None;
        }
    };
    let guard = Guard::capture(shell, key.clone(), serial)?;
    with_registry(|r| {
        let e = r.entries.get_mut(&key).unwrap();
        e.job = Some(guard.job);
        e.intent = Some(intent.clone());
        e.stage = Stage::Preparing;
        e.message = format!("Checking access and the current {}...", e.target.noun());
    });
    Some((guard, intent))
}
fn admitted(
    shell: &ShellState,
    guard: &Guard,
    intent: &Intent,
    loaded: &network::Metadata,
) -> Option<Permit> {
    if !guard.current(shell, Stage::Preparing) || registry::blocked(&guard.key.owner) {
        return None;
    }
    let permit = intent.admit(&loaded.document, loaded.allow.as_deref(), &loaded.personal)?;
    with_registry(|r| {
        let e = r.entries.get_mut(&guard.key).unwrap();
        e.stage = Stage::Saving;
        e.message = if e.target.creation().is_some() {
            "Creating view...".into()
        } else {
            format!("Renaming {}...", e.target.noun())
        };
    });
    Some(permit)
}
fn unsent(shell: &mut ShellState, guard: &Guard, message: &str) {
    with_registry(|r| {
        if let Some(e) = r.entries.get_mut(&guard.key).filter(|e| {
            e.serial == guard.serial && e.job == Some(guard.job) && e.stage == Stage::Preparing
        }) {
            e.stage = Stage::NotRequested;
            e.message = message.into();
        }
    });
    let _ = shell;
}
fn completed(
    shell: &mut ShellState,
    guard: &Guard,
    result: Result<Value, super_platinum_core::slack::transport::ListColumnRenameFailure>,
) {
    use super_platinum_core::slack::transport::ListColumnRenameFailure as Failure;
    if let Err(Failure::NotRequested(e) | Failure::Unconfirmed(e)) = &result {
        registry::record(&guard.key.owner, e);
    }
    let not_requested = matches!(&result, Err(Failure::NotRequested(_)));
    with_registry(|r| {
        if let Some(e) = r.entries.get_mut(&guard.key).filter(|e| {
            e.serial == guard.serial && e.job == Some(guard.job) && e.stage == Stage::Saving
        }) {
            e.stage = if not_requested {
                Stage::NotRequested
            } else {
                Stage::Uncertain
            };
            e.raw_reply = result.ok();
            e.message=if e.target.creation().is_some(){if not_requested{"The create request was not sent. Close this editor and review a new attempt."}else{"The create request is not confirmed. Check current views explicitly. This request will not be sent again."}}else if not_requested{"The rename was not sent. Close this editor and review the name again."}else{"The rename is not confirmed. Check the current name before doing anything else. This request will not be sent again."}.into();
        }
    });
    // Deliberately no canonical projection and no automatic read or resend.
    let _ = shell;
}
pub(crate) fn check(mut state: Signal<ShellState>, serial: u64) {
    let guard = {
        let shell = state.peek();
        let Some(key) = Key::current(&shell) else {
            return;
        };
        let Some(e) = entry(&shell) else { return };
        if e.serial != serial
            || !matches!(
                e.stage,
                Stage::Uncertain | Stage::Conflict | Stage::Available
            )
            || registry::blocked(&key.owner)
            || !owner::readable(&shell)
        {
            return;
        }
        let Some(g) = Guard::capture_read(&shell, key.clone(), serial) else {
            return;
        };
        with_registry(|r| {
            let e = r.entries.get_mut(&key).unwrap();
            e.job = Some(g.job);
            e.stage = Stage::Checking;
            e.review = None;
            e.available = None;
            e.message = format!("Checking the current {} name...", e.target.noun());
        });
        g
    };
    state.write();
    network::check(state, guard);
}
fn checked(
    shell: &mut ShellState,
    guard: &Guard,
    result: Result<network::Metadata, Error>,
) -> bool {
    if let Err(e) = &result {
        registry::record(&guard.key.owner, e);
    }
    let current = guard.current(shell, Stage::Checking);
    let mut applied = false;
    let retained = with_registry(|r| {
        r.entries
            .get(&guard.key)
            .filter(|e| {
                e.serial == guard.serial && e.job == Some(guard.job) && e.stage == Stage::Checking
            })
            .cloned()
    });
    let Some(old) = retained else { return false };
    let mut stage = Stage::Uncertain;
    let mut proof = None;
    let mut available = None;
    let message = match result {
        Ok(loaded) if current => {
            let intent = old.intent.as_ref().unwrap();
            if !network::same_incarnation(intent.original(), &loaded.document) {
                "The List identity changed. The original request remains unconfirmed."
            } else {
                available = view_create::matching(shell, intent, &loaded);
                stage = if available.is_some() {
                    Stage::Available
                } else if intent.observes_desired(&loaded.document) {
                    Stage::Observed
                } else {
                    Stage::Conflict
                };
                let view = shell.slack_lists.view.as_mut().unwrap();
                view.title = loaded.document.title.clone();
                view.loaded.as_mut().unwrap().document = loaded.document;
                view.loaded.as_mut().unwrap().allow_lists = loaded.allow;
                view.loaded.as_mut().unwrap().personal_task_lists = Some(loaded.personal);
                view.revision = next_id();
                rebuild_rows(shell);
                proof = owner::Review::capture(shell);
                applied = true;
                if stage == Stage::Available {
                    "A matching new view is now available. Opening it is navigation, not proof this request created it."
                } else if old.target.creation().is_some() {
                    "Review the returned views below. No isolated matching addition is available. The earlier create may still arrive; closing does not repeat or undo it."
                } else if stage == Stage::Observed {
                    "The current name matches your rename."
                } else {
                    "The returned schema differs from this rename. Review the current fields below. The earlier rename may still arrive. Closing this review does not repeat or undo it."
                }
            }
        }
        Ok(_) => "This List visit changed while checking. Reopen the List and check again.",
        Err(_) => {
            "Could not confirm the current schema. The earlier request will not be sent again."
        }
    };
    with_registry(|r| {
        let e = r.entries.get_mut(&guard.key).unwrap();
        e.stage = stage;
        e.review = proof;
        e.available = available;
        e.message = message.into();
    });
    applied
}
