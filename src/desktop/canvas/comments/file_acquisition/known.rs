//! One attempt per owned invalidation; ordinary renders never refresh known rows.
use super::*;

fn pending_known(shell: &ShellState, model: &super::super::State) -> Vec<Cold> {
    let mut work: Vec<Cold> = Vec::new();
    for row in model.rows.iter().take(MAX_ROWS) {
        let Some(cold) = file_source::known(shell, model, &row.message) else {
            continue;
        };
        if !work.iter().any(|old| old.id() == cold.id())
            && !model
                .file_acquisition
                .attempts
                .iter()
                .any(|old| old.cold.id() == cold.id() && old.cold.demanded(shell, model))
        {
            work.push(cold);
        }
    }
    work
}

/// Called from the mounted comments effect, not from projection/render. The
/// tracked reads observe events and accepted page changes; writes occur only
/// when a new original event/preimage has work, so failures never auto-loop.
pub(in crate::canvas::comments) fn observe(
    state: Signal<ShellState>,
    mut model: Signal<super::super::State>,
) {
    let work = {
        let shell = state.read();
        let value = model.read();
        if !enabled(&shell) || value.loading || value.error.is_some() {
            return;
        }
        pending_known(&shell, &value)
    };
    if work.is_empty() {
        return;
    }
    let wake = {
        let shell = state.peek();
        let mut value = model.write();
        enqueue(&shell, &mut value, work);
        value
            .file_acquisition
            .limit
            .as_ref()
            .and_then(|l| l.until.map(|at| (l.scope.clone(), at)))
    };
    if let Some((scope, at)) = wake {
        run::arm(state, model, scope, at);
    }
    start(state, model);
}

fn enqueue(shell: &ShellState, model: &mut super::super::State, work: Vec<Cold>) {
    sync_limit(shell, model);
    let limited = cooling(shell, model);
    for cold in work {
        if !cold.demanded(shell, model) {
            continue;
        }
        // Replacing only this ID retires its exact carrier through slot_current.
        // The serial worker then admits its new slot; other IDs remain bounded.
        model
            .file_acquisition
            .attempts
            .retain(|old| old.cold.id() != cold.id());
        model.file_acquisition.attempts.push(Attempt {
            token: Arc::new(()),
            cold,
            phase: if limited {
                Phase::Unavailable
            } else {
                Phase::Queued
            },
            notice: limited.then_some(LIMITED),
            receipt: None,
        });
    }
}

#[cfg(test)]
mod tests;
