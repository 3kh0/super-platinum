//! One menu opening authorizes at most one choice from the unchanged source.
use super::{ButtonState, Pending, Source, inputs};
use crate::state::{Overlay, ShellState};
use dioxus::prelude::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use super_platinum_core::block_actions::{BlockElementMetadata, form::OptionSpec};

#[derive(Clone)]
pub(crate) struct Ticket {
    source: Source,
    metadata: BlockElementMetadata,
    input_revision: Option<u64>,
    stateless: bool,
    consumed: Arc<AtomicBool>,
    pub(crate) options: Vec<OptionSpec>,
}

pub(crate) fn id() -> u64 {
    super::next_id()
}

/// Called once per mounted source, and again when opening its menu. Empty
/// message models have no draft to retain in the shared container cache.
pub(crate) fn stateless(shell: &ShellState, source: &Source) -> bool {
    source
        .message(shell)
        .ok()
        .and_then(|message| {
            super_platinum_core::block_actions::message_state::MessageInputs::parse(message).ok()
        })
        .is_some_and(|model| model.fields.is_empty())
}

pub(crate) fn presentation(
    shell: &ShellState,
    source: &Source,
    metadata: &BlockElementMetadata,
    stateless: bool,
) -> ButtonState {
    let valid = metadata
        .block_id
        .as_deref()
        .zip(metadata.action_id.as_deref())
        .is_some_and(|(block, action)| {
            source.overflow_metadata(shell, block, action).as_ref() == Ok(metadata)
                && (stateless || inputs::supported(shell, source))
        });
    if !valid {
        return ButtonState::unavailable(if super::hydration::pending(shell, source) {
            "Loading app information…"
        } else {
            "This app action is unavailable here"
        });
    }
    super::operation_status(shell, source, metadata)
}

pub(crate) fn open(
    state: Signal<ShellState>,
    source: Source,
    metadata: BlockElementMetadata,
) -> Option<Ticket> {
    inputs::ensure(state, source.clone());
    let shell = state.peek();
    let stateless = stateless(&shell, &source);
    if shell.overlay.is_some() || !presentation(&shell, &source, &metadata, stateless).enabled {
        return None;
    }
    let options = source
        .overflow_options(
            &shell,
            metadata.block_id.as_deref()?,
            metadata.action_id.as_deref()?,
        )
        .ok()?;
    Some(Ticket {
        input_revision: inputs::revision(&shell, &source),
        stateless,
        source,
        metadata,
        options,
        consumed: Arc::new(AtomicBool::new(false)),
    })
}

impl Ticket {
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        !self.consumed.load(Ordering::SeqCst)
            && shell.overlay.is_none()
            && inputs::revision(shell, &self.source) == self.input_revision
            && presentation(shell, &self.source, &self.metadata, self.stateless).enabled
    }
    pub(crate) fn cancel(&self) {
        self.consumed.store(true, Ordering::SeqCst);
    }
    fn capture(&self, shell: &ShellState, selected: &str) -> Option<super::Captured> {
        if !self.current(shell)
            || !self.options.iter().any(|option| option.value == selected)
            || self.consumed.swap(true, Ordering::SeqCst)
        {
            return None;
        }
        let captured = self
            .source
            .capture_overflow(
                shell,
                self.metadata.block_id.as_deref()?,
                self.metadata.action_id.as_deref()?,
                selected,
            )
            .ok()?;
        (captured.metadata() == &self.metadata).then_some(captured)
    }
}

pub(crate) fn choose(mut state: Signal<ShellState>, ticket: Ticket, selected: String) {
    let captured = {
        let mut shell = state.write();
        let Some(captured) = ticket.capture(&shell, &selected) else {
            return;
        };
        if captured.confirmation().is_some() {
            shell.message_buttons.confirmation = Some(Pending {
                captured,
                input_revision: ticket.input_revision,
                revision: super::next_id(),
            });
            shell.overlay = Some(Overlay::ButtonConfirmation);
            return;
        }
        captured
    };
    super::dispatch(state, captured);
}

#[cfg(test)]
mod tests;
