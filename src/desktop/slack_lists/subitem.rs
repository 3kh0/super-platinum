//! Direct-root creation, retained identity, explicit readback and navigation.
use super::*;
use std::sync::Arc;
use super_platinum_core::{
    slack::transport::ListSubitemFailure,
    slack_lists::{
        ListTimestamp, Record, SubitemCreateAttempt, SubitemCreatePermit, SubitemCreationIntent,
        SubitemReadback, subitem_input_bytes,
    },
};
pub(crate) mod fixture;
mod network;
mod owner;
mod presentation;
mod registry;
#[cfg(test)]
mod tests;
use owner::{Guard, Proof};
pub(super) use presentation::{Child, project_children, project_reason};
pub(crate) use presentation::{children, presentation, reason};
pub(crate) use registry::Stage;
use registry::{Entry, Key, entry, with};
pub(crate) fn blocks(shell: &ShellState) -> bool {
    Key::current(shell).is_some_and(|k| with(|r| r.entries.contains_key(&k)))
}
pub(super) fn blocked(shell: &ShellState) -> bool {
    shell
        .slack_lists
        .scope
        .as_ref()
        .is_some_and(rename::creation_cooldown)
}
fn available(shell: &ShellState, checking: bool) -> bool {
    let Some(scope) = shell.slack_lists.scope.as_ref() else {
        return false;
    };
    let Some(v) = shell.slack_lists.view.as_ref() else {
        return false;
    };
    scope.current(shell)
        && shell.overlay == Some(Overlay::SlackList)
        && !scope.deleted(shell, &v.id)
        && !v.loading
        && !v.unavailable
        && (checking || !v.stale)
        && v.editor.is_none()
        && v.retry_at <= now()
        && !blocked(shell)
        && !delete::blocks_mutation(shell)
        && !rename::blocks_mutation(shell)
        && !create::legacy_blocks_mutation(shell)
        && !shell.slack_lists.operations.contains_key(&v.id)
}
pub(crate) fn add(mut state: Signal<ShellState>, detail: u64) {
    let result = begin(&mut state.write(), detail);
    match result {
        Ok(g) => network::save(state, g),
        Err(message) => {
            if let Some(v) = state.write().slack_lists.view.as_mut() {
                v.error = Some(message.into());
            }
        }
    }
}
fn begin(shell: &mut ShellState, detail: u64) -> Result<Guard, &'static str> {
    if !available(shell, false) || blocks(shell) {
        return Err("Finish the current change or refresh this parent first.");
    }
    let v = shell
        .slack_lists
        .view
        .as_ref()
        .ok_or("Open the parent again.")?;
    let parent =
        record_detail::subitem_target(v, detail).ok_or("Open an active root item again.")?;
    let l = v.loaded.as_ref().ok_or("Wait for the parent to load.")?;
    let key = Key::current(shell).ok_or("Open the List again.")?;
    let bytes = subitem_input_bytes(&l.document, &l.records).map_err(|e| e.0)?;
    let charge = registry::charge(bytes).ok_or("The retained subitem limit was reached.")?;
    if !with(|r| r.room(&key, charge)) {
        return Err("Resolve a retained subitem change before adding another.");
    }
    if let Some(reason) = record_detail::subitem_reason(v) {
        return Err(reason);
    }
    let proof = Proof::capture_bounded(shell, &parent).ok_or("The parent changed.")?;
    let intent = SubitemCreationIntent::retain(proof.document.clone(), proof.page.clone(), parent);
    let timestamp = create::subitem_timestamp(shell)?;
    let serial = next_id();
    let job = next_id();
    let scope = shell.slack_lists.scope.clone().unwrap();
    with(|r| {
        r.entries.insert(
            key.clone(),
            Entry {
                serial,
                job,
                stage: Stage::Preparing,
                in_flight: true,
                intent: Arc::new(intent),
                timestamp,
                reply: None,
                read: None,
                child: None,
                failure: None,
                review: None,
                retirement: None,
                message: "Checking the parent before adding a subitem...",
                charge,
            },
        )
    });
    Ok(Guard {
        key,
        serial,
        job,
        scope,
        proof,
    })
}
fn prepare_permit(
    intent: Arc<SubitemCreationIntent>,
    loaded: &network::Loaded,
) -> Option<SubitemCreatePermit> {
    SubitemCreateAttempt::new((*intent).clone())
        .admit(
            &loaded.document,
            &loaded.parent,
            loaded.allow.as_deref(),
            Some(&loaded.personal),
            loaded.experiments.sub_tasks,
        )
        .ok()
}
fn admit_prepared(
    shell: &ShellState,
    g: &Guard,
    permit: SubitemCreatePermit,
) -> Option<SubitemCreatePermit> {
    if !g.current(shell, Stage::Preparing) || blocked(shell) {
        return None;
    }
    with(|r| {
        let e = r.entries.get_mut(&g.key).unwrap();
        e.stage = Stage::Sending;
        e.message = "Adding subitem...";
    });
    Some(permit)
}
#[cfg(test)]
fn admit(shell: &ShellState, g: &Guard, loaded: &network::Loaded) -> Option<SubitemCreatePermit> {
    let intent = with(|r| r.entries.get(&g.key).map(|e| e.intent.clone()))?;
    admit_prepared(shell, g, prepare_permit(intent, loaded)?)
}
fn unsent(shell: &mut ShellState, g: &Guard, message: &'static str) {
    with(|r| {
        if let Some(e) = r.entries.get_mut(&g.key).filter(|e| {
            e.serial == g.serial
                && e.job == g.job
                && matches!(e.stage, Stage::Preparing | Stage::NotRequested)
        }) {
            e.stage = Stage::NotRequested;
            e.in_flight = false;
            e.message = message;
        }
    });
    let _ = shell;
}
fn completed(shell: &mut ShellState, g: &Guard, result: Result<Value, ListSubitemFailure>) {
    if let Err(ListSubitemFailure::NotRequested(e) | ListSubitemFailure::Unconfirmed(e)) = &result {
        rename::record_creation_limit(&g.scope, e);
    }
    with(|r| {
        let Some(e) = r
            .entries
            .get_mut(&g.key)
            .filter(|e| e.serial == g.serial && e.job == g.job && e.stage == Stage::Sending)
        else {
            return;
        };
        e.in_flight = false;
        e.failure = result.as_ref().err().map(|error| match error {
            ListSubitemFailure::NotRequested(e) => {
                registry::Failure::from_error("not_requested", e)
            }
            ListSubitemFailure::Unconfirmed(e) => registry::Failure::from_error("unconfirmed", e),
        });
        match result {
            Err(ListSubitemFailure::NotRequested(_)) => {
                e.stage = Stage::NotRequested;
                e.message = "The subitem request was not sent.";
            }
            Err(ListSubitemFailure::Unconfirmed(_)) => {
                e.stage = Stage::Uncertain;
                e.message = "Could not confirm the subitem. Check current subitems; this request will not be sent again.";
            }
            Ok(raw) => {
                e.child = e.intent.validate_response(&raw).ok();
                e.reply = Some(Arc::new(raw));
                e.stage = if e.child.is_some() {
                    Stage::AwaitingRead
                } else {
                    Stage::Uncertain
                };
                e.message = if e.child.is_some() {
                    "Subitem added. Check current subitems before opening it."
                } else {
                    "The subitem identity was not confirmed. This request will not be sent again."
                };
            }
        }
    });
    let _ = shell;
}
pub(crate) fn check(mut state: Signal<ShellState>, serial: u64) {
    let attempt = checking(&mut state.write(), serial);
    if let Some(g) = attempt {
        network::check(state, g);
    }
}
fn checking(shell: &mut ShellState, serial: u64) -> Option<Guard> {
    if !available(shell, true) {
        return None;
    }
    let e = entry(shell)?;
    if e.serial != serial || e.stage.busy() || e.in_flight || e.stage == Stage::NotRequested {
        return None;
    }
    let proof = Proof::capture(shell, e.intent.parent_id())?;
    let key = Key::current(shell)?;
    let job = next_id();
    let scope = shell.slack_lists.scope.clone()?;
    with(|r| {
        let e = r.entries.get_mut(&key).unwrap();
        e.stage = Stage::Checking;
        e.in_flight = true;
        e.job = job;
        e.review = None;
        e.retirement = None;
        e.message = "Checking current subitems...";
    });
    Some(Guard {
        key,
        serial,
        job,
        scope,
        proof,
    })
}
fn checked(shell: &mut ShellState, g: &Guard, result: Result<network::Loaded, Error>) -> bool {
    if let Err(error) = &result {
        rename::record_creation_limit(&g.scope, error);
    }
    let failure = result
        .as_ref()
        .err()
        .map(|e| registry::Failure::from_error("read_failed", e));
    let current = g.current(shell, Stage::Checking);
    let Some(e) = with(|r| {
        r.entries
            .get(&g.key)
            .filter(|e| e.serial == g.serial && e.job == g.job && e.stage == Stage::Checking)
            .cloned()
    }) else {
        return false;
    };
    // A closed or replaced owner still gets its completion, never stale projection.
    let fallback = if e.child.is_some() {
        Stage::AwaitingRead
    } else {
        Stage::Uncertain
    };
    with(|r| {
        let e = r.entries.get_mut(&g.key).unwrap();
        e.stage = fallback;
        e.in_flight = false;
        e.failure = failure;
        e.message =
            "The check did not establish a current subitem. Review the parent and check again.";
    });
    let Ok(loaded) = result else {
        return false;
    };
    with(|r| r.entries.get_mut(&g.key).unwrap().read = Some(loaded.raw.clone()));
    if !current {
        return false;
    }
    let Ok((document, records)) = loaded.parent.merge(&g.proof.document, &g.proof.page) else {
        return false;
    };
    if !loaded.parent.metadata_matches(&loaded.document) {
        return false;
    }
    // The checked payload reservation includes this full positive merge.
    if subitem_input_bytes(&document, &records).is_err() {
        return false;
    }
    let found = e
        .child
        .as_ref()
        .and_then(|child| loaded.parent.confirmed(&e.intent, child))
        .is_some();
    let v = shell.slack_lists.view.as_mut().unwrap();
    let l = v.loaded.as_mut().unwrap();
    v.title = document.title.clone();
    l.document = document;
    l.records = records;
    l.allow_lists = loaded.allow;
    l.personal_task_lists = Some(loaded.personal);
    l.list_experiments = Some(loaded.experiments);
    v.stale = false;
    v.revision = next_id();
    rebuild_rows(shell);
    let review = Proof::capture(shell, e.intent.parent_id());
    with(|r| {
        let e = r.entries.get_mut(&g.key).unwrap();
        e.review = review;
        e.stage = if found {
            Stage::Available
        } else {
            Stage::Reviewed
        };
        e.message = if found {
            "The exact added subitem is available. Open it when ready."
        } else if e.child.is_some() {
            "The added ID is not in this response. Absence does not mean it was deleted."
        } else {
            "Current subitems are shown. No returned item identifies the earlier request; it may still arrive."
        };
    });
    true
}
pub(crate) fn dismiss(mut state: Signal<ShellState>, serial: u64) {
    dismiss_shell(&mut state.write(), serial);
}
fn dismiss_shell(shell: &mut ShellState, serial: u64) -> bool {
    let Some(key) = Key::current(shell) else {
        return false;
    };
    with(|r| {
        let Some(e) = r.entries.get_mut(&key).filter(|e| e.serial == serial) else {
            return false;
        };
        if e.stage == Stage::Preparing {
            // Revoke write authority immediately, but keep charged memory while
            // the original detached read/validation still holds its snapshot.
            e.stage = Stage::NotRequested;
            e.message = "Canceled. Waiting for the original check to finish; nothing will be sent.";
            return true;
        }
        if e.in_flight {
            return false;
        }
        if e.stage == Stage::NotRequested
            || (!e.stage.busy()
                && (e.review.as_ref().is_some_and(|p| p.current(shell))
                    || e.retirement.as_ref().is_some_and(|p| p.current(shell))))
        {
            r.entries.remove(&key);
            true
        } else {
            false
        }
    })
}

