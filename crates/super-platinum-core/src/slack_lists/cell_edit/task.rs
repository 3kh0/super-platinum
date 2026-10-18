//! Retained T7xK: TodoAssignee checks newly selected users with files.canSee.
//! Share is optional: Not Now still assigns. Permission grants are a separate,
//! explicit workflow; this module never converts a selection into a file share.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskAssignmentDecision {
    /// Every newly selected user has independently observed access.
    ExistingAccess,
    /// The user explicitly chose Not Now in the access dialog.
    AssignWithoutSharing,
}

#[derive(Clone, Debug)]
pub struct TaskAssignment {
    edit: CellEdit,
    checks: Vec<(String, Option<bool>)>,
}
impl Column {
    pub fn task_edit_value(
        &self,
        record: &Record,
        personal: &PersonalTaskLists,
    ) -> Result<CellEditValue, ListError> {
        let _ = personal; // Presence establishes a verified account-scoped preference response.
        eligible(self, record)?;
        value::read(self, record)
    }
}
pub(super) fn eligible(column: &Column, record: &Record) -> Result<(), ListError> {
    if column.kind == "todo_completed" {
        return super::completion::value(column, record).map(|_| ());
    }
    if !matches!(
        column.kind.as_str(),
        "assignee" | "todo_assignee" | "date" | "due_date" | "todo_due_date"
    ) {
        return Err(ListError("This field does not use the To-do editor."));
    }
    value::identity(column, record)?;
    value::read(column, record).map(|_| ())
}
fn capture(
    document: &Document,
    page: &RecordsPage,
    row: &str,
    column_id: &str,
    allow_lists: Option<&str>,
    personal: &PersonalTaskLists,
    desired: CellEditValue,
) -> Result<CellEdit, ListError> {
    if let Some(reason) = document.read_only_reason(allow_lists) {
        return Err(ListError(reason));
    }
    let column = document
        .column(column_id)
        .ok_or(ListError("This To-do field is unavailable."))?;
    let record = page
        .record(row)
        .ok_or(ListError("This To-do item is unavailable."))?;
    if record
        .raw
        .get("list_id")
        .is_some_and(|id| id.as_str() != Some(&document.id))
    {
        return Err(ListError("This To-do record belongs to another List."));
    }
    let initial = column.task_edit_value(record, personal)?;
    value::validate(column, &desired, &initial)?;
    if initial == desired {
        return Err(ListError("This To-do field already has that value."));
    }
    Ok(CellEdit {
        inner: Edit::Typed {
            list_id: document.id.clone(),
            record_id: record.id.clone(),
            column_id: column.id.clone(),
            column_key: column.key.clone(),
            document: document.raw.clone(),
            record: record.raw.clone(),
            column: column.raw.clone(),
            value: desired,
            task_policy: Some(personal.clone()),
        },
    })
}
impl TaskAssignment {
    pub fn capture(
        document: &Document,
        page: &RecordsPage,
        row: &str,
        column_id: &str,
        allow_lists: Option<&str>,
        personal: &PersonalTaskLists,
        users: Vec<String>,
    ) -> Result<Self, ListError> {
        let column = document
            .column(column_id)
            .ok_or(ListError("This assignee field is unavailable."))?;
        if !matches!(column.kind.as_str(), "assignee" | "todo_assignee") {
            return Err(ListError("This is not an assignee field."));
        }
        let record = page
            .record(row)
            .ok_or(ListError("This To-do item is unavailable."))?;
        let CellEditValue::Users(original) = column.task_edit_value(record, personal)? else {
            return Err(ListError("This assignee value is malformed."));
        };
        let checks = if column.kind == "todo_assignee" {
            users
                .iter()
                .filter(|id| !original.contains(id))
                .cloned()
                .map(|id| (id, None))
                .collect()
        } else {
            Vec::new()
        };
        Ok(Self {
            edit: capture(
                document,
                page,
                row,
                column_id,
                allow_lists,
                personal,
                CellEditValue::Users(users),
            )?,
            checks,
        })
    }
    pub fn list_id(&self) -> &str {
        self.edit.list_id()
    }
    pub fn users_to_check(&self) -> impl Iterator<Item = &str> {
        self.checks
            .iter()
            .filter(|(_, value)| value.is_none())
            .map(|(id, _)| id.as_str())
    }
    pub fn record_access(&mut self, user: &str, response: &Value) -> Result<(), ListError> {
        let slot = self
            .checks
            .iter_mut()
            .find(|(id, value)| id == user && value.is_none())
            .ok_or(ListError(
                "This assignee access check is stale or duplicated.",
            ))?;
        if response.get("ok") != Some(&Value::Bool(true))
            || response.get("error").is_some_and(|v| !v.is_null())
        {
            return Err(ListError("This assignee's List access was not verified."));
        }
        let allowed = response
            .get("can_see")
            .and_then(Value::as_bool)
            .ok_or(ListError("This assignee's List access was not verified."))?;
        slot.1 = Some(allowed);
        Ok(())
    }
    pub fn sharing_needed(&self) -> Result<Vec<String>, ListError> {
        if self.checks.iter().any(|(_, value)| value.is_none()) {
            return Err(ListError("Wait for the assignee access checks."));
        }
        Ok(self
            .checks
            .iter()
            .filter(|(_, value)| *value == Some(false))
            .map(|(id, _)| id.clone())
            .collect())
    }
    /// A successful share reply alone is not an access observation. The host
    /// rechecks canSee for this same user after the explicit Share action.
    pub fn observe_shared_access(&mut self, user: &str, response: &Value) -> Result<(), ListError> {
        let slot = self
            .checks
            .iter_mut()
            .find(|(id, value)| id == user && *value == Some(false))
            .ok_or(ListError(
                "This shared assignee no longer needs an access check.",
            ))?;
        if response.get("ok") != Some(&Value::Bool(true))
            || response.get("can_see") != Some(&Value::Bool(true))
            || response.get("error").is_some_and(|v| !v.is_null())
        {
            return Err(ListError("The shared List access is still unconfirmed."));
        }
        slot.1 = Some(true);
        Ok(())
    }
    pub fn revalidate(
        &self,
        document: &Document,
        page: &RecordsPage,
        allow_lists: Option<&str>,
        personal: &PersonalTaskLists,
    ) -> Result<(), ListError> {
        self.edit
            .revalidate(document, page, allow_lists, Some(personal))
    }
    pub fn into_edit(self, decision: TaskAssignmentDecision) -> Result<CellEdit, ListError> {
        let missing = self.sharing_needed()?;
        if !missing.is_empty() && decision != TaskAssignmentDecision::AssignWithoutSharing {
            return Err(ListError(
                "Choose Share or Not Now before assigning these people.",
            ));
        }
        Ok(self.edit)
    }
}
impl CellEdit {
    /// Calendar dates retain their existing date[] encoder. Reminders and
    /// timestamp/range values are deliberately not coerced into this method.
    pub fn capture_task_date(
        document: &Document,
        page: &RecordsPage,
        row: &str,
        column_id: &str,
        allow_lists: Option<&str>,
        personal: &PersonalTaskLists,
        date: Option<String>,
    ) -> Result<Self, ListError> {
        capture(
            document,
            page,
            row,
            column_id,
            allow_lists,
            personal,
            CellEditValue::Date(date),
        )
    }
}
#[cfg(test)]
mod tests;
