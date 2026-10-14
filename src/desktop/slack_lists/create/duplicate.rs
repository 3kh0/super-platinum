//! Duplicate shares creation delivery receipts; the visible source grants only
//! unsent authority. The core intent pins canonical fields and adjacent position.
use super::*;

#[derive(Clone)]
pub(super) struct Origin {
    open: u64,
    generation: u64,
    record: String,
}
impl Origin {
    pub(super) fn current(&self, view: &View) -> bool {
        record_detail::delete_target(view, self.generation)
            .is_some_and(|(open, record, _)| open == self.open && record == self.record)
    }
    pub(super) fn still_open(&self, view: &View) -> bool {
        record_detail::matches_delete_origin(view, self.open, &self.record)
    }
}

pub(crate) fn reason(shell: &ShellState) -> Option<&'static str> {
    if let Some(reason) = unavailable_for(shell, true) {
        return Some(reason);
    }
    let view = shell.slack_lists.view.as_ref()?;
    let Some(detail) = record_detail::presentation(view) else {
        return Some("Open an item to duplicate it.");
    };
    let Some(_) = record_detail::delete_target(view, detail.generation) else {
        return Some("This item changed. Reopen its details.");
    };
    record_detail::duplicate_reason(view)
}

pub(crate) fn duplicate_item(mut state: Signal<ShellState>, generation: u64) {
    let result = begin(&mut state.write(), generation);
    match result {
        Ok(attempt) => network::dispatch(state, attempt),
        Err(error) => {
            if let Some(view) = state.write().slack_lists.view.as_mut().filter(|view| {
                record_detail::presentation(view)
                    .is_some_and(|detail| detail.generation == generation)
            }) {
                view.error = Some(error.into());
            }
        }
    }
}
pub(super) fn begin(shell: &mut ShellState, generation: u64) -> Result<Attempt, &'static str> {
    if let Some(reason) = unavailable_for(shell, true) {
        return Err(reason);
    }
    let view = shell
        .slack_lists
        .view
        .as_ref()
        .ok_or("Open this item again.")?;
    let (open, record, _) = record_detail::delete_target(view, generation)
        .ok_or("This item changed. Reopen its details before duplicating it.")?;
    let loaded = view.loaded.as_ref().unwrap();
    let intent = CreationIntent::capture_duplicate(
        &loaded.document,
        &loaded.records,
        &record,
        loaded.allow_lists.as_deref(),
        loaded.personal_task_lists.as_ref(),
        view.layout.selected.as_deref(),
    )
    .map_err(|error| error.0)?;
    reserve(
        shell,
        intent,
        Some(Origin {
            open,
            generation,
            record,
        }),
    )
}

pub(in crate::slack_lists) fn close_detail(shell: &mut ShellState) {
    let Some(view) = shell.slack_lists.view.as_ref() else {
        return;
    };
    shell.slack_lists.create.receipts.retain(|_, receipt| {
        !(receipt.stage == Stage::Preparing
            && receipt.attempt.visit == view.generation
            && receipt.attempt.origin.is_some())
    });
}

#[cfg(test)]
mod tests;