pub(crate) fn open(mut state: Signal<ShellState>, serial: u64) {
    open_shell(&mut state.write(), serial);
}
fn open_shell(shell: &mut ShellState, serial: u64) -> bool {
    let Some(e) = entry(shell).filter(|e| e.serial == serial && e.stage == Stage::Available) else {
        return false;
    };
    if !e.review.as_ref().is_some_and(|p| p.current(shell)) {
        return false;
    }
    let Some(child) = e.child.as_ref() else {
        return false;
    };
    let Some(raw) = e.read.as_ref() else {
        return false;
    };
    let Ok(read) = SubitemReadback::from_response(e.intent.list_id(), e.intent.parent_id(), raw)
    else {
        return false;
    };
    if read.confirmed(&e.intent, child).is_none() {
        return false;
    }
    let key = Key::current(shell).unwrap();
    if !record_detail::navigate_subitem(shell, e.intent.parent_id(), &child.id) {
        return false;
    }
    with(|r| {
        r.entries.remove(&key);
    });
    true
}
pub(super) fn poll(shell: &ShellState) -> bool {
    let Some(key) = Key::current(shell) else {
        return false;
    };
    with(|r| {
        let Some(e) = r.entries.get_mut(&key) else {
            return false;
        };
        if e.stage == Stage::Preparing
            && !shell
                .slack_lists
                .view
                .as_ref()
                .is_some_and(|v| record_detail::subitem_origin(v, e.intent.parent_id()).is_some())
        {
            e.stage = Stage::NotRequested;
            e.message = "The parent closed. Waiting for the original check to finish; nothing will be sent.";
            true
        } else {
            false
        }
    })
}

