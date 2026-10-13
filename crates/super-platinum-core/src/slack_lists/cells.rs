use super::{Column, PersonalTaskLists, Record};
use serde_json::{Number, Value};

#[derive(Clone, Debug, PartialEq)]
pub enum CellValue {
    Empty,
    Boolean(bool),
    Number(Number),
    Text(String),
    RichText(Vec<Value>),
    Values(Vec<String>),
    Link(Value),
    Unsupported(Value),
}
impl Column {
    /// Cheap projection check; actual dispatch still captures and revalidates its owner.
    pub fn editable_checkbox(&self, record: &Record) -> bool {
        self.editable_checkbox_with_policy(record, "", None)
    }
    pub fn editable_checkbox_with_policy(
        &self,
        record: &Record,
        list_id: &str,
        personal_tasks: Option<&PersonalTaskLists>,
    ) -> bool {
        if !super::policy::ordinary_boolean_kind(&self.kind, list_id, personal_tasks)
            || record
                .raw
                .get("archived")
                .is_some_and(|v| !v.is_null() && v != &Value::Bool(false))
            || record
                .raw
                .get("is_deleted")
                .is_some_and(|v| !v.is_null() && v != &Value::Bool(false))
        {
            return false;
        }
        let Some(field) = record.fields.get(&self.key) else {
            return false;
        };
        field.get("checkbox").and_then(Value::as_bool).is_some()
            && field
                .get("column_id")
                .is_none_or(|id| id.as_str() == Some(&self.id))
            && field
                .get("row_id")
                .is_none_or(|id| id.as_str() == Some(&record.id))
    }
    pub fn cell(&self, record: &Record) -> CellValue {
        let Some(field) = record.fields.get(&self.key) else {
            return CellValue::Empty;
        };
        let member = match self.kind.as_str() {
            "text" | "rich_text" => "rich_text",
            "checkbox" | "completed" | "todo_completed" => "checkbox",
            "number" | "currency" => "number",
            "select" | "multi_select" => "select",
            "date" | "due_date" | "todo_due_date" => "date",
            "user" | "assignee" | "todo_assignee" | "vote" => "user",
            "attachment" | "canvas" => "attachment",
            "channel" => "channel",
            "email" => "email",
            "phone" => "phone",
            "rating" => "rating",
            "link" => "link",
            _ => return CellValue::Unsupported(field.clone()),
        };
        let value = field.get(member).or_else(|| {
            if member == "rich_text" {
                field.get("richText")
            } else {
                None
            }
        });
        let Some(value) = value.filter(|v| !v.is_null()) else {
            return record
                .fallback(&self.key)
                .filter(|s| !s.is_empty())
                .map(|s| CellValue::Text(s.into()))
                .unwrap_or_else(|| {
                    if field.as_object().is_some_and(|object| {
                        object.iter().any(|(key, value)| {
                            !matches!(key.as_str(), "key" | "column_id" | "row_id" | "text")
                                && !value.is_null()
                        })
                    }) {
                        CellValue::Unsupported(field.clone())
                    } else {
                        CellValue::Empty
                    }
                });
        };
        if member == "checkbox" {
            return value
                .as_bool()
                .map(CellValue::Boolean)
                .unwrap_or_else(|| CellValue::Unsupported(field.clone()));
        }
        let Some(values) = value.as_array() else {
            return CellValue::Unsupported(field.clone());
        };
        if member == "rich_text" {
            return CellValue::RichText(values.clone());
        }
        if values.is_empty() {
            return CellValue::Empty;
        }
        match member {
            "number" | "rating" => values
                .first()
                .and_then(Value::as_number)
                .cloned()
                .map(CellValue::Number)
                .unwrap_or_else(|| CellValue::Unsupported(field.clone())),
            "link" => {
                if values.len() == 1 && values[0].is_object() {
                    CellValue::Link(values[0].clone())
                } else {
                    CellValue::Unsupported(field.clone())
                }
            }
            _ => match values
                .iter()
                .map(|v| v.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>()
            {
                Some(values) => CellValue::Values(values),
                None => CellValue::Unsupported(field.clone()),
            },
        }
    }
    /// Plain accessible/export fallback; structured renderers should use `presentation`.
    pub fn display_text(&self, record: &Record) -> String {
        self.presentation(record).plain_text()
    }
}
