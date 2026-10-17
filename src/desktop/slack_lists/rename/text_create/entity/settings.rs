//! Explicit local configuration actions; only Create captures a network intent.
use super::*;
use super_platinum_core::slack_lists::EntityCreateFormat;

#[derive(Clone, Debug)]
pub(crate) enum Edit {
    Format(EntityCreateFormat),
    ShowNames(bool),
    Activate(String),
    Remove(String),
}
pub(crate) fn edit(mut state: Signal<ShellState>, stamp: Stamp, action: Edit) {
    if edit_shell(&state.peek(), stamp, action) {
        state.write();
    }
}
pub(crate) fn edit_shell(shell: &ShellState, stamp: Stamp, action: Edit) -> bool {
    stamp.with(shell, |draft| {
        let options = match &action {
            Edit::Format(format) => draft.options.with_format(*format),
            Edit::ShowNames(show) => draft.options.with_show_names(*show),
            Edit::Activate(id) => {
                if !draft.directory.contains(id)
                    || !directory::cache_allows(shell, directory_kind(draft.options.domain()), id)
                {
                    return false;
                }
                if draft.options.format() == EntityCreateFormat::Single {
                    draft.options.select_single(id)
                } else {
                    let mut values = draft.options.selected_ids().to_vec();
                    if let Some(index) = values.iter().position(|value| value == id) {
                        values.remove(index);
                    } else {
                        values.push(id.clone());
                    }
                    draft.options.select_multiple(values)
                }
            }
            Edit::Remove(id) => {
                // Only the multiple control has selected-item removal. Single
                // activation uses the reference callback's full scalar equality.
                if draft.options.format() != EntityCreateFormat::Multiple
                    || !draft.options.selected_ids().contains(id)
                {
                    return false;
                }
                draft.options.select_multiple(
                    draft
                        .options
                        .selected_ids()
                        .iter()
                        .filter(|value| *value != id)
                        .cloned()
                        .collect(),
                )
            }
        };
        let Ok(options) = options else {
            return false;
        };
        if options == draft.options {
            return false;
        }
        let selected = options.selected_ids();
        let mut labels = draft.labels.clone();
        for c in draft
            .directory
            .presentation()
            .candidates
            .iter()
            .filter(|c| selected.contains(&c.id))
        {
            labels.insert(c.id.clone(), c.label.clone());
        }
        labels.retain(|id, _| selected.contains(id));
        if !draft.replace_options(options) {
            return false;
        }
        draft.labels = labels;
        true
    })
}
pub(crate) fn capture(
    shell: &ShellState,
    entry: &registry::Entry,
) -> Result<EntityCreateOptions, ListError> {
    let draft = entry
        .entity_draft
        .as_ref()
        .filter(|d| Some(d.options.domain()) == domain(entry.field_choice.kind))
        .ok_or(ListError(
            "The default settings changed. Review this field again.",
        ))?;
    if draft
        .options
        .selected_ids()
        .iter()
        .any(|id| !directory::cache_allows(shell, directory_kind(draft.options.domain()), id))
    {
        return Err(ListError(
            "A selected default is no longer available. Review the defaults before creating this field.",
        ));
    }
    Ok(draft.options.clone())
}

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) fn close_test(shell: &ShellState) -> bool {
    Stamp::capture(shell)
        .is_some_and(|stamp| crate::slack_lists::rename::close_entry(shell, stamp.serial))
}
