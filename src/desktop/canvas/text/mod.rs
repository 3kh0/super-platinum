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
pub(crate) mod continuation_check;
mod continuation_fixture;
pub(super) mod fixture;
mod mapping;
pub(crate) mod replacement_check;
pub(super) mod replacement_fixture;
mod replacement_save;
#[cfg(test)]
mod replacement_tests;
mod save;
pub(crate) mod structure_check;
pub(super) mod structure_fixture;
pub(crate) mod structure_recovery_check;
mod structure_save;
#[cfg(test)]
mod structure_tests;
use checklist::structure_ops::Action;
#[cfg(test)]
mod tests;
mod view;
pub(super) use mapping::Mapping;
pub(super) use view::Editor;

enum Kind {
    Replacement,
    Text(Arc<TextIntent>),
    Structure(Action),
}
impl Kind {
    fn action(&self) -> Option<Action> {
        match self {
            Self::Structure(action) => Some(*action),
            _ => None,
        }
    }
    fn label(&self) -> &'static str {
        match self {
            Self::Text(_) => "Edit paragraph",
            Self::Replacement => "Delete paragraph and leave an empty paragraph",
            Self::Structure(Action::Before) => "Add paragraph before",
            Self::Structure(Action::After) => "Add paragraph after",
            Self::Structure(Action::Delete) => "Delete paragraph",
        }
    }
}

static NEXT_DRAFT: AtomicU64 = AtomicU64::new(1);
pub(super) struct Draft {
    pub(super) generation: u64,
    visit: u64,
    read: u64,
    section: String,
    kind: Kind,
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
    let Some(intent) = &item.intent else {
        return;
    };
    draft.set(Some(Draft {
        generation: NEXT_DRAFT.fetch_add(1, Ordering::Relaxed),
        visit: current.visit,
        read: current.read,
        section: section.into(),
        kind: Kind::Text(Arc::clone(intent)),
        blocks: item.blocks.clone(),
        reading: false,
        pending: None,
        error: None,
    }));
}
fn open_structure(
    state: Signal<ShellState>,
    reader: Signal<Reader>,
    mut draft: Signal<Option<Draft>>,
    section: &str,
    action: Action,
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
    let Some(actions) = &item.structure else {
        return;
    };
    if !match action {
        Action::Before => actions.insert_before,
        Action::After => actions.insert_after,
        Action::Delete => actions.delete || item.replacement,
    } {
        return;
    }
    let blocks = if action == Action::Delete {
        item.blocks.clone()
    } else {
        serde_json::to_string(
            &super_platinum_core::canvas::edit::text::TextDraft::default().rich_blocks(),
        )
        .unwrap()
    };
    draft.set(Some(Draft {
        generation: NEXT_DRAFT.fetch_add(1, Ordering::Relaxed),
        visit: current.visit,
        read: current.read,
        section: section.into(),
        kind: if action == Action::Delete && item.replacement {
            Kind::Replacement
        } else {
            Kind::Structure(action)
        },
        blocks,
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
    let before = section.clone();
    let after = section.clone();
    let remove = section.clone();
    rsx! {div {class:"canvas-editable-paragraph","data-canvas-text-section":"{section}",
        div {class:"canvas-paragraph-content",for node in &item.preview {{crate::view::rich::rich_node(node,epoch,state)}}}
        div {class:"canvas-paragraph-actions",
            if item.intent.is_some() {button {class:"canvas-text-open",r#type:"button",disabled,"aria-label":"Edit paragraph",onclick:move |_|{if rename.peek().is_none(){open(state,reader,draft,&target);}},"Edit"}}
            if let Some(actions) = &item.structure {
                if actions.insert_before {button {class:"canvas-text-open",r#type:"button",disabled,"data-structure-action":"before","aria-label":"Add paragraph before",onclick:move |_|{if rename.peek().is_none(){open_structure(state,reader,draft,&before,Action::Before);}},"Add before"}}
                if actions.insert_after {button {class:"canvas-text-open",r#type:"button",disabled,"data-structure-action":"after","aria-label":"Add paragraph after",onclick:move |_|{if rename.peek().is_none(){open_structure(state,reader,draft,&after,Action::After);}},"Add after"}}
                if actions.delete || item.replacement {button {class:"canvas-text-open",r#type:"button",disabled,"data-structure-action":"delete","aria-label":"Delete paragraph",onclick:move |_|{if rename.peek().is_none(){open_structure(state,reader,draft,&remove,Action::Delete);}},"Delete"}}
            }
        }
    }}
}
