//! Calendar date[] adapter for ordinary and special To-do fields. The existing
//! mounted date input and cell worker retain draft ownership and write custody.
use super::*;
use super_platinum_core::slack_lists::{Column, Record};
pub(crate) mod fixture;
mod route;
pub(crate) use route::{open_detail_reminder, open_reminder, reminder_for, reminder_reason};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Calendar,
    PersonalReminder,
}
/// Exact mode evidence belongs to the mounted draft and to its one-attempt
/// receipt. A later experiment response must never reinterpret a calendar save.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::slack_lists) struct Policy {
    personal: bool,
    todos: bool,
    rollback: bool,
    subtask: bool,
}
impl Policy {
    fn read(loaded: &Loaded, record: &Record) -> Result<Self, &'static str> {
        let personal = loaded
            .personal_task_lists
            .as_ref()
            .ok_or("Refresh your task preferences before editing this date.")?
            .contains(&loaded.document.id);
        let experiments = loaded
            .list_experiments
            .as_ref()
            .ok_or("Refresh your task settings before editing this date.")?;
        Ok(Self {
            personal,
            todos: experiments.todos,
            rollback: experiments.todos_rollback,
            subtask: record.parent_id().is_some(),
        })
    }
    fn mode(self) -> Mode {
        if self.personal && self.todos && !self.rollback && !self.subtask {
            Mode::PersonalReminder
        } else {
            Mode::Calendar
        }
    }
    pub(in crate::slack_lists) fn current(self, loaded: &Loaded, row: &str) -> bool {
        loaded
            .records
            .records
            .iter()
            .find(|record| record.id == row)
            .is_some_and(|record| Self::read(loaded, record) == Ok(self))
            && self.mode() == Mode::Calendar
    }
}
pub(in crate::slack_lists) fn mode(
    loaded: &Loaded,
    record: &Record,
    column: &Column,
) -> Result<Mode, &'static str> {
    match column.kind.as_str() {
        "date" | "due_date" => Ok(Mode::Calendar),
        "todo_due_date" => Ok(Policy::read(loaded, record)?.mode()),
        _ => Err("This field is not a date."),
    }
}
pub(super) fn calendar_policy(
    loaded: &Loaded,
    record: &Record,
    column: &Column,
) -> Result<Option<Policy>, &'static str> {
    if column.kind != "todo_due_date" {
        return Ok(None);
    }
    let policy = Policy::read(loaded, record)?;
    if policy.mode() != Mode::Calendar {
        return Err(
            "Use Remind me for this personal To-do. Its timestamp is not a calendar-date cell.",
        );
    }
    Ok(Some(policy))
}
pub(in crate::slack_lists) fn supports(column: &Column) -> bool {
    matches!(column.kind.as_str(), "date" | "due_date" | "todo_due_date")
}
pub(in crate::slack_lists) fn input(
    loaded: &Loaded,
    record: &Record,
    column: &Column,
) -> Result<CellEditValue, &'static str> {
    if !supports(column) {
        return Err("This field is not a calendar date.");
    }
    calendar_policy(loaded, record, column)?;
    column
        .task_edit_value(
            record,
            loaded.personal_task_lists.as_ref().ok_or("Refresh your task preferences before editing this date.")?,
        )
        .map_err(|_| "This date uses a timestamp, range, or value that this editor cannot safely change.")
}
pub(super) fn capture(
    loaded: &Loaded,
    row: &str,
    column: &str,
    value: CellEditValue,
) -> Result<CellEdit, &'static str> {
    let target = loaded
        .document
        .column(column)
        .ok_or("This date field is no longer available.")?;
    let record = loaded
        .records
        .records
        .iter()
        .find(|record| record.id == row)
        .ok_or("This item is no longer available.")?;
    calendar_policy(loaded, record, target)?;
    let CellEditValue::Date(date) = value else {
        return Err("Choose a calendar date or explicitly clear it.");
    };
    CellEdit::capture_task_date(
        &loaded.document,
        &loaded.records,
        row,
        column,
        loaded.allow_lists.as_deref(),
        loaded
            .personal_task_lists
            .as_ref()
            .ok_or("Refresh your task preferences before editing this date.")?,
        date,
    )
    .map_err(|_| "This date cannot be saved. Check the date and your current access.")
}

#[cfg(test)]
mod tests;
