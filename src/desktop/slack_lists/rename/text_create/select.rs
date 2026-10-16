//! Small owned draft. Row generations, not reusable option strings, own callbacks.
use super::*;
use super_platinum_core::slack_lists::{SelectCreateOption, SelectCreateOptions};
#[derive(Clone, Debug)]
pub(crate) struct Row {
    pub generation: u64,
    pub option: SelectCreateOption,
}
#[derive(Clone, Debug)]
pub(crate) struct Draft {
    pub rows: Vec<Row>,
    pub pending: String,
    pub add_epoch: u64,
    pub error: Option<&'static str>,
}
impl Default for Draft {
    fn default() -> Self {
        Self {
            rows: Vec::new(),
            pending: String::new(),
            add_epoch: next_id(),
            error: None,
        }
    }
}
impl Draft {
    /// Allocation-free UI hint over already typed rows. The authoritative size
    /// and full configuration capture still runs only at the explicit Save boundary.
    pub(crate) fn ready(&self) -> bool {
        self.pending.is_empty()
            && !self.rows.is_empty()
            && self.rows.len() <= SelectCreateOptions::MAX_CHOICES
            && self.rows.iter().enumerate().all(|(index, row)| {
                !self.rows[..index]
                    .iter()
                    .any(|other| other.option.value() == row.option.value())
            })
    }
    pub(crate) fn capture(&self) -> Result<SelectCreateOptions, ListError> {
        if !self.pending.is_empty() {
            return Err(ListError(
                "Add the pending option, or clear its text before creating the field.",
            ));
        }
        SelectCreateOptions::capture(self.rows.iter().map(|r| r.option.clone()).collect())
    }
    pub(crate) fn can_add(&self) -> bool {
        self.rows.len() < SelectCreateOptions::MAX_CHOICES
            && SelectCreateOption::valid_label(&self.pending)
    }
}
pub(crate) enum Edit {
    Pending { epoch: u64, value: String },
    Add { epoch: u64 },
    Remove { row: u64 },
}
pub(crate) fn edit(
    mut state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    selection: u64,
    rendered: Choice,
    edit: Edit,
) {
    let return_focus = matches!(edit, Edit::Add { .. } | Edit::Remove { .. });
    if edit_shell(&state.peek(), serial, visit, selection, rendered, edit) {
        state.write();
        if return_focus {
            let epoch = super::entry(&state.peek())
                .map(|e| e.select_draft.add_epoch)
                .unwrap_or_default();
            dioxus::document::eval(&format!(
                "({})({serial},{},{epoch})",
                include_str!("select_focus.js"),
                rendered.revision
            ));
        }
    }
}
pub(crate) fn edit_shell(
    shell: &ShellState,
    serial: u64,
    visit: u64,
    selection: u64,
    rendered: Choice,
    edit: Edit,
) -> bool {
    if rendered.kind != ColumnCreateKind::Select {
        return false;
    }
    with_edit(shell, serial, visit, selection, rendered, |entry| {
        let draft = &mut entry.select_draft;
        match edit {
            Edit::Pending { epoch, value } => {
                if epoch != draft.add_epoch || value.len() > 4096 || draft.pending == value {
                    return false;
                }
                draft.pending = value;
                draft.error = None;
            }
            Edit::Add { epoch } => {
                if epoch != draft.add_epoch {
                    return false;
                }
                if !draft.can_add() {
                    draft.error = Some(
                        "Add up to eight options. Each label needs 1-256 UTF-16 units on one line.",
                    );
                    return true;
                }
                let mut choice = None;
                for _ in 0..4 {
                    match SelectCreateOption::generate(&draft.pending) {
                        Ok(candidate)
                            if !draft
                                .rows
                                .iter()
                                .any(|r| r.option.value() == candidate.value()) =>
                        {
                            choice = Some(candidate);
                            break;
                        }
                        Ok(_) => (),
                        Err(error) => {
                            draft.error = Some(error.0);
                            return true;
                        }
                    }
                }
                let Some(option) = choice else {
                    draft.error =
                        Some("Could not create a distinct option identity. Try adding it again.");
                    return true;
                };
                // Consume synchronously, before the next render. A duplicate old
                // Add/input callback cannot consume later text or replay this label.
                draft.pending.clear();
                draft.add_epoch = next_id();
                draft.error = None;
                draft.rows.push(Row {
                    generation: next_id(),
                    option,
                });
            }
            Edit::Remove { row } => {
                let Some(index) = draft.rows.iter().position(|r| r.generation == row) else {
                    return false;
                };
                draft.rows.remove(index);
                draft.error = None;
            }
        }
        true
    })
}

/// Test teardown exercises the actual Cancel boundary for this exact owned editor.
#[cfg(test)]
pub(crate) fn close_fixture(shell: &ShellState) -> bool {
    let Some(e) = super::entry(shell).filter(|e| {
        e.target == Target::NewTextField("V_FILTER".into())
            && e.stage == Stage::Editing
            && e.field_choice.kind == ColumnCreateKind::Select
            && e.job.is_none()
            && e.intent.is_none()
    }) else {
        return false;
    };
    super::super::close_entry(shell, e.serial)
}