/// Explicit review of a retained delivery receipt, not a claim about the server.
/// Available even when the original parent is absent, archived or reparented.
/// It never sends, merges evidence, opens a guessed child or removes on a timer.
pub(crate) fn review_retained(mut state: Signal<ShellState>, serial: u64) {
    review_retained_shell(&mut state.write(), serial);
}
fn review_retained_shell(shell: &mut ShellState, serial: u64) -> bool {
    let Some(_) = entry(shell).filter(|e| e.serial == serial && !e.stage.busy() && !e.in_flight)
    else {
        return false;
    };
    let Some(proof) = owner::Retirement::capture(shell) else {
        return false;
    };
    let key = Key::current(shell).unwrap();
    with(|r| {
        let e = r.entries.get_mut(&key).unwrap();
        e.review = None;
        e.retirement = Some(proof);
        e.stage = Stage::Reviewed;
        e.message = "This retained request may still arrive. Closing this receipt does not undo it or prove that it was not added. Review Slack before adding another subitem.";
    });
    true
}
pub(crate) fn retained_review_visible(shell: &ShellState) -> bool {
    let Some(key) = Key::current(shell) else {
        return false;
    };
    with(|r| {
        r.entries.get(&key).is_some_and(|e| {
            !shell
                .slack_lists
                .view
                .as_ref()
                .is_some_and(|v| record_detail::subitem_origin(v, e.intent.parent_id()).is_some())
        })
    })
}

pub(crate) fn visible(shell: &ShellState) -> bool {
    blocks(shell)
        || shell
            .slack_lists
            .view
            .as_ref()
            .and_then(|v| v.loaded.as_ref())
            .is_some_and(|l| {
                l.list_experiments.is_some_and(|f| f.sub_tasks)
                    && l.personal_task_lists
                        .as_ref()
                        .is_some_and(|p| !p.contains(&l.document.id))
            })
}

pub(crate) fn selection_stamp(shell: &ShellState) -> Option<u64> {
    shell
        .slack_lists
        .view
        .as_ref()
        .map(|v| v.layout.selection_generation)
}
