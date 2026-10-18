//! Board choice draft and action custody in the existing schema-operation registry.
use super::*;
use super_platinum_core::slack_lists::{
    BoardChoiceAction, BoardChoiceChange, BoardLaneOrderDraft, SelectCreateOption,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Add,
    Rename,
    Color,
}
#[derive(Clone, Debug)]
struct ContextIdentity(std::sync::Arc<()>);
impl PartialEq for ContextIdentity {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for ContextIdentity {}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Choice {
    pub view: String,
    column: String,
    pub(super) refinements: super_platinum_core::slack_lists::ViewRefinementDraft,
    local_state: super::super::refinement::State,
    pub value: Option<String>,
    pub kind: Kind,
    identity: ContextIdentity,
    epoch: u64,
    visit: u64,
    selection: u64,
    layout: u64,
    grouping: u64,
    find: u64,
    query: String,
    draft: Option<BoardLaneOrderDraft>,
}
impl Choice {
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        shell.core.account_epoch == self.epoch
            && shell.slack_lists.scope.as_ref().is_some_and(|s| {
                s.current(shell) && std::sync::Arc::ptr_eq(&s.identity, &self.identity.0)
            })
            && shell.slack_lists.view.as_ref().is_some_and(|v| {
                v.layout.refinements == self.local_state
                    && v.generation == self.visit
                    && v.layout.selected.as_deref() == Some(&self.view)
                    && v.layout.selection_generation == self.selection
                    && v.layout.generation == self.layout
                    && v.layout.grouping_revision == self.grouping
                    && v.layout.find.revision == self.find
                    && v.layout.find.input == self.query
                    && v.layout.find.applied == self.query
                    && v.layout.board_order == self.draft
            })
    }
    pub(super) fn accepts(&self, action: &BoardChoiceAction) -> bool {
        match (self.kind, action) {
            (Kind::Add, BoardChoiceAction::Add { .. }) => true,
            (Kind::Rename, BoardChoiceAction::Rename { value, .. })
            | (Kind::Color, BoardChoiceAction::Color { value, .. }) => {
                self.value.as_deref() == Some(value)
            }
            _ => false,
        }
    }
}
fn choices<'a>(doc: &'a Document, choice: &Choice) -> Option<&'a Vec<Value>> {
    doc.column(&choice.column)
        .filter(|c| c.kind == "select")?
        .raw
        .pointer("/options/choices")?
        .as_array()
}
pub(super) fn name<'a>(doc: &'a Document, choice: &Choice) -> Option<&'a str> {
    if choice.kind == Kind::Add {
        return Some("");
    }
    choices(doc, choice)?
        .iter()
        .find(|c| c["value"].as_str() == choice.value.as_deref())?["label"]
        .as_str()
}
pub(super) fn supports(doc: &Document, choice: &Choice) -> bool {
    name(doc, choice).is_some()
        && choices(doc, choice).is_some_and(|values| {
            choice.kind == Kind::Add
                || values
                    .iter()
                    .any(|c| c["value"].as_str() == choice.value.as_deref())
        })
}
pub(super) fn initial(doc: &Document, choice: &Choice) -> Option<BoardChoiceAction> {
    if choice.kind == Kind::Add {
        let generated = SelectCreateOption::generate("Board option").ok()?;
        return Some(BoardChoiceAction::Add {
            value: generated.value().into(),
            label: String::new(),
            color: generated.color().into(),
        });
    }
    let original = choices(doc, choice)?
        .iter()
        .find(|c| c["value"].as_str() == choice.value.as_deref())?;
    let value = choice.value.clone()?;
    Some(match choice.kind {
        Kind::Rename => BoardChoiceAction::Rename {
            value,
            label: original["label"].as_str()?.into(),
        },
        Kind::Color => BoardChoiceAction::Color {
            value,
            color: original["color"].as_str().unwrap_or("gray").into(),
        },
        Kind::Add => unreachable!(),
    })
}
pub(crate) fn control(shell: &ShellState, value: Option<&str>, kind: Kind) -> Option<Choice> {
    let v = shell.slack_lists.view.as_ref()?;
    if v.layout.find.input != v.layout.find.applied {
        return None;
    }
    let loaded = v.loaded.as_ref()?;
    let id = v.layout.selected.as_deref()?;
    let refinements = v.layout.refinements.draft(&loaded.document, id).ok()?;
    let read = super_platinum_core::slack_lists::BoardLaneOrder::refined_projection_document(
        &loaded.document,
        &refinements,
        v.layout.board_order.as_ref(),
    )
    .ok()?;
    let raw = read.views.iter().find(|v| v["id"] == id)?;
    let key = raw.pointer("/grouping/group_by")?.as_str()?;
    let column = loaded
        .document
        .columns
        .iter()
        .find(|c| c.key == key && c.kind == "select")?
        .id
        .clone();
    let choice = Choice {
        column,
        refinements,
        local_state: v.layout.refinements.clone(),
        view: v.layout.selected.clone()?,
        value: value.map(str::to_owned),
        kind,
        identity: ContextIdentity(shell.slack_lists.scope.as_ref()?.identity.clone()),
        epoch: shell.core.account_epoch,
        visit: v.generation,
        selection: v.layout.selection_generation,
        layout: v.layout.generation,
        grouping: v.layout.grouping_revision,
        find: v.layout.find.revision,
        query: v.layout.find.applied.clone(),
        draft: v.layout.board_order.clone(),
    };
    supports(&v.loaded.as_ref()?.document, &choice).then_some(choice)
}
pub(crate) fn available(shell: &ShellState, value: Option<&str>, kind: Kind) -> Option<Choice> {
    let choice = control(shell, value, kind)?;
    let loaded = shell.slack_lists.view.as_ref()?.loaded.as_ref()?;
    if !BoardChoiceChange::supports_refined(
        &loaded.document,
        &choice.refinements,
        loaded.allow_lists.as_deref(),
        loaded.personal_task_lists.as_ref(),
    ) {
        return None;
    }
    if kind == Kind::Add && choices(&loaded.document, &choice)?.len() >= 1024 {
        return None;
    }

    can_target(shell, &Target::BoardChoice(choice.clone())).then_some(choice)
}
pub(crate) fn open(mut state: Signal<ShellState>, choice: Choice) {
    open_shell(&mut state.write(), choice);
}
pub(crate) fn open_shell(shell: &mut ShellState, choice: Choice) -> bool {
    choice.current(shell)
        && available(shell, choice.value.as_deref(), choice.kind)
            .is_some_and(|fresh| fresh == choice)
        && open_target(shell, Target::BoardChoice(choice.clone()), choice.visit)
}
pub(super) fn ready(shell: &ShellState, e: &registry::Entry) -> bool {
    let Target::BoardChoice(choice) = &e.target else {
        return false;
    };
    let Input::BoardChoice(action) = &e.input else {
        return false;
    };
    let Some(loaded) = shell
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
    else {
        return false;
    };
    choice.current(shell)
        && choice.accepts(action)
        && BoardChoiceChange::capture_refined(
            &e.original,
            &choice.refinements,
            action.clone(),
            loaded.allow_lists.as_deref(),
            loaded.personal_task_lists.as_ref(),
        )
        .is_ok()
}
pub(crate) fn input(
    mut state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    rendered: BoardChoiceAction,
    label: Option<String>,
    color: Option<String>,
) {
    let changed = set_input(&state.peek(), serial, visit, rendered, label, color);
    if changed {
        state.write();
    }
}
fn set_input(
    shell: &ShellState,
    serial: u64,
    visit: u64,
    rendered: BoardChoiceAction,
    label: Option<String>,
    color: Option<String>,
) -> bool {
    if generation(shell) != Some(visit) || !owner::available(shell) {
        return false;
    }
    let Some(key) = Key::current(shell) else {
        return false;
    };
    if registry::blocked(&key.owner) {
        return false;
    }
    with_registry(|r| {
        let Some(e) = r
            .entries
            .get_mut(&key)
            .filter(|e| e.serial == serial && e.stage == Stage::Editing)
        else {
            return false;
        };
        let Target::BoardChoice(choice) = &e.target else {
            return false;
        };
        if !choice.current(shell)
            || !shell
                .slack_lists
                .view
                .as_ref()
                .and_then(|v| v.loaded.as_ref())
                .is_some_and(|l| l.document == e.original)
        {
            return false;
        }
        let Input::BoardChoice(current) = &e.input else {
            return false;
        };
        let same = match (current, &rendered) {
            (BoardChoiceAction::Add { value: a, .. }, BoardChoiceAction::Add { value: b, .. })
            | (
                BoardChoiceAction::Rename { value: a, .. },
                BoardChoiceAction::Rename { value: b, .. },
            )
            | (
                BoardChoiceAction::Color { value: a, .. },
                BoardChoiceAction::Color { value: b, .. },
            ) => a == b,
            _ => false,
        };
        if !same {
            return false;
        }
        let mut action = current.clone();
        match &mut action {
            BoardChoiceAction::Add {
                label: current_label,
                color: current_color,
                ..
            } => {
                if let Some(label) = label {
                    *current_label = label;
                }
                if let Some(color) = color {
                    *current_color = color;
                }
            }
            BoardChoiceAction::Rename { label: current, .. } => {
                if color.is_some() {
                    return false;
                }
                if let Some(label) = label {
                    *current = label;
                }
            }
            BoardChoiceAction::Color { color: current, .. } => {
                if label.is_some() {
                    return false;
                }
                if let Some(color) = color {
                    *current = color;
                }
            }
        }
        e.input = Input::BoardChoice(action);
        true
    })
}
fn prepare_choice(
    shell: &mut ShellState,
    serial: u64,
    visit: u64,
    rendered: &BoardChoiceAction,
) -> Option<(Guard, Intent)> {
    if generation(shell) != Some(visit)
        || !entry(shell).is_some_and(|e| {
            e.serial == serial
                && e.stage == Stage::Editing
                && e.input == Input::BoardChoice(rendered.clone())
                && ready(shell, &e)
        })
    {
        return None;
    }
    prepare(shell, serial)
}
pub(crate) fn confirm(
    mut state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    rendered: BoardChoiceAction,
) {
    let prepared = prepare_choice(&mut state.write(), serial, visit, &rendered);
    if let Some((guard, intent)) = prepared {
        network::save(state, guard, intent);
    }
}
#[derive(Clone)]
pub(crate) struct Presentation {
    pub kind: Kind,
    pub action: BoardChoiceAction,
    pub ready: bool,
    pub current: Option<(String, String)>,
}
pub(super) fn presentation(shell: &ShellState, e: &registry::Entry) -> Option<Presentation> {
    let Target::BoardChoice(choice) = &e.target else {
        return None;
    };
    let Input::BoardChoice(action) = &e.input else {
        return None;
    };
    let value = match action {
        BoardChoiceAction::Add { value, .. }
        | BoardChoiceAction::Rename { value, .. }
        | BoardChoiceAction::Color { value, .. } => value,
    };
    let current = shell
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
        .and_then(|l| choices(&l.document, choice))
        .and_then(|values| values.iter().find(|c| c["value"] == value.as_str()))
        .and_then(|c| {
            Some((
                c["label"].as_str()?.to_owned(),
                c["color"].as_str().unwrap_or("Unspecified").to_owned(),
            ))
        });
    Some(Presentation {
        current,
        kind: choice.kind,
        action: action.clone(),
        ready: ready(shell, e),
    })
}

#[cfg(test)]
mod tests;
