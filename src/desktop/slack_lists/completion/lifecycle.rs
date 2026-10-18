use super::*;

pub(super) fn failed(shell: &mut ShellState, task: &Task, message: &str, error: Option<&Error>) {
    if !task.owned(shell) {
        return;
    }
    let entry = shell
        .slack_lists
        .completion
        .entries
        .get_mut(&task.serial)
        .unwrap();
    entry.retry_at = entry.retry_at.max(error.map(retry_at).unwrap_or(0));
    entry.running = None;
    if matches!(entry.phase, Phase::Preparing | Phase::ReferenceReady) {
        entry.phase = Phase::Stopped;
    }
    entry.message = message.into();
}
pub(super) fn prepare_primary(
    shell: &mut ShellState,
    task: &Task,
    primary: &Loaded,
    reference: Option<&Loaded>,
) -> Result<(), &'static str> {
    if task.authority.is_none() || !task.current(shell) {
        return Err("This To-do visit changed. No new write was sent.");
    }
    let entry = shell
        .slack_lists
        .completion
        .entries
        .get_mut(&task.serial)
        .unwrap();
    if entry.phase != Phase::Preparing {
        return Err("This completion step is no longer current.");
    }
    let personal = primary
        .personal_task_lists
        .as_ref()
        .ok_or("Refresh your To-do access first.")?;
    entry
        .intent
        .primary()
        .revalidate(
            &primary.document,
            &primary.records,
            primary.allow_lists.as_deref(),
            personal,
        )
        .map_err(|e| e.0)?;
    let mirror = if entry.intent.reference().is_some() {
        let target = reference.ok_or("The referenced item could not be checked.")?;
        let prefs = target
            .personal_task_lists
            .as_ref()
            .ok_or("Referenced To-do access was not verified.")?;
        Some(
            entry
                .intent
                .capture_reference(
                    &target.document,
                    &target.records,
                    target.allow_lists.as_deref(),
                    prefs,
                )
                .map_err(|e| e.0)?,
        )
    } else {
        None
    };
    entry.reference = mirror;
    entry.phase = Phase::PrimarySending;
    entry.message = "Updating your To-do…".into();
    Ok(())
}
pub(super) fn prepare_reference(
    shell: &mut ShellState,
    task: &Task,
    primary: &Loaded,
    reference: &Loaded,
) -> Result<TaskCompletionCell, &'static str> {
    if task.authority.is_none() || !task.current(shell) {
        return Err("This To-do visit changed. Its referenced item was not sent.");
    }
    let entry = shell
        .slack_lists
        .completion
        .entries
        .get_mut(&task.serial)
        .unwrap();
    if entry.phase != Phase::ReferenceReady || !entry.primary_observed {
        return Err("Observe the personal To-do before continuing.");
    }
    if !entry
        .intent
        .reference_still_current(&primary.document, &primary.records)
    {
        return Err(
            "The personal To-do or its reference changed. The referenced item was not sent.",
        );
    }
    let mirror = entry
        .reference
        .as_ref()
        .ok_or("The referenced completion intent is unavailable.")?;
    let prefs = reference
        .personal_task_lists
        .as_ref()
        .ok_or("Referenced access was not verified.")?;
    mirror
        .revalidate(
            &reference.document,
            &reference.records,
            reference.allow_lists.as_deref(),
            prefs,
        )
        .map_err(|_| "The referenced item or its access changed. No referenced write was sent.")?;
    let mirror = mirror.clone();
    entry.phase = Phase::ReferenceSending;
    entry.message = "Your To-do is updated. Updating its referenced item…".into();
    Ok(mirror)
}
pub(super) fn settled(
    shell: &mut ShellState,
    task: &Task,
    reference: bool,
    result: Result<Value, Error>,
) -> bool {
    if !task.owned(shell) {
        return false;
    }
    let entry = shell
        .slack_lists
        .completion
        .entries
        .get_mut(&task.serial)
        .unwrap();
    let expected = if reference {
        Phase::ReferenceSending
    } else {
        Phase::PrimarySending
    };
    if entry.phase != expected {
        return false;
    }
    let cell = if reference {
        entry.reference.as_ref().unwrap()
    } else {
        entry.intent.primary()
    };
    let acknowledged = result
        .as_ref()
        .is_ok_and(|v| cell.edit().validate_response(v).is_ok());
    entry.retry_at = entry
        .retry_at
        .max(result.as_ref().err().map(retry_at).unwrap_or(0));
    // The existing cell executor is one-attempt. Every result after invocation
    // remains readback-only here, including Offline after a socket was started.
    entry.phase = if reference {
        Phase::ReferenceUnknown
    } else {
        Phase::PrimaryUnknown
    };
    entry.running = None;
    entry.message = if reference {
        "Your To-do was observed updated. Check the referenced item's result; it will not be resent."
    } else {
        "Check this To-do's result before continuing. The completion will not be resent."
    }.into();
    acknowledged
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Observed {
    Retained,
    ReferenceReady,
    Complete,
}
pub(super) fn observed(
    shell: &mut ShellState,
    task: &Task,
    primary: &Loaded,
    reference: Option<&Loaded>,
) -> Observed {
    if !task.current(shell) {
        failed(
            shell,
            task,
            "This check belongs to an older visit. Reopen the List to check again.",
            None,
        );
        return Observed::Retained;
    }
    let entry = shell
        .slack_lists
        .completion
        .entries
        .get_mut(&task.serial)
        .unwrap();
    let phase = entry.phase;
    if !matches!(phase, Phase::PrimaryUnknown | Phase::ReferenceUnknown) {
        return Observed::Retained;
    }
    entry.running = None;
    if phase == Phase::ReferenceUnknown {
        let Some(mirror) = entry.reference.as_ref() else {
            return Observed::Retained;
        };
        let Some(target) = reference else {
            return Observed::Retained;
        };
        if mirror
            .readback_value(&target.document, &target.records)
            .is_none()
        {
            entry.message = "Your To-do was observed updated. The referenced result remains unconfirmed; check again without resending.".into();
            return Observed::Retained;
        }
        if mirror.readback_matches(&target.document, &target.records)
            && entry
                .intent
                .primary()
                .readback_matches(&primary.document, &primary.records)
        {
            return complete(shell, task.serial);
        }
        entry.phase = Phase::Stopped;
        entry.message = "Both completion requests have been checked. Slack currently shows a different or unavailable item; review this partial result.".into();
        return Observed::Retained;
    }
    let primary_value = entry
        .intent
        .primary()
        .readback_value(&primary.document, &primary.records);
    if !entry
        .intent
        .primary()
        .readback_matches(&primary.document, &primary.records)
    {
        if primary_value.is_some() {
            entry.phase = Phase::Stopped;
            entry.message = "Slack currently shows a different personal completion value. No further change was sent.".into();
        } else {
            entry.message =
                "The personal completion is still unconfirmed. Check again; it will not be resent."
                    .into();
        }
        return Observed::Retained;
    }
    entry.primary_observed = true;
    let Some(mirror) = entry.reference.as_ref() else {
        return complete(shell, task.serial);
    };
    if !entry
        .intent
        .reference_still_current(&primary.document, &primary.records)
    {
        // A sent mirror remains uncertain even when the primary reference later
        // changes. Never discard that receipt or infer the new target.
        if phase == Phase::PrimaryUnknown {
            entry.phase = Phase::Stopped;
        }
        entry.message = "Your To-do was updated, but its reference changed. Review this partial result; no new target will be chosen.".into();
        return Observed::Retained;
    }
    if phase == Phase::PrimaryUnknown {
        if !mirror.needs_write() {
            if reference
                .is_some_and(|target| mirror.readback_matches(&target.document, &target.records))
            {
                return complete(shell, task.serial);
            }
            entry.phase = Phase::Stopped;
            entry.message = "Your To-do is updated, but the previously completed reference changed. No new referenced write was sent.".into();
            return Observed::Retained;
        }
        entry.phase = Phase::ReferenceReady;
        entry.message =
            "Your To-do is updated. Continue to update the originally referenced item.".into();
        return Observed::ReferenceReady;
    }
    Observed::Retained
}
fn complete(shell: &mut ShellState, serial: u64) -> Observed {
    shell.slack_lists.completion.entries.remove(&serial);
    if let Some(view) = shell.slack_lists.view.as_mut() {
        view.stale = true;
        view.error = Some("To-do completion observed. Refreshing the List…".into());
    }
    Observed::Complete
}
