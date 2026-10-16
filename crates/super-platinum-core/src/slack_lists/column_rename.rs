//! Bounded ordinary Text/precision-only Number rename, not type conversion or cell editing.
//! Primary source: post-hydration-schema/source-excerpts.json (TC0j/ColumnEditor).
use super::{Document, ListError, PersonalTaskLists};
use serde_json::{Value, json};

/// Immutable reviewed schema. This is content evidence, not account/dispatch authority.
#[derive(Clone, Debug, PartialEq)]
pub struct ColumnRename {
    document: Document,
    column_id: String,
    column_key: String,
    desired: String,
    expected_metadata: Value,
}

impl ColumnRename {
    /// Presentation support only; capture/admit still reparse and check List policy.
    pub fn supports(column: &super::Column) -> bool {
        if column.kind == "text" {
            return column.raw.get("options") == Some(&json!({"format":"text"}));
        }
        let precision = column
            .raw
            .get("options")
            .and_then(|o| o.get("precision"))
            .and_then(Value::as_u64);
        column.kind == "number"
            && precision.is_some_and(|p| {
                p <= 5 && column.raw.get("options") == Some(&json!({"precision":p}))
            })
    }
    /// Exact captured lossless options; the name-only builder must preserve them.
    pub fn unchanged_options(&self) -> &Value {
        &self.document.column(&self.column_id).unwrap().raw["options"]
    }

    pub fn capture(
        document: &Document,
        column_id: &str,
        name: &str,
        allow_lists: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<Self, ListError> {
        // Public model fields must not bypass parsing or disagree with their raw data.
        if Document::parse(document.raw.clone())? != *document {
            return Err(ListError("List schema representations disagree."));
        }
        eligible(document, column_id, allow_lists, personal)?;
        let column = document.column(column_id).unwrap();
        let desired = name.trim();
        // Local single-line input/memory bound, not a claimed Slack maximum.
        if desired.is_empty()
            || desired.encode_utf16().count() > 256
            || desired.chars().any(char::is_control)
            || desired == column.name
        {
            return Err(ListError(
                "Enter a different, nonempty field name on one line.",
            ));
        }
        let mut expected_metadata = document.raw["list_metadata"].clone();
        let schema = expected_metadata["schema"].as_array_mut().unwrap();
        let target = schema.iter_mut().find(|c| c["id"] == column_id).unwrap();
        target["name"] = json!(desired);
        Ok(Self {
            document: document.clone(),
            column_id: column_id.into(),
            column_key: column.key.clone(),
            desired: desired.into(),
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
                "This List changed. Review the field before renaming it.",
            ));
        }
        eligible(fresh, &self.column_id, allow_lists, personal)
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

fn eligible(
    document: &Document,
    column_id: &str,
    allow_lists: Option<&str>,
    personal: Option<&PersonalTaskLists>,
) -> Result<(), ListError> {
    if let Some(reason) = document.read_only_reason(allow_lists) {
        return Err(ListError(reason));
    }
    if personal.is_none_or(|p| p.contains(&document.id)) {
        return Err(ListError("Ordinary List preferences are not verified."));
    }
    let column = document
        .column(column_id)
        .ok_or(ListError("The field is unavailable."))?;
    // Stock ColumnEditor merges config defaults with stored options, then TC0j
    // transforms those options and requests default-cell backfill. Restrict the
    // first slice to a shape for which this transform is exactly lossless and
    // contains no default-value, task, notification, or typed-cell options.
    if !ColumnRename::supports(column) {
        return Err(ListError(
            "Renaming this field configuration is not supported yet.",
        ));
    }
    Ok(())
}

/// Host-retained operation; not serializable and not independently a durable ledger.
/// Account/session/visit/read/revision/cooldown fences belong to the desktop host.
pub struct ColumnRenameAttempt {
    intent: ColumnRename,
    phase: ColumnRenamePhase,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnRenamePhase {
    Reviewing,
    InFlight,
    NotRequested,
    Unconfirmed,
    Observed,
}
/// A consumed, one-use request-construction capability. Not Clone.
pub struct ColumnRenamePermit {
    intent: ColumnRename,
}
impl ColumnRenamePermit {
    pub(crate) fn into_intent(self) -> ColumnRename {
        self.intent
    }
}
impl ColumnRenameAttempt {
    pub fn new(intent: ColumnRename) -> Self {
        Self {
            intent,
            phase: ColumnRenamePhase::Reviewing,
        }
    }
    pub fn phase(&self) -> ColumnRenamePhase {
        self.phase
    }
    pub fn intent(&self) -> &ColumnRename {
        &self.intent
    }
    pub fn admit(
        &mut self,
        fresh: &Document,
        allow_lists: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<ColumnRenamePermit, ListError> {
        if self.phase != ColumnRenamePhase::Reviewing {
            return Err(ListError("This rename attempt has already been admitted."));
        }
        self.intent.revalidate(fresh, allow_lists, personal)?;
        self.phase = ColumnRenamePhase::InFlight;
        Ok(ColumnRenamePermit {
            intent: self.intent.clone(),
        })
    }
    /// The host may pass true only for typed pre-dispatch NotRequested evidence.
    /// Every received ACK, API denial, timeout or post-dispatch failure stays
    /// unconfirmed until a separate current metadata read observes the rename.
    pub fn finish(&mut self, not_requested: bool) -> bool {
        if self.phase != ColumnRenamePhase::InFlight {
            return false;
        }
        self.phase = if not_requested {
            ColumnRenamePhase::NotRequested
        } else {
            ColumnRenamePhase::Unconfirmed
        };
        true
    }
    pub fn observe(&mut self, fresh: &Document) -> bool {
        if self.phase != ColumnRenamePhase::Unconfirmed || !self.intent.observes_desired(fresh) {
            return false;
        }
        self.phase = ColumnRenamePhase::Observed;
        true
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod number_tests;
