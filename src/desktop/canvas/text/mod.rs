//! Existing canonical paragraph drafts. HTML export supplies only an exact anchor.
use super::{Reader, checklist};
use crate::state::ShellState;
use dioxus::prelude::*;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use super_platinum_core::canvas::edit::text::TextIntent;

pub(crate) mod check;
pub(super) mod fixture;
mod mapping;
mod save;
#[cfg(test)]
mod tests;
mod view;
pub(super) use mapping::Mapping;
pub(super) use view::Editor;

static NEXT_DRAFT: AtomicU64 = AtomicU64::new(1);
pub(super) struct Draft {
    pub(super) generation: u64,
    visit: u64,
    read: u64,
    section: String,
    intent: Arc<TextIntent>,
    blocks: String,
    reading: bool,
    pub(super) pending: Option<checklist::Attempt>,
    error: Option<&'static str>,
}

pub(super) fn open(
    state: Signal<ShellState>,
    reader: Signal<Reader>,
    mut draft: Signal<Option<Draft>>,
    section: &str,
) {
    if draft.peek().is_some() {
        return;
    }
    let shell = state.peek();
    let Some(current) = checklist::presentation(&shell) else {
        return;
    };
    let r = reader.peek();
    if r.loading
        || current.loading
        || current.stale
        || current.operation.is_some()
        || current.capacity_reason.is_some()
        || r.text.owner != Some((current.visit, current.read))
    {
        return;
    }
    let Some(item) = r.text.items.get(section) else {
        return;
    };
    draft.set(Some(Draft {
        generation: NEXT_DRAFT.fetch_add(1, Ordering::Relaxed),
        visit: current.visit,
        read: current.read,
        section: section.into(),
        intent: Arc::clone(&item.intent),
        blocks: item.blocks.clone(),
        reading: false,
        pending: None,
        error: None,
    }));
}
fn current(shell: &ShellState, draft: &Draft) -> bool {
    checklist::presentation(shell).is_some_and(|view| {
        view.visit == draft.visit
            && view.read == draft.read
            && !view.loading
            && !view.stale
            && view.operation.is_none()
            && view.capacity_reason.is_none()
    })
}
fn cancel(mut state: Signal<ShellState>, mut draft: Signal<Option<Draft>>, generation: u64) {
    let attempt = {
        let d = draft.peek();
        let Some(d) = d.as_ref().filter(|d| d.generation == generation) else {
            return;
        };
        d.pending.clone()
    };
    if let Some(attempt) = attempt {
        if !checklist::cancel_preflight(&mut state.write(), &attempt) {
            return;
        }
    }
    draft.set(None);
}
/// Canonical receipt settlement remains proof even if a later event invalidates
/// the read cache before this component's effect runs. Absence is not proof.
pub(super) fn should_close(shell: &ShellState, draft: Option<&Draft>) -> bool {
    let Some(d) = draft.filter(|d| d.pending.is_some()) else {
        return false;
    };
    checklist::text_ops::settled(shell, d.pending.as_ref().unwrap())
}

pub(super) fn paragraph(
    state: Signal<ShellState>,
    reader: Signal<Reader>,
    draft: Signal<Option<Draft>>,
    section: Option<&str>,
    epoch: u64,
    rename: Signal<Option<super::RenameDraft>>,
    fallback: Element,
) -> Element {
    let r = reader.read();
    let current = checklist::presentation(&state.read());
    let item = section.and_then(|id| r.text.items.get(id));
    let Some(item) = item else {
        return rsx! {p {{fallback}}};
    };
    let section = section.unwrap().to_owned();
    let edit = draft.read();
    let disabled = rename.peek().is_some()
        || edit.is_some()
        || r.loading
        || current.as_ref().is_none_or(|view| {
            view.loading
                || view.stale
                || view.operation.is_some()
                || view.capacity_reason.is_some()
                || r.text.owner != Some((view.visit, view.read))
        });
    let target = section.clone();
    rsx! {div {class:"canvas-editable-paragraph","data-canvas-text-section":"{section}",
        div {class:"canvas-paragraph-content",for node in &item.preview {{crate::view::rich::rich_node(node,epoch,state)}}}
        button {class:"canvas-text-open",r#type:"button",disabled,"aria-label":"Edit paragraph",onclick:move |_|{if rename.peek().is_none(){open(state,reader,draft,&target);}},"Edit"}
    }}
}
