//! BoardAddGroupButton / VpmY / BoardView q -> TC0j -> HC6Z.
//! Ordinary Select choice addition, label and color only. Deletion has a separate
//! projected-cell write and is intentionally not a variant of this schema intent.
use super::{Column, Document, ListError, PersonalTaskLists, SelectCreateOption};
use serde_json::{Value, json};
use std::collections::BTreeSet;
mod attempt;
pub use attempt::{BoardChoiceAttempt, BoardChoicePermit, BoardChoicePhase};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BoardChoiceAction {
    Add {
        value: String,
        label: String,
        color: String,
    },
    Rename {
        value: String,
        label: String,
    },
    Color {
        value: String,
        color: String,
    },
}
impl BoardChoiceAction {
    /// Capture generated identity once at the user action, never during render.
    pub fn generate_add(label: &str, color: &str) -> Result<Self, ListError> {
        let generated = SelectCreateOption::generate("Board option")?;
        let action = Self::Add {
            value: generated.value().into(),
            label: label.into(),
            color: color.into(),
        };
        action.validate()?;
        Ok(action)
    }
    fn validate(&self) -> Result<(), ListError> {
        let (value, label, color) = match self {
            Self::Add {
                value,
                label,
                color,
            } => (value, Some(label), Some(color)),
            Self::Rename { value, label } => (value, Some(label), None),
            Self::Color { value, color } => (value, None, Some(color)),
        };
        if !identity(value) {
            return Err(unsupported());
        }
        if let Some(label) = label {
            // Local bounded single-line policy, not an asserted Slack limit.
            if label.encode_utf16().count() > 256
                || label.chars().any(char::is_control)
                || matches!(self, Self::Add { .. }) && js_trim(label).is_empty()
            {
                return Err(ListError(
                    "Use one line of at most 256 UTF-16 units for the group name.",
                ));
            }
        }
        if let Some(color) = color {
            // Reuse the source-backed palette and generated identity format.
            SelectCreateOption::capture(
                "Board option",
                if matches!(self, Self::Add { .. }) {
                    value
                } else {
                    "OptABCDEFG1"
                },
                color,
            )?;
        }
        Ok(())
    }
}
fn js_trim(value: &str) -> &str {
    value.trim_matches(|c| matches!(c,'\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{00a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}'))
}
fn identity(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 1024 && !s.chars().any(char::is_control)
}
fn unsupported() -> ListError {
    ListError("Changing this Board group is not supported yet.")
}

/// Lossless domain subset of iLfW.CN. Output-only emoji_url and narrowed typed
/// defaults are not silently removed. Unknown options and choice members remain.
pub(crate) fn supported_options(value: &Value) -> bool {
    let Some(options) = value.as_object() else {
        return false;
    };
    if options.contains_key("emoji_url")
        || options.contains_key("emojiUrl")
        || options.contains_key("defaultValueTyped")
        || value.to_string().len() > 60 * 1024
    {
        return false;
    }
    if let Some(default) = options.get("default_value_typed") {
        let Some(default) = default.as_object() else {
            return false;
        };
        if default.iter().any(|(key, value)| {
            !matches!(key.as_str(), "select" | "user" | "channel")
                || value
                    .as_array()
                    .is_none_or(|a| a.iter().any(|v| !v.is_string()))
        }) {
            return false;
        }
    }
    if options
        .get("format")
        .is_some_and(|v| !matches!(v.as_str(), Some("single_select" | "multi_select")))
    {
        return false;
    }
    let Some(choices) = options.get("choices").and_then(Value::as_array) else {
        return false;
    };
    // Local resource bound; raw unknown content remains inside this bound.
    if choices.len() > 1024 {
        return false;
    }
    let mut seen = BTreeSet::new();
    choices.iter().all(|choice| {
        choice.is_object()
            && choice["value"]
                .as_str()
                .is_some_and(|id| identity(id) && seen.insert(id))
            && choice["label"].is_string()
    })
}
pub(super) fn basis<'a>(
    doc: &'a Document,
    view_id: &str,
    allow: Option<&str>,
    personal: Option<&PersonalTaskLists>,
) -> Result<&'a Column, ListError> {
    if Document::parse(doc.raw.clone())? != *doc {
        return Err(unsupported());
    }
    if let Some(reason) = doc.read_only_reason(allow) {
        return Err(ListError(reason));
    }
    if personal.is_none_or(|p| p.contains(&doc.id)) {
        return Err(ListError("Ordinary List preferences are not verified."));
    }
    let mut seen = BTreeSet::new();
    for view in &doc.views {
        if !view["id"]
            .as_str()
            .is_some_and(|id| identity(id) && seen.insert(id))
        {
            return Err(unsupported());
        }
    }
    let view = doc
        .views
        .iter()
        .find(|view| view["id"] == view_id && view["type"] == "board")
        .ok_or_else(unsupported)?;
    let key = view
        .pointer("/grouping/group_by")
        .and_then(Value::as_str)
        .filter(|key| identity(key))
        .ok_or_else(unsupported)?;
    let mut columns = doc.columns.iter().filter(|col| col.key == key);
    let column = columns.next().ok_or_else(unsupported)?;
    if columns.next().is_some()
        || column.kind != "select"
        || !supported_options(&column.raw["options"])
        || ["deleted", "is_deleted", "suggested", "is_suggested"]
            .iter()
            .any(|key| {
                column
                    .raw
                    .get(*key)
                    .is_some_and(|v| v != &Value::Bool(false))
            })
    {
        return Err(unsupported());
    }
    match view.pointer("/grouping/group_by_column_id") {
        None | Some(Value::Null) => {}
        Some(Value::String(id)) if id.is_empty() || id == &column.id => {}
        _ => return Err(unsupported()),
    }
    Ok(column)
}

