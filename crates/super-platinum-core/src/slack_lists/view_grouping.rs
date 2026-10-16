//! Exact simple Group by replacement on one existing custom Table or Board.
//! This does not authorize cell changes, lane order, or enriched grouping edits.
use super::{Column, Document, ListError, PersonalTaskLists, ViewRename};
use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ViewGroupingChoice {
    None,
    Column { column_id: String },
}
#[derive(Clone, Debug, PartialEq)]
pub struct ViewGrouping {
    document: Document,
    view_id: String,
    kind: String,
    desired: ViewGroupingChoice,
    grouping: Value,
    expected_metadata: Value,
}
fn identity(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 1024 && !s.chars().any(char::is_control)
}
impl ViewGrouping {
    /// Borrowed choice hint. Capture/admission reparse and verify full raw identity.
    pub fn supports_column(column: &Column) -> bool {
        if !identity(&column.id)
            || !identity(&column.key)
            || ["is_deleted", "deleted", "suggested", "is_suggested"]
                .iter()
                .any(|k| column.raw.get(*k).is_some_and(|v| v != &Value::Bool(false)))
        {
            return false;
        }
        match column.kind.as_str() {
            "checkbox" => column.raw.get("options").is_none_or(|v| v == &json!({})),
            "select" => {
                let Some(options) = column.raw.get("options").and_then(Value::as_object) else {
                    return false;
                };
                options
                    .keys()
                    .all(|k| matches!(k.as_str(), "format" | "choices"))
                    && options.get("format").is_none_or(|v| v == "single_select")
                    && options.get("choices").is_some_and(Value::is_array)
                    && super::saved_views::supports_new_grouping(column)
            }
            _ => false,
        }
    }
    pub fn grouping_columns(document: &Document) -> impl Iterator<Item = &Column> {
        document
            .columns
            .iter()
            .filter(|column| Self::supports_column(column))
    }
    /// None means unsupported; Some(None) means ungrouped. Raw ungrouped variants
    /// remain distinct in the captured document and never authorize a no-op clear.
    /// The host must also require the actual saved projection with its loaded page.
    pub fn current(document: &Document, view_id: &str) -> Option<ViewGroupingChoice> {
        if !identity(view_id) {
            return None;
        }
        let catalog = super::saved_views::catalog(document).ok()?;
        if !catalog.iter().any(|view| view.id == view_id) {
            return None;
        }
        let view = document.views.iter().find(|view| view["id"] == view_id)?;
        if !ViewRename::supports(view) {
            return None;
        }
        let Some(value) = view.get("grouping").filter(|v| !v.is_null()) else {
            return Some(ViewGroupingChoice::None);
        };
        let members = value.as_object()?;
        if members.is_empty() {
            return Some(ViewGroupingChoice::None);
        }
        if members.len() != 1 {
            return None;
        }
        let key = members.get("group_by")?.as_str()?;
        if key.is_empty() {
            return Some(ViewGroupingChoice::None);
        }
        let mut matches = document.columns.iter().filter(|column| column.key == key);
        let column = matches.next()?;
        if matches.next().is_some() || !Self::supports_column(column) {
            return None;
        }
        Some(ViewGroupingChoice::Column {
            column_id: column.id.clone(),
        })
    }
    pub fn capture(
        document: &Document,
        view_id: &str,
        desired: ViewGroupingChoice,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<Self, ListError> {
        if Document::parse(document.raw.clone())? != *document {
            return Err(ListError("List representations disagree."));
        }
        eligible(document, allow, personal)?;
        let before = Self::current(document, view_id)
            .ok_or(ListError("This saved view grouping is not supported."))?;
        if before == desired {
            return Err(ListError("Choose a different grouping."));
        }
        let key = match &desired {
            ViewGroupingChoice::None => "",
            ViewGroupingChoice::Column { column_id } => {
                &document
                    .column(column_id)
                    .filter(|column| Self::supports_column(column))
                    .ok_or(ListError(
                        "Choose a supported Checkbox or single-select field.",
                    ))?
                    .key
            }
        };
        let grouping = json!({"group_by":key});
        let mut expected_metadata = document.raw["list_metadata"].clone();
        let view = expected_metadata["views"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|view| view["id"] == view_id)
            .unwrap();
        let kind = view["type"].as_str().unwrap().to_owned();
        view["grouping"] = grouping.clone();
        Ok(Self {
            document: document.clone(),
            view_id: view_id.into(),
            kind,
            desired,
            grouping,
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
    pub fn kind(&self) -> &str {
        &self.kind
    }
    pub fn desired_grouping(&self) -> &ViewGroupingChoice {
        &self.desired
    }
    /// Whole simple object; clear is {group_by:""}, never omission or null.
    pub fn grouping(&self) -> &Value {
        &self.grouping
    }
    pub fn revalidate(
        &self,
        fresh: &Document,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<(), ListError> {
        if Document::parse(fresh.raw.clone())? != *fresh
            || !super::cell_edit::document_matches(&self.document.raw, &fresh.raw)
        {
            return Err(ListError(
                "This List changed. Cancel and review its grouping again.",
            ));
        }
        eligible(fresh, allow, personal)?;
        if Self::current(fresh, &self.view_id).is_none() {
            return Err(ListError(
                "This saved view grouping is no longer supported.",
            ));
        }
        Ok(())
    }
    /// Canonical state observation, never mutation ACK or causal attribution.
    /// The host separately validates actual current projection before showing it.
    pub fn observes_desired(&self, fresh: &Document) -> bool {
        Document::parse(fresh.raw.clone()).is_ok_and(|parsed| parsed == *fresh)
            && fresh.id == self.document.id
            && ["created", "user"]
                .iter()
                .all(|key| fresh.raw.get(*key) == self.document.raw.get(*key))
            && fresh.raw.get("list_metadata") == Some(&self.expected_metadata)
    }
}
fn eligible(
    doc: &Document,
    allow: Option<&str>,
    personal: Option<&PersonalTaskLists>,
) -> Result<(), ListError> {
    if let Some(reason) = doc.read_only_reason(allow) {
        return Err(ListError(reason));
    }
    if personal.is_none_or(|p| p.contains(&doc.id)) {
        return Err(ListError("Ordinary List preferences are not verified."));
    }
    Ok(())
}

/// Host-retained operation; not serializable and not independently a durable ledger.
/// Account/session/visit/read/revision/cooldown fences belong to the desktop host.
pub struct ViewGroupingAttempt {
    intent: ViewGrouping,
    phase: ViewGroupingPhase,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewGroupingPhase {
    Reviewing,
    InFlight,
    NotRequested,
    Unconfirmed,
    Observed,
}
/// A consumed, one-use request-construction capability. Not Clone.
pub struct ViewGroupingPermit {
    intent: ViewGrouping,
}
impl ViewGroupingPermit {
    pub(crate) fn into_intent(self) -> ViewGrouping {
        self.intent
    }
}
impl ViewGroupingAttempt {
    pub fn new(intent: ViewGrouping) -> Self {
        Self {
            intent,
            phase: ViewGroupingPhase::Reviewing,
        }
    }
    pub fn phase(&self) -> ViewGroupingPhase {
        self.phase
    }
    pub fn intent(&self) -> &ViewGrouping {
        &self.intent
    }
    pub fn admit(
        &mut self,
        fresh: &Document,
        allow_lists: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<ViewGroupingPermit, ListError> {
        if self.phase != ViewGroupingPhase::Reviewing {
            return Err(ListError(
                "This grouping change attempt has already been admitted.",
            ));
        }
        self.intent.revalidate(fresh, allow_lists, personal)?;
        self.phase = ViewGroupingPhase::InFlight;
        Ok(ViewGroupingPermit {
            intent: self.intent.clone(),
        })
    }
    /// The host may pass true only for typed pre-dispatch NotRequested evidence.
    /// Every received ACK, API denial, timeout or post-dispatch failure stays
    /// unconfirmed until a separate current metadata read observes the grouping change.
    pub fn finish(&mut self, not_requested: bool) -> bool {
        if self.phase != ViewGroupingPhase::InFlight {
            return false;
        }
        self.phase = if not_requested {
            ViewGroupingPhase::NotRequested
        } else {
            ViewGroupingPhase::Unconfirmed
        };
        true
    }
    pub fn observe(&mut self, fresh: &Document) -> bool {
        if self.phase != ViewGroupingPhase::Unconfirmed || !self.intent.observes_desired(fresh) {
            return false;
        }
        self.phase = ViewGroupingPhase::Observed;
        true
    }
}

#[cfg(test)]
mod tests;
