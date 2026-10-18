//! Existing owned schema editor reused solely for local read refinements.
//! A local entry can never reach shared prepare/admission/dispatch.
use super::super::refinement::{self, controls::Choice};
use super::*;
mod adapter;
#[derive(Clone, Copy)]
pub(crate) enum Kind {
    Filter,
    Sort,
    Visibility,
    Grouping,
}
fn target(doc: &Document, id: &str, kind: Kind) -> Option<Target> {
    Some(match kind {
        Kind::Filter => Target::Filter(id.into()),
        Kind::Sort => Target::Sort(id.into()),
        Kind::Grouping => Target::Grouping(id.into()),
        Kind::Visibility => Target::Visibility {
            view: id.into(),
            column: doc
                .columns
                .iter()
                .find(|c| {
                    super_platinum_core::slack_lists::ViewFieldVisibility::current(doc, id, &c.id)
                        .is_some()
                })?
                .id
                .clone(),
        },
    })
}
/// Typed editor adapter only; never passed to a shared capture or write builder.
/// Canonical metadata and all unknown values stay in the separately captured Action.
fn editor_document(shell: &ShellState, kind: Kind) -> Option<Document> {
    let v = shell.slack_lists.view.as_ref()?;
    let id = v.layout.selected.as_deref()?;
    let mut raw = refinement::document(v).ok()?.raw.clone();
    let view = raw["list_metadata"]["views"]
        .as_array_mut()?
        .iter_mut()
        .find(|v| v["id"] == id)?;
    view["type"] = json!("table");
    for key in ["default_view_key", "is_all_items_view", "is_locked"] {
        view.as_object_mut()?.remove(key);
    }
    if matches!(kind, Kind::Grouping) {
        let grouping = view.get("grouping").filter(|v| !v.is_null());
        let key = match grouping {
            None => "",
            Some(v) => v
                .as_object()?
                .get("group_by")
                .map(Value::as_str)
                .unwrap_or(Some(""))?,
        };
        view["grouping"] = json!({"group_by":key});
    }
    adapter::rules(&v.loaded.as_ref()?.document, view, kind)?;
    Document::parse(raw).ok()
}
pub(crate) fn can_open(shell: &ShellState, kind: Kind) -> bool {
    if !owner::available(shell) || blocks_mutation(shell) {
        return false;
    }
    let Some(doc) = editor_document(shell, kind) else {
        return false;
    };
    let Some(id) = selected_view(shell) else {
        return false;
    };
    target(&doc, id, kind).is_some_and(|target| target.supports(&doc))
        && Key::current(shell).is_some_and(|key| !registry::blocked(&key.owner))
        && with_registry(|r| r.entries.len() < registry::MAX_ENTRIES)
}
pub(crate) fn open_stamped(
    mut state: Signal<ShellState>,
    kind: Kind,
    stamp: Option<refinement::toolbar::Stamp>,
) {
    if let Some(stamp) = stamp {
        open_stamped_shell(&mut state.write(), kind, &stamp);
    }
}
pub(super) fn open_stamped_shell(
    shell: &mut ShellState,
    kind: Kind,
    stamp: &refinement::toolbar::Stamp,
) -> bool {
    if !stamp.current(shell) {
        return false;
    }
    let Some(selected) = selection(shell) else {
        return false;
    };
    open_shell(shell, kind, stamp.visit, selected)
}
pub(crate) fn open(mut state: Signal<ShellState>, kind: Kind, visit: u64, selected: u64) {
    open_shell(&mut state.write(), kind, visit, selected);
}
pub(crate) fn open_shell(shell: &mut ShellState, kind: Kind, visit: u64, selected: u64) -> bool {
    if generation(shell) != Some(visit)
        || selection(shell) != Some(selected)
        || !can_open(shell, kind)
    {
        return false;
    }
    let Some(action) = refinement::capture(shell) else {
        return false;
    };
    let Some(doc) = editor_document(shell, kind) else {
        return false;
    };
    let Some(target) = target(&doc, selected_view(shell).unwrap(), kind) else {
        return false;
    };
    open_target_inner(shell, target, visit, Some((action, doc)))
}
pub(super) fn original_current(shell: &ShellState, e: &registry::Entry) -> bool {
    match &e.local_refinement {
        Some(action) => action.current_retained(shell),
        None => shell
            .slack_lists
            .view
            .as_ref()
            .and_then(|v| v.loaded.as_ref())
            .is_some_and(|l| l.document == e.original),
    }
}
pub(super) fn current_document<'a>(
    shell: &'a ShellState,
    e: &'a registry::Entry,
) -> Option<&'a Document> {
    if e.local_refinement.is_some() {
        original_current(shell, e).then_some(&e.original)
    } else {
        Some(&shell.slack_lists.view.as_ref()?.loaded.as_ref()?.document)
    }
}
fn choice(e: &registry::Entry) -> Option<Choice> {
    match (&e.target, &e.input) {
        (Target::Filter(_), Input::Filter(v)) => Some(Choice::Filter(v.clone())),
        (Target::Sort(_), Input::Sort(v)) => Some(Choice::Sort(v.clone())),
        (Target::Grouping(_), Input::Grouping(v)) => Some(Choice::Grouping(v.clone())),
        (Target::Visibility { column, .. }, Input::Visibility(v)) => Some(Choice::Visibility {
            column: column.clone(),
            visible: *v,
        }),
        _ => None,
    }
}
pub(super) fn valid(shell: &ShellState, e: &registry::Entry) -> bool {
    let Some(action) = &e.local_refinement else {
        return false;
    };
    refinement::controls::draft_retained(
        shell,
        action,
        &match choice(e) {
            Some(v) => v,
            None => return false,
        },
    )
    .is_some()
}
/// Receives the exact retained entry, including a complete owned Filter capture.
pub(super) fn finish(shell: &mut ShellState, e: &registry::Entry) -> bool {
    let Some(action) = &e.local_refinement else {
        return false;
    };
    let Some(key) = Key::current(shell) else {
        return false;
    };
    if registry::blocked(&key.owner)
        || !owner::available(shell)
        || e.stage != Stage::Editing
        || !original_current(shell, e)
        || !with_registry(|r| {
            r.entries.get(&key).is_some_and(|current| {
                current.serial == e.serial
                    && current.stage == Stage::Editing
                    && current.local_refinement.is_some()
            })
        })
    {
        return false;
    }
    let Some(choice) = choice(e) else {
        return false;
    };
    let Some(draft) = refinement::controls::draft_retained(shell, action, &choice) else {
        return false;
    };
    if !refinement::apply_retained(shell, action, draft) {
        return false;
    }
    with_registry(|r| {
        r.entries.remove(&key);
    });
    true
}

#[cfg(test)]
mod tests;
