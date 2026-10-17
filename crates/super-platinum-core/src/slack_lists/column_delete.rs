//! Ordinary field deletion from retained L34r/Jd3h and field-menu source.
//! Task bundles have a separate removal command and are deliberately excluded.
use super::{Document, ListError, PersonalTaskLists};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct ColumnDelete {
    document: Document,
    authenticated_user: String,
    column_id: String,
    column_key: String,
    expected_metadata: Value,
}
impl ColumnDelete {
    /// Presentation support; capture and admission additionally verify raw identity,
    /// permissions and personal-list policy. Known ordinary types do not rewrite options.
    pub fn supports(document: &Document, column_id: &str) -> bool {
        let Some(column) = document.column(column_id) else {
            return false;
        };
        if document
            .primary_column()
            .is_some_and(|primary| primary.id == column_id)
            || column
                .raw
                .get("is_primary_column")
                .is_some_and(|v| v != &Value::Bool(false) && !v.is_null())
        {
            return false;
        }
        matches!(
            column.kind.as_str(),
            "text"
                | "rich_text"
                | "message"
                | "number"
                | "select"
                | "multi_select"
                | "date"
                | "user"
                | "attachment"
                | "checkbox"
                | "email"
                | "phone"
                | "channel"
                | "currency"
                | "rating"
                | "created_by"
                | "last_edited_by"
                | "created_time"
                | "last_edited_time"
                | "vote"
                | "assignee"
                | "due_date"
                | "completed"
                | "canvas"
                | "reference"
                | "link"
        )
    }
    /// Captured owner-only experiment policy. Unknown assignments grant nothing;
    /// an explicit off assignment preserves ordinary write-collaborator access.
    pub fn schema_policy_allows(
        document: &Document,
        authenticated_user: &str,
        schema_owner_only: Option<bool>,
    ) -> bool {
        !authenticated_user.is_empty()
            && authenticated_user.trim() == authenticated_user
            && schema_owner_only.is_some_and(|owner_only| {
                !owner_only
                    || document.raw["access"] == "owner"
                    || document.raw["user"] == authenticated_user
            })
    }
    pub fn capture(
        document: &Document,
        column_id: &str,
        authenticated_user: &str,
        schema_owner_only: Option<bool>,
        allow_lists: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<Self, ListError> {
        if Document::parse(document.raw.clone())? != *document {
            return Err(ListError("List schema representations disagree."));
        }
        eligible(
            document,
            column_id,
            authenticated_user,
            schema_owner_only,
            allow_lists,
            personal,
        )?;
        let column = document.column(column_id).unwrap();
        let mut expected_metadata = document.raw["list_metadata"].clone();
        expected_metadata["schema"]
            .as_array_mut()
            .unwrap()
            .retain(|c| c["key"] != column.key);
        let views = expected_metadata
            .get_mut("views")
            .and_then(Value::as_array_mut)
            .ok_or(ListError(
                "List views must be verified before deleting a field.",
            ))?;
        for view in views {
            let columns = view
                .get_mut("columns")
                .and_then(Value::as_array_mut)
                .ok_or(ListError("List view fields are missing or malformed."))?;
            // Unlike the stock optimistic reducer, refuse ambiguous view entries.
            let mut keys = std::collections::HashSet::new();
            for entry in columns.iter() {
                let key = entry
                    .get("key")
                    .and_then(Value::as_str)
                    .filter(|k| !k.trim().is_empty())
                    .ok_or(ListError("List view fields are missing or malformed."))?;
                if !keys.insert(key.to_owned()) {
                    return Err(ListError("List view fields have ambiguous identities."));
                }
            }
            columns.retain(|c| c["key"] != column.key);
        }
        // L34r removes the schema entry and view columns, not records, filters,
        // sorts, grouping or integrations. Keep all other raw members exactly.
        // Absence/null is preserved instead of optimistic default normalization.
        Ok(Self {
            document: document.clone(),
            authenticated_user: authenticated_user.into(),
            column_id: column_id.into(),
            column_key: column.key.clone(),
            expected_metadata,
        })
    }
    pub fn list_id(&self) -> &str {
        &self.document.id
    }
    pub fn column_id(&self) -> &str {
        &self.column_id
    }
    pub fn column_key(&self) -> &str {
        &self.column_key
    }
    pub fn original(&self) -> &Document {
        &self.document
    }
    pub fn revalidate(
        &self,
        fresh: &Document,
        authenticated_user: &str,
        schema_owner_only: Option<bool>,
        allow_lists: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<(), ListError> {
        if authenticated_user != self.authenticated_user
            || Document::parse(fresh.raw.clone())? != *fresh
            || !super::cell_edit::document_matches(&self.document.raw, &fresh.raw)
        {
            return Err(ListError(
                "This List changed. Review the field before deleting it.",
            ));
        }
        eligible(
            fresh,
            &self.column_id,
            authenticated_user,
            schema_owner_only,
            allow_lists,
            personal,
        )
    }
    /// Independent observation, not proof this attempt caused deletion. A missing
    /// target alone is insufficient: every other metadata member must be retained.
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
    document: &Document,
    column_id: &str,
    authenticated_user: &str,
    schema_owner_only: Option<bool>,
    allow_lists: Option<&str>,
    personal: Option<&PersonalTaskLists>,
) -> Result<(), ListError> {
    if let Some(reason) = document.read_only_reason(allow_lists) {
        return Err(ListError(reason));
    }
    if !ColumnDelete::schema_policy_allows(document, authenticated_user, schema_owner_only) {
        return Err(ListError(
            "Field deletion permissions are unavailable or require the List owner.",
        ));
    }
    if personal.is_none_or(|p| p.contains(&document.id)) {
        return Err(ListError("Ordinary List preferences are not verified."));
    }
    if !ColumnDelete::supports(document, column_id) {
        return Err(ListError("Deleting this field is not supported."));
    }
    Ok(())
}

/// Host-retained operation; not serializable and not independently a durable ledger.
/// Account/session/visit/read/revision/cooldown fences belong to the desktop host.
pub struct ColumnDeleteAttempt {
    intent: ColumnDelete,
    phase: ColumnDeletePhase,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnDeletePhase {
    Reviewing,
    InFlight,
    NotRequested,
    Unconfirmed,
    Observed,
}
/// A consumed, one-use request-construction capability. Not Clone.
pub struct ColumnDeletePermit {
    intent: ColumnDelete,
}
impl ColumnDeletePermit {
    pub(crate) fn into_intent(self) -> ColumnDelete {
        self.intent
    }
}
impl ColumnDeleteAttempt {
    pub fn new(intent: ColumnDelete) -> Self {
        Self {
            intent,
            phase: ColumnDeletePhase::Reviewing,
        }
    }
    pub fn phase(&self) -> ColumnDeletePhase {
        self.phase
    }
    pub fn intent(&self) -> &ColumnDelete {
        &self.intent
    }
    pub fn admit(
        &mut self,
        fresh: &Document,
        authenticated_user: &str,
        schema_owner_only: Option<bool>,
        allow_lists: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<ColumnDeletePermit, ListError> {
        if self.phase != ColumnDeletePhase::Reviewing {
            return Err(ListError(
                "This field deletion attempt has already been admitted.",
            ));
        }
        self.intent.revalidate(
            fresh,
            authenticated_user,
            schema_owner_only,
            allow_lists,
            personal,
        )?;
        self.phase = ColumnDeletePhase::InFlight;
        Ok(ColumnDeletePermit {
            intent: self.intent.clone(),
        })
    }
    /// The host may pass true only for typed pre-dispatch NotRequested evidence.
    /// Every received ACK, API denial, timeout or post-dispatch failure stays
    /// unconfirmed until a separate current metadata read observes the deletion.
    pub fn finish(&mut self, not_requested: bool) -> bool {
        if self.phase != ColumnDeletePhase::InFlight {
            return false;
        }
        self.phase = if not_requested {
            ColumnDeletePhase::NotRequested
        } else {
            ColumnDeletePhase::Unconfirmed
        };
        true
    }
    pub fn observe(&mut self, fresh: &Document) -> bool {
        if self.phase != ColumnDeletePhase::Unconfirmed || !self.intent.observes_desired(fresh) {
            return false;
        }
        self.phase = ColumnDeletePhase::Observed;
        true
    }
}

#[cfg(test)]
mod tests;
