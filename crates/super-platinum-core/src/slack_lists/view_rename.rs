//! Existing custom Table/Board view-name update, source 0Kzd/ViewEditorMenu.
//! No view creation, layout edits, defaults or pagination.
use super::{Document, ListError, PersonalTaskLists};
use serde_json::{Value, json};

/// Immutable reviewed schema. This is content evidence, not account/dispatch authority.
#[derive(Clone, Debug, PartialEq)]
pub struct ViewRename {
    document: Document,
    view_id: String,
    kind: String,
    grouping: Option<Value>,
    desired: String,
    expected_metadata: Value,
}

impl ViewRename {
    pub fn supports(raw: &Value) -> bool {
        supported_view(raw)
    }
    pub fn capture(
        document: &Document,
        view_id: &str,
        name: &str,
        allow_lists: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<Self, ListError> {
        // Public model fields must not bypass parsing or disagree with their raw data.
        if Document::parse(document.raw.clone())? != *document {
            return Err(ListError("List schema representations disagree."));
        }
        eligible(document, view_id, allow_lists, personal)?;
        let view = document.views.iter().find(|v| v["id"] == view_id).unwrap();
        let desired = name;
        // Conservative native bound aligned with the captured 60-character input.
        // Preserve nonblank text exactly; the source caller does not trim it.
        if desired.trim().is_empty()
            || desired.encode_utf16().count() > 60
            || desired.chars().any(char::is_control)
            || view["name"].as_str() == Some(desired)
        {
            return Err(ListError(
                "Enter a different, nonempty view name on one line.",
            ));
        }
        let mut expected_metadata = document.raw["list_metadata"].clone();
        let schema = expected_metadata["views"].as_array_mut().unwrap();
        let target = schema.iter_mut().find(|c| c["id"] == view_id).unwrap();
        target["name"] = json!(desired);
        Ok(Self {
            document: document.clone(),
            view_id: view_id.into(),
            kind: view["type"].as_str().unwrap().into(),
            grouping: view.get("grouping").filter(|v| !v.is_null()).cloned(),
            desired: desired.into(),
            expected_metadata,
        })
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
    pub fn grouping(&self) -> Option<&Value> {
        self.grouping.as_ref()
    }
    pub fn desired_name(&self) -> &str {
        &self.desired
    }
    pub fn original(&self) -> &Document {
        &self.document
    }
    pub fn revalidate(
        &self,
        fresh: &Document,
        allow_lists: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<(), ListError> {
        if Document::parse(fresh.raw.clone())? != *fresh
            || !super::cell_edit::document_matches(&self.document.raw, &fresh.raw)
        {
            return Err(ListError(
                "This List changed. Review the saved view before renaming it.",
            ));
        }
        eligible(fresh, &self.view_id, allow_lists, personal)
    }
    /// Observation of the desired state, not proof this attempt caused it.
    /// Compare every list_metadata member: target identity/type/options, siblings,
    /// order, views and unknown schema metadata. Fresh top-level server file
    /// timestamps/access may differ; they must be projected from the fresh read.
    pub fn observes_desired(&self, fresh: &Document) -> bool {
        Document::parse(fresh.raw.clone()).is_ok_and(|parsed| parsed == *fresh)
            && fresh.id == self.document.id
            && ["created", "user"]
                .iter()
                .all(|key| fresh.raw.get(*key) == self.document.raw.get(*key))
            && fresh.raw.get("list_metadata") == Some(&self.expected_metadata)
    }
}

/// Borrowed eligibility hint only; it does not authorize dispatch.
fn supported_view(raw: &Value) -> bool {
    matches!(raw["type"].as_str(), Some("table" | "board"))
        && raw
            .get("default_view_key")
            .is_none_or(|v| v.is_null() || v.as_str() == Some(""))
        && ["is_all_items_view", "is_locked"]
            .iter()
            .all(|k| raw.get(*k).is_none_or(|v| v == &Value::Bool(false)))
        && raw
            .get("name")
            .is_some_and(|v| v.as_str().is_some_and(|s| !s.is_empty()))
}

fn eligible(
    document: &Document,
    view_id: &str,
    allow_lists: Option<&str>,
    personal: Option<&PersonalTaskLists>,
) -> Result<(), ListError> {
    if let Some(reason) = document.read_only_reason(allow_lists) {
        return Err(ListError(reason));
    }
    if personal.is_none_or(|p| p.contains(&document.id)) {
        return Err(ListError("Ordinary List preferences are not verified."));
    }
    let view = document
        .views
        .iter()
        .find(|v| v["id"] == view_id)
        .ok_or(ListError("The saved view is unavailable."))?;
    let parsed = super::saved_views::catalog(document)
        .map_err(|_| ListError("Saved view identities are malformed or ambiguous."))?;
    if !parsed.iter().any(|v| v.id == view_id) || !supported_view(view) {
        return Err(ListError(
            "Rename an ordinary unlocked custom Table or Board view.",
        ));
    }
    if view
        .get("grouping")
        .filter(|v| !v.is_null())
        .is_some_and(|v| !v.is_object() || serde_json::to_vec(v).map_or(true, |b| b.len() > 8192))
    {
        return Err(ListError("This view grouping cannot be preserved safely."));
    }
    Ok(())
}

/// Host-retained operation; not serializable and not independently a durable ledger.
/// Account/session/visit/read/revision/cooldown fences belong to the desktop host.
pub struct ViewRenameAttempt {
    intent: ViewRename,
    phase: ViewRenamePhase,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewRenamePhase {
    Reviewing,
    InFlight,
    NotRequested,
    Unconfirmed,
    Observed,
}
/// A consumed, one-use request-construction capability. Not Clone.
pub struct ViewRenamePermit {
    intent: ViewRename,
}
impl ViewRenamePermit {
    pub(crate) fn into_intent(self) -> ViewRename {
        self.intent
    }
}
impl ViewRenameAttempt {
    pub fn new(intent: ViewRename) -> Self {
        Self {
            intent,
            phase: ViewRenamePhase::Reviewing,
        }
    }
    pub fn phase(&self) -> ViewRenamePhase {
        self.phase
    }
    pub fn intent(&self) -> &ViewRename {
        &self.intent
    }
    pub fn admit(
        &mut self,
        fresh: &Document,
        allow_lists: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<ViewRenamePermit, ListError> {
        if self.phase != ViewRenamePhase::Reviewing {
            return Err(ListError("This rename attempt has already been admitted."));
        }
        self.intent.revalidate(fresh, allow_lists, personal)?;
        self.phase = ViewRenamePhase::InFlight;
        Ok(ViewRenamePermit {
            intent: self.intent.clone(),
        })
    }
    /// The host may pass true only for typed pre-dispatch NotRequested evidence.
    /// Every received ACK, API denial, timeout or post-dispatch failure stays
    /// unconfirmed until a separate current metadata read observes the rename.
    pub fn finish(&mut self, not_requested: bool) -> bool {
        if self.phase != ViewRenamePhase::InFlight {
            return false;
        }
        self.phase = if not_requested {
            ViewRenamePhase::NotRequested
        } else {
            ViewRenamePhase::Unconfirmed
        };
        true
    }
    pub fn observe(&mut self, fresh: &Document) -> bool {
        if self.phase != ViewRenamePhase::Unconfirmed || !self.intent.observes_desired(fresh) {
            return false;
        }
        self.phase = ViewRenamePhase::Observed;
        true
    }
}

#[cfg(test)]
mod tests;