#[derive(Clone, Debug, PartialEq)]
pub struct BoardChoiceChange {
    document: Document,
    refinement: Option<super::ViewRefinementDraft>,
    view_id: String,
    column_id: String,
    column_key: String,
    action: BoardChoiceAction,
    options: Value,
    expected_metadata: Value,
}
impl BoardChoiceChange {
    /// Read-only presentation gate; capture and admission repeat full validation.
    pub fn supports(
        doc: &Document,
        view_id: &str,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> bool {
        basis(doc, view_id, allow, personal).is_ok()
    }

    pub(crate) fn supported_wire_options(options: &Value) -> bool {
        supported_options(options)
    }
    pub fn capture(
        doc: &Document,
        view_id: &str,
        action: BoardChoiceAction,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<Self, ListError> {
        Self::capture_read(doc, doc, view_id, action, allow, personal)
    }
    pub fn capture_refined(
        doc: &Document,
        local: &super::ViewRefinementDraft,
        action: BoardChoiceAction,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<Self, ListError> {
        let read = super::BoardLaneOrder::refined_projection_document(doc, local, None)?;
        let mut intent = Self::capture_read(doc, &read, local.view_id(), action, allow, personal)?;
        intent.refinement = Some(local.clone());
        Ok(intent)
    }
    pub fn supports_refined(
        doc: &Document,
        local: &super::ViewRefinementDraft,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> bool {
        super::BoardLaneOrder::refined_projection_document(doc, local, None)
            .is_ok_and(|read| Self::supports(&read, local.view_id(), allow, personal))
    }
    fn capture_read(
        doc: &Document,
        read: &Document,
        view_id: &str,
        action: BoardChoiceAction,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<Self, ListError> {
        action.validate()?;
        let col = basis(read, view_id, allow, personal)?;
        let mut options = col.raw["options"].clone();
        let choices = options["choices"].as_array_mut().unwrap();
        match &action {
            BoardChoiceAction::Add {
                value,
                label,
                color,
            } => {
                if choices
                    .iter()
                    .any(|choice| choice["value"] == value.as_str())
                {
                    return Err(ListError("This group identity is already present."));
                }
                choices.push(json!({"value":value,"label":label,"color":color}));
            }
            BoardChoiceAction::Rename { value, label } => {
                let choice = choices
                    .iter_mut()
                    .find(|choice| choice["value"] == value.as_str())
                    .ok_or_else(unsupported)?;
                choice["label"] = json!(js_trim(label));
            }
            BoardChoiceAction::Color { value, color } => {
                let choice = choices
                    .iter_mut()
                    .find(|choice| choice["value"] == value.as_str())
                    .ok_or_else(unsupported)?;
                choice["color"] = json!(color);
            }
        }
        if options == col.raw["options"] {
            return Err(ListError("Choose a different group name or color."));
        }
        if !supported_options(&options) {
            return Err(unsupported());
        }
        let mut expected_metadata = doc.raw["list_metadata"].clone();
        let target = expected_metadata["schema"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|c| c["id"] == col.id)
            .unwrap();
        target["options"] = options.clone();
        Ok(Self {
            document: doc.clone(),
            refinement: None,
            view_id: view_id.into(),
            column_id: col.id.clone(),
            column_key: col.key.clone(),
            action,
            options,
            expected_metadata,
        })
    }
    pub fn original(&self) -> &Document {
        &self.document
    }
    pub fn list_id(&self) -> &str {
        &self.document.id
    }
    pub fn view_id(&self) -> &str {
        &self.view_id
    }
    pub fn column_id(&self) -> &str {
        &self.column_id
    }
    pub fn column_key(&self) -> &str {
        &self.column_key
    }
    pub fn action(&self) -> &BoardChoiceAction {
        &self.action
    }
    pub fn options(&self) -> &Value {
        &self.options
    }
    pub fn revalidate(
        &self,
        fresh: &Document,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<(), ListError> {
        if !super::cell_edit::document_matches(&self.document.raw, &fresh.raw) {
            return Err(ListError("This List changed. Review the group again."));
        }
        if let Some(local) = &self.refinement {
            let read = super::BoardLaneOrder::refined_projection_document(fresh, local, None)?;
            basis(&read, &self.view_id, allow, personal).map(|_| ())
        } else {
            basis(fresh, &self.view_id, allow, personal).map(|_| ())
        }
    }
    /// Independent exact metadata observation, not proof of causation or record
    /// preservation. Source backfills defaults: the host must refresh records too.
    pub fn observes_desired(&self, fresh: &Document) -> bool {
        Document::parse(fresh.raw.clone()).is_ok_and(|parsed| parsed == *fresh)
            && fresh.id == self.document.id
            && ["created", "user"]
                .iter()
                .all(|k| fresh.raw.get(*k) == self.document.raw.get(*k))
            && fresh.raw.get("list_metadata") == Some(&self.expected_metadata)
    }
}
#[cfg(test)]
mod tests;
