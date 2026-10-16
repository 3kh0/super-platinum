//! Explicit schema edits; retained receipts never resend a write.
use super::*;
use serde_json::json;
use super_platinum_core::slack_lists::{ColumnRename, ColumnRenameAttempt, ColumnRenamePermit};
pub(crate) mod filter;
pub(crate) mod fixture;
pub(crate) mod grouping;
mod network;
mod owner;
pub(crate) mod precision;
mod registry;
pub(crate) mod sort;
mod target;
pub(crate) mod text_create;
pub(crate) mod view_create;
pub(crate) mod visibility;
pub(crate) use target::Target;
use target::{Input, Intent, Permit};
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
    pub text_creation: bool,
    pub field_choice: Option<text_create::Choice>,
    pub select_draft: Option<text_create::select::Draft>,
    pub observed_field: Option<text_create::Observed>,
    pub grouping: String,
    pub grouping_options: Vec<(String, String)>,
    pub can_navigate: bool,
    pub noun: &'static str,
    pub limit: usize,
    pub name: String,
    pub precision: Option<precision::Presentation>,
    pub visibility: Option<visibility::Presentation>,
    pub filter: Option<filter::Presentation>,
    pub sort: Option<sort::Presentation>,
    pub view_grouping: Option<grouping::Presentation>,
    pub before: String,
    pub stage: Stage,
    pub message: String,
    pub blocked: bool,
    pub current_review: bool,
    pub can_check: bool,
    pub checkable: bool,
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
        let population_current = text_create::population_current(shell, e);
        let editing_ready = owner::available(shell)
            && original_current
            && selection_current(shell, e)
            && population_current;
        let message = if e.target.is_grouping()
            && e.stage == Stage::Observed
            && (!e
                .review
                .as_ref()
                .is_some_and(|p| p.navigation_current(shell))
                || !grouping::projection_current(shell, &e.target))
        {
            "This view changed since grouping was observed. Check current grouping before finishing this review. The request will not be repeated.".into()
        } else if e.target.is_text_creation()
            && e.stage == Stage::Observed
            && !e
                .review
                .as_ref()
                .is_some_and(|p| p.navigation_current(shell))
        {
            "This List view changed since the field was observed. Check current fields before finishing this review. The create request will not be repeated.".into()
        } else if e.stage == Stage::Editing && e.target.is_text_creation() && !population_current {
            "The loaded items changed. Your field name is kept. Cancel, refresh this List, then review a new field.".into()
        } else if e.target.is_text_creation()
            && !population_current
            && matches!(e.stage, Stage::Uncertain | Stage::Conflict)
        {
            "The loaded items changed since this request. Check current fields to review the current List; the original creation cannot be confirmed against changed items and will not be repeated.".into()
        } else if e.stage == Stage::Editing && !editing_ready {
            "This List changed or is no longer ready. Cancel, close the List, refresh it, then reopen the editor. Your input is kept until you cancel.".into()
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
            text_creation: e.target.is_text_creation(),
            field_choice: e.target.is_text_creation().then_some(e.field_choice),
            select_draft: (e.target.is_text_creation()
                && e.field_choice.kind
                    == super_platinum_core::slack_lists::ColumnCreateKind::Select)
                .then(|| e.select_draft.clone()),
            observed_field: e
                .review
                .as_ref()
                .filter(|p| p.navigation_current(shell))
                .and_then(|_| e.observed_field.clone()),
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
            name: e.input.name().unwrap_or_default().into(),
            precision: precision::presentation(shell, e),
            visibility: visibility::presentation(shell, e),
            filter: filter::presentation(shell, e),
            sort: sort::presentation(shell, e),
            view_grouping: grouping::presentation(shell, e),
            before: e.before.clone(),
            stage: e.stage,
            message,
            blocked,
            can_check: !blocked && owner::readable(shell),
            checkable: checkable(shell, e),
            can_confirm: e.stage == Stage::Editing && editing_ready && !blocked,
            current_fields,
            current_review: if e.target.is_grouping() {
                grouping::review_current(shell, e)
            } else {
                e.review.as_ref().is_some_and(|p| {
                    if e.target.creation().is_some() || e.target.is_text_creation() {
                        p.navigation_current(shell)
                    } else {
                        p.current(shell)
                    }
                })
            },
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
    loaded
        .document
        .column(column)
        .is_some_and(ColumnRename::supports)
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
pub(crate) fn open_shell(shell: &mut ShellState, column: &str, visit: u64) -> bool {
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
    let original_records = if target.is_text_creation() {
        let Some(records) = text_create::capture_population(shell, &target) else {
            return false;
        };
        Some(records)
    } else {
        None
    };
    let before = target.name(document).unwrap().to_owned();
    let input = if target.is_text_creation() {
        Input::NewTextField(String::new())
    } else if let Target::Filter(view) = &target {
        let Some(value) = super_platinum_core::slack_lists::ViewFilter::current(document, view)
        else {
            return false;
        };
        Input::Filter(value)
    } else if let Target::Grouping(view) = &target {
        let Some(value) = super_platinum_core::slack_lists::ViewGrouping::current(document, view)
        else {
            return false;
        };
        Input::Grouping(value)
    } else if let Target::Sort(view) = &target {
        let Some(value) = super_platinum_core::slack_lists::ViewSort::current(document, view)
        else {
            return false;
        };
        Input::Sort(value)
    } else if let Target::Visibility { view, column } = &target {
        let Some(value) =
            super_platinum_core::slack_lists::ViewFieldVisibility::current(document, view, column)
        else {
            return false;
        };
        Input::Visibility(value)
    } else if target.is_precision() {
        let Some(value) = precision::current(document, target.id().unwrap()) else {
            return false;
        };
        Input::Precision(value)
    } else {
        Input::Name(before.clone())
    };
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
                original_records,
                observed_field: None,
                field_choice: text_create::Choice::initial(),
                select_draft: Default::default(),
                filter_rows: filter::rows_for(&input),
                filter_capture: None,
                input,
                visibility_choice: next_id(),
                filter_choice: next_id(),
                sort_choice: next_id(),
                grouping_choice: next_id(),
                before,
                original: document.clone(),
                intent: None,
                stage: Stage::Editing,
                message: if target.is_text_creation() {
                    "Create one field shared with everyone who has access to this List. Existing items and view settings are kept.".into()
                } else if target.is_filter() {
                    "Choose conditions that must all match, then Save for everyone. Cancel leaves the saved view unchanged.".into()
                } else if target.is_grouping() {
                    "Choose a field, or None, then Save for everyone. Cancel leaves the shared view unchanged.".into()
                } else if target.is_sort() {
                    "Choose one Number field and direction, or No sort, then Save for everyone. Cancel leaves the saved view unchanged.".into()
                } else if target.is_visibility() {
                    "Choose one field and its visibility, then Save for everyone. Cancel leaves the saved view unchanged.".into()
                } else if target.is_precision() {
                    "Choose the number of decimals, then save. Stored numbers are unchanged.".into()
                } else {
                    format!(
                        "Choose a {} name, then confirm.{}",
                        target.noun(),
                        if target.is_view() {
                            " This changes the shared view for everyone with access."
                        } else {
                            ""
                        }
                    )
                },
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
            if !e.target.is_text_creation()
                && !e.target.is_filter()
                && !e.target.is_sort()
                && !e.target.is_grouping()
                && !e.target.is_precision()
                && !e.target.is_visibility()
                && value.encode_utf16().count() <= e.target.limit()
            {
                e.input = Input::Name(value);
            }
        }
    });
    drop(shell);
    state.write();
}
pub(crate) fn close(mut state: Signal<ShellState>, serial: u64) {
    let changed = close_entry(&state.peek(), serial);
    if changed {
        state.write();
    }
}
fn close_entry(shell: &ShellState, serial: u64) -> bool {
    let Some(key) = Key::current(&shell) else {
        return false;
    };
    let can = entry(&shell).is_some_and(|e| {
        e.serial == serial
            && (!e.target.is_grouping()
                || e.stage != Stage::Observed
                || grouping::review_current(shell, &e))
            && (!e.target.is_text_creation()
                || e.stage != Stage::Observed
                || e.review
                    .as_ref()
                    .is_some_and(|p| p.navigation_current(shell)))
            && (matches!(
                e.stage,
                Stage::Editing | Stage::NotRequested | Stage::Observed
            ) || (matches!(e.stage, Stage::Conflict | Stage::Available)
                && e.review.as_ref().is_some_and(|p| {
                    if e.target.creation().is_some() || e.target.is_text_creation() {
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
    can
}
pub(crate) fn confirm(mut state: Signal<ShellState>, serial: u64) {
    // Every rendered Filter Save uses the owned complete-set snapshot.
    if entry(&state.peek()).is_some_and(|e| e.target.is_filter()) {
        return;
    }
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
        || old.filter_capture.is_some()
        || (old.target.is_grouping() && !grouping::projection_current(shell, &old.target))
        || !selection_current(shell, &old)
        || !owner::available(shell)
        || registry::blocked(&key.owner)
    {
        return None;
    }
    let loaded = shell.slack_lists.view.as_ref()?.loaded.as_ref()?;
    if loaded.document != old.original || !text_create::population_current(shell, &old) {
        return None;
    }
    let intent = match text_create::capture(shell, &old, loaded) {
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
    let permit = text_create::admit(shell, intent, loaded)?;
    with_registry(|r| {
        let e = r.entries.get_mut(&guard.key).unwrap();
        e.stage = Stage::Saving;
        e.message = if e.target.is_text_creation() {
            format!(
                "Creating {} field...",
                text_create::label(e.field_choice.kind)
            )
        } else if e.target.is_filter() {
            "Saving shared filter...".into()
        } else if e.target.is_grouping() {
            "Saving shared grouping...".into()
        } else if e.target.is_sort() {
            "Saving shared sort...".into()
        } else if e.target.is_visibility() {
            "Saving field visibility...".into()
        } else if e.target.is_precision() {
            "Saving number format...".into()
        } else if e.target.creation().is_some() {
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
            e.message=if e.target.is_text_creation(){if not_requested{"The create request was not sent. Close this editor and review a new attempt."}else{"The field request is not confirmed. Check current fields explicitly. This request will not be sent again."}}else if e.target.is_filter(){if not_requested{"The filter request was not sent. Close this editor and review a new attempt."}else{"The shared filter is not confirmed. Check the current filter explicitly. This request will not be sent again."}}else if e.target.is_grouping(){if not_requested{"The grouping request was not sent. Close this editor and review a new attempt."}else{"The shared grouping is not confirmed. Check current grouping explicitly. This request will not be sent again."}}else if e.target.is_sort(){if not_requested{"The sort request was not sent. Close this editor and review a new attempt."}else{"The shared sort is not confirmed. Check the current sort explicitly. This request will not be sent again."}}else if e.target.is_visibility(){if not_requested{"The visibility request was not sent. Close this editor and review a new attempt."}else{"Field visibility is not confirmed. Check the current view explicitly. This request will not be sent again."}}else if e.target.is_precision(){if not_requested{"The number format request was not sent. Close this editor and review a new attempt."}else{"The number format is not confirmed. Check the current format explicitly. This request will not be sent again."}}else if e.target.creation().is_some(){if not_requested{"The create request was not sent. Close this editor and review a new attempt."}else{"The create request is not confirmed. Check current views explicitly. This request will not be sent again."}}else if not_requested{"The rename was not sent. Close this editor and review the name again."}else{"The rename is not confirmed. Check the current name before doing anything else. This request will not be sent again."}.into();
        }
    });
    // Deliberately no canonical projection and no automatic read or resend.
    let _ = shell;
}
fn checkable(shell: &ShellState, e: &registry::Entry) -> bool {
    matches!(
        e.stage,
        Stage::Uncertain | Stage::Conflict | Stage::Available
    ) || (e.target.is_grouping()
        && e.stage == Stage::Observed
        && !grouping::review_current(shell, e))
        || (e.target.is_text_creation()
            && e.stage == Stage::Observed
            && !e
                .review
                .as_ref()
                .is_some_and(|p| p.navigation_current(shell)))
}
pub(crate) fn check(mut state: Signal<ShellState>, serial: u64) {
    let guard = {
        let shell = state.peek();
        let Some(key) = Key::current(&shell) else {
            return;
        };
        let Some(e) = entry(&shell) else { return };
        if e.serial != serial
            || !checkable(&shell, &e)
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
            e.observed_field = None;
            e.message = if e.target.is_text_creation() {
                "Checking current fields...".into()
            } else if e.target.is_filter() {
                "Checking current shared filter...".into()
            } else if e.target.is_grouping() {
                "Checking current shared grouping...".into()
            } else if e.target.is_sort() {
                "Checking current shared sort...".into()
            } else if e.target.is_visibility() {
                "Checking current field visibility...".into()
            } else if e.target.is_precision() {
                "Checking the current number format...".into()
            } else {
                format!("Checking the current {} name...", e.target.noun())
            };
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
    let mut observed_field = None;
    let message = match result {
        Ok(loaded) if current => {
            let intent = old.intent.as_ref().unwrap();
            if !network::same_incarnation(intent.original(), &loaded.document) {
                "The List identity changed. The original request remains unconfirmed."
            } else {
                available = view_create::matching(shell, intent, &loaded);
                observed_field = text_create::matching(shell, intent, &loaded);
                stage = if available.is_some() {
                    Stage::Available
                } else if observed_field.is_some() || intent.observes_desired(&loaded.document) {
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
                if old.target.is_grouping()
                    && stage == Stage::Observed
                    && !grouping::projection_current(shell, &old.target)
                {
                    stage = Stage::Conflict;
                }
                proof = owner::Review::capture(shell);
                applied = true;
                if stage == Stage::Available {
                    "A matching new view is now available. Opening it is navigation, not proof this request created it."
                } else if old.target.is_text_creation() {
                    if stage == Stage::Observed {
                        match old.field_choice.kind {
                            super_platinum_core::slack_lists::ColumnCreateKind::Text => {
                                "A matching Text field is now visible in this Table."
                            }
                            super_platinum_core::slack_lists::ColumnCreateKind::NumberDefault0 => {
                                "A matching Number field is now visible in this Table."
                            }
                            super_platinum_core::slack_lists::ColumnCreateKind::Checkbox => {
                                "A matching Checkbox field is now visible in this Table."
                            }
                            super_platinum_core::slack_lists::ColumnCreateKind::Select => {
                                "A matching Select field is now visible in this Table."
                            }
                            super_platinum_core::slack_lists::ColumnCreateKind::DateSingleDefault => {
                                "A matching Date field is now visible in this Table."
                            }
                        }
                    } else {
                        "No isolated matching field is visible in this Table. Review the returned fields. The earlier request may still arrive; closing does not repeat or undo it."
                    }
                } else if old.target.creation().is_some() {
                    "Review the returned views below. No isolated matching addition is available. The earlier create may still arrive; closing does not repeat or undo it."
                } else if old.target.is_filter() {
                    if stage == Stage::Observed {
                        "The current shared filter matches your choice. Stored values are unchanged."
                    } else {
                        "The returned view differs from your filter choice. Review the current state. The earlier request may still arrive; closing does not repeat or undo it."
                    }
                } else if old.target.is_grouping() {
                    if stage == Stage::Observed {
                        "The current shared grouping matches your choice. Stored values are unchanged."
                    } else {
                        "The returned grouping or its current projection differs from your choice. Review the actual view. Closing does not repeat or undo the earlier request."
                    }
                } else if old.target.is_sort() {
                    if stage == Stage::Observed {
                        "The current shared sort matches your choice. Stored values are unchanged."
                    } else {
                        "The returned view differs from your sort choice. Review the current state. The earlier request may still arrive; closing does not repeat or undo it."
                    }
                } else if old.target.is_visibility() {
                    if stage == Stage::Observed {
                        "The current field visibility matches your choice."
                    } else {
                        "The returned view differs from your visibility choice. Review the current state. The earlier request may still arrive; closing does not repeat or undo it."
                    }
                } else if old.target.is_precision() {
                    if stage == Stage::Observed {
                        "The current number format matches your choice. Stored numbers are unchanged."
                    } else {
                        "The returned schema differs from your number format choice. Review the current format below. The earlier request may still arrive; closing does not repeat or undo it."
                    }
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
        e.observed_field = observed_field;
        e.message = message.into();
    });
    applied
}
