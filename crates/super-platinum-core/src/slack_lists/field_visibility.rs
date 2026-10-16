//! One existing saved Table column visibility bit; no schema or record mutation.
use super::{ColumnRename, Document, ListError, PersonalTaskLists, ViewRename};
use serde_json::{Value, json};
#[derive(Clone, Debug, PartialEq)]
pub struct ViewFieldVisibility {
    document: Document,
    view_id: String,
    column_id: String,
    key: String,
    desired: bool,
    expected_metadata: Value,
}
impl ViewFieldVisibility {
    /// Borrowed hint only. Full raw/typed identity and access are checked at admission.
    pub fn current(document: &Document, view: &str, column: &str) -> Option<bool> {
        let identity =
            |s: &str| !s.trim().is_empty() && s.len() <= 1024 && !s.chars().any(char::is_control);
        if !identity(view) || !identity(column) {
            return None;
        }
        let mut views = document.views.iter().filter(|v| v["id"] == view);
        let view = views.next()?;
        if views.next().is_some() {
            return None;
        }
        if view["type"] != "table" || !ViewRename::supports(view) {
            return None;
        }
        let column = document.column(column)?;
        if !identity(&column.key)
            || !ColumnRename::supports(column)
            || column.key.eq_ignore_ascii_case("name")
            || document.primary_column().is_some_and(|c| c.id == column.id)
            || column
                .raw
                .get("is_primary_column")
                .is_some_and(|v| v != &Value::Bool(false))
        {
            return None;
        }
        if let Some(group) = view.get("grouping").filter(|v| !v.is_null()) {
            let group = group.as_object()?;
            for (name, identity) in [
                ("group_by", column.key.as_str()),
                ("group_by_column_id", column.id.as_str()),
            ] {
                if let Some(value) = group.get(name) {
                    let value = value.as_str()?;
                    if value == identity {
                        return None;
                    }
                }
            }
        }
        let entries = view.get("columns")?.as_array()?;
        let mut seen = std::collections::BTreeSet::new();
        let mut found = None;
        for item in entries {
            let key = item.get("key")?.as_str()?;
            if !seen.insert(key) {
                return None;
            }
            if key == column.key {
                // Unlike broad read projection, a write requires the exact explicit ID.
                if item.get("id")?.as_str()? != column.id {
                    return None;
                }
                found = Some(item.get("visible")?.as_bool()?);
            }
        }
        found
    }
    pub fn capture(
        document: &Document,
        view: &str,
        column: &str,
        desired: bool,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<Self, ListError> {
        if Document::parse(document.raw.clone())? != *document {
            return Err(ListError("List representations disagree."));
        }
        eligible(document, allow, personal)?;
        let before = Self::current(document, view, column)
            .ok_or(ListError("This field visibility is not supported."))?;
        if before == desired {
            return Err(ListError("Choose a different field visibility."));
        }
        let key = document.column(column).unwrap().key.clone();
        let mut expected_metadata = document.raw["list_metadata"].clone();
        let target = expected_metadata["views"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|v| v["id"] == view)
            .unwrap();
        target["columns"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|c| c["key"] == key)
            .unwrap()["visible"] = json!(desired);
        Ok(Self {
            document: document.clone(),
            view_id: view.into(),
            column_id: column.into(),
            key,
            desired,
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
        &self.key
    }
    pub fn desired_visible(&self) -> bool {
        self.desired
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
                "This List changed. Cancel and review field visibility again.",
            ));
        }
        eligible(fresh, allow, personal)?;
        if Self::current(fresh, &self.view_id, &self.column_id).is_none() {
            return Err(ListError("This field is no longer available."));
        }
        Ok(())
    }
    /// Full canonical observation, never a mutation ACK or causal attribution.
    pub fn observes_desired(&self, fresh: &Document) -> bool {
        Document::parse(fresh.raw.clone()).is_ok_and(|p| p == *fresh)
            && fresh.id == self.document.id
            && ["created", "user"]
                .iter()
                .all(|k| fresh.raw.get(*k) == self.document.raw.get(*k))
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
pub struct ViewFieldVisibilityAttempt {
    intent: ViewFieldVisibility,
    phase: ViewFieldVisibilityPhase,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewFieldVisibilityPhase {
    Reviewing,
    InFlight,
    NotRequested,
    Unconfirmed,
    Observed,
}
/// A consumed, one-use request-construction capability. Not Clone.
pub struct ViewFieldVisibilityPermit {
    intent: ViewFieldVisibility,
}
impl ViewFieldVisibilityPermit {
    pub(crate) fn into_intent(self) -> ViewFieldVisibility {
        self.intent
    }
}
impl ViewFieldVisibilityAttempt {
    pub fn new(intent: ViewFieldVisibility) -> Self {
        Self {
            intent,
            phase: ViewFieldVisibilityPhase::Reviewing,
        }
    }
    pub fn phase(&self) -> ViewFieldVisibilityPhase {
        self.phase
    }
    pub fn intent(&self) -> &ViewFieldVisibility {
        &self.intent
    }
    pub fn admit(
        &mut self,
        fresh: &Document,
        allow_lists: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<ViewFieldVisibilityPermit, ListError> {
        if self.phase != ViewFieldVisibilityPhase::Reviewing {
            return Err(ListError(
                "This visibility change attempt has already been admitted.",
            ));
        }
        self.intent.revalidate(fresh, allow_lists, personal)?;
        self.phase = ViewFieldVisibilityPhase::InFlight;
        Ok(ViewFieldVisibilityPermit {
            intent: self.intent.clone(),
        })
    }
    /// The host may pass true only for typed pre-dispatch NotRequested evidence.
    /// Every received ACK, API denial, timeout or post-dispatch failure stays
    /// unconfirmed until a separate current metadata read observes the visibility change.
    pub fn finish(&mut self, not_requested: bool) -> bool {
        if self.phase != ViewFieldVisibilityPhase::InFlight {
            return false;
        }
        self.phase = if not_requested {
            ViewFieldVisibilityPhase::NotRequested
        } else {
            ViewFieldVisibilityPhase::Unconfirmed
        };
        true
    }
    pub fn observe(&mut self, fresh: &Document) -> bool {
        if self.phase != ViewFieldVisibilityPhase::Unconfirmed
            || !self.intent.observes_desired(fresh)
        {
            return false;
        }
        self.phase = ViewFieldVisibilityPhase::Observed;
        true
    }
}

#[cfg(test)]
mod tests;
