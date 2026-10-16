//! One existing ordinary Number field's decimal display precision, not a type
//! conversion or cell edit. Source: list-number-precision-plan/source-excerpts.json.
use super::{Document, ListError, PersonalTaskLists};
use serde_json::{Value, json};

/// Captured content evidence. Account/visit/cooldown authority belongs to the host.
#[derive(Clone, Debug, PartialEq)]
pub struct ColumnPrecision {
    document: Document,
    column_id: String,
    column_key: String,
    desired: u8,
    expected_metadata: Value,
}
impl ColumnPrecision {
    pub fn capture(
        document: &Document,
        column_id: &str,
        desired: u8,
        allow_lists: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<Self, ListError> {
        if Document::parse(document.raw.clone())? != *document {
            return Err(ListError("List schema representations disagree."));
        }
        let current = eligible(document, column_id, allow_lists, personal)?;
        if desired > 5 || desired == current {
            return Err(ListError(
                "Choose a different number of decimals from 0 to 5.",
            ));
        }
        let column = document.column(column_id).unwrap();
        let mut expected_metadata = document.raw["list_metadata"].clone();
        let schema = expected_metadata["schema"].as_array_mut().unwrap();
        let target = schema.iter_mut().find(|c| c["id"] == column_id).unwrap();
        target["options"]["precision"] = json!(desired);
        Ok(Self {
            document: document.clone(),
            column_id: column_id.into(),
            column_key: column.key.clone(),
            desired,
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
    pub fn desired_precision(&self) -> u8 {
        self.desired
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
                "This List changed. Review the number format again.",
            ));
        }
        eligible(fresh, &self.column_id, allow_lists, personal).map(|_| ())
    }
    /// Exact full metadata on the same List incarnation, never an ACK or local
    /// patch. Top-level access/timestamps may change and must come from this read.
    /// Observing the state does not establish that this attempt caused it.
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
) -> Result<u8, ListError> {
    if let Some(reason) = document.read_only_reason(allow_lists) {
        return Err(ListError(reason));
    }
    if personal.is_none_or(|p| p.contains(&document.id)) {
        return Err(ListError("Ordinary List preferences are not verified."));
    }
    let column = document
        .column(column_id)
        .ok_or(ListError("The field is unavailable."))?;
    let current = column
        .raw
        .get("options")
        .and_then(|options| options.get("precision"))
        .and_then(Value::as_u64)
        .filter(|value| *value <= 5);
    // The stock options converter plus default-cell backfill is lossless only
    // for this deliberately bounded shape. Unknown options/defaults and other
    // Number configurations are not silently overwritten.
    let Some(current) = current else {
        return Err(ListError(
            "Changing this field's number format is not supported yet.",
        ));
    };
    if column.kind != "number" || column.raw.get("options") != Some(&json!({"precision":current})) {
        return Err(ListError(
            "Changing this field's number format is not supported yet.",
        ));
    }
    Ok(current as u8)
}

/// Process-local attempt state; not a durable dispatch ledger. The host retains
/// the original account/session/visit/read/revision and enforces no replay even
/// though PreparedRequest is cloneable. A new attempt requires a fresh review.
pub struct ColumnPrecisionAttempt {
    intent: ColumnPrecision,
    phase: ColumnPrecisionPhase,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnPrecisionPhase {
    Reviewing,
    InFlight,
    NotRequested,
    Unconfirmed,
    Observed,
}
/// Consumed request-construction permit, deliberately not Clone.
pub struct ColumnPrecisionPermit {
    intent: ColumnPrecision,
}
impl ColumnPrecisionPermit {
    pub(crate) fn into_intent(self) -> ColumnPrecision {
        self.intent
    }
}
impl ColumnPrecisionAttempt {
    pub fn new(intent: ColumnPrecision) -> Self {
        Self {
            intent,
            phase: ColumnPrecisionPhase::Reviewing,
        }
    }
    pub fn phase(&self) -> ColumnPrecisionPhase {
        self.phase
    }
    pub fn intent(&self) -> &ColumnPrecision {
        &self.intent
    }
    pub fn admit(
        &mut self,
        fresh: &Document,
        allow_lists: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<ColumnPrecisionPermit, ListError> {
        if self.phase != ColumnPrecisionPhase::Reviewing {
            return Err(ListError(
                "This number format attempt has already been admitted.",
            ));
        }
        self.intent.revalidate(fresh, allow_lists, personal)?;
        self.phase = ColumnPrecisionPhase::InFlight;
        Ok(ColumnPrecisionPermit {
            intent: self.intent.clone(),
        })
    }
    /// True is reserved for typed pre-dispatch NotRequested evidence. Every
    /// received response or post-dispatch failure requires a separate readback.
    pub fn finish(&mut self, not_requested: bool) -> bool {
        if self.phase != ColumnPrecisionPhase::InFlight {
            return false;
        }
        self.phase = if not_requested {
            ColumnPrecisionPhase::NotRequested
        } else {
            ColumnPrecisionPhase::Unconfirmed
        };
        true
    }
    pub fn observe(&mut self, fresh: &Document) -> bool {
        if self.phase != ColumnPrecisionPhase::Unconfirmed || !self.intent.observes_desired(fresh) {
            return false;
        }
        self.phase = ColumnPrecisionPhase::Observed;
        true
    }
}

#[cfg(test)]
mod tests;
