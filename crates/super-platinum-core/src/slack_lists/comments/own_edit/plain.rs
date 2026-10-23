//! Explicit plain editor custody. Delta operations are neither rich blocks nor
//! restored send authority. Only a current ordered preflight can create intent.
use super::*;
use crate::slack_lists::comments::{
    edit_seed::{
        mode::SeedPlan,
        plain::PlainSeed,
        prepare::{EncodePlan, EncodingObservations, EncodingStep, PreparedPlain},
    },
    ordered::{MessageWitness, ResponseWitness},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlainDelta {
    version: u8,
    ops: Vec<Value>,
}
impl PlainDelta {
    pub fn capture(ops: Vec<Value>) -> Result<Self, ListError> {
        let value = Self { version: 1, ops };
        value.validate()?;
        Ok(value)
    }
    pub fn ops(&self) -> &[Value] {
        &self.ops
    }
    pub fn validate(&self) -> Result<(), ListError> {
        if self.version != 1
            || serde_json::to_vec(&self.ops).map_err(|_| invalid())?.len()
                > content::MAX_BLOCK_BYTES
        {
            return Err(invalid());
        }
        // Capture validates insert-only shape, UTF-16 and allocation budgets.
        // No encoding runs here; the headers argument is unused by capture.
        EncodePlan::capture(&self.ops, EditMode::PlainText, false).map_err(|_| invalid())?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PlainSource {
    version: u8,
    ordered_response: String,
    initial: PlainDelta,
}
impl PlainSource {
    fn response(&self, seed: &OwnEditSeed) -> Result<ResponseWitness, ListError> {
        ResponseWitness::decode(self.ordered_response.as_bytes(), seed.http_raw())
    }
    pub(super) fn validate(&self, seed: &OwnEditSeed) -> Result<(), ListError> {
        if self.version != 1 {
            return Err(invalid());
        }
        self.initial.validate()?;
        let response = self.response(seed)?;
        let original = message(&response, seed.ts())?;
        let plan = SeedPlan::capture(original.raw_message(), EditMode::PlainText)
            .map_err(|_| invalid())?;
        if original.raw_message() != seed.original_raw()
            || plan.preserved_blocks() != seed.preserved()
        {
            return Err(invalid());
        }
        Ok(())
    }
}

impl OwnEditSeed {
    /// Captures the actual initializer and Page's original ordered witness.
    /// Restored initial ops remain inert editor content, not fresh observations.
    #[allow(clippy::too_many_arguments)]
    pub fn capture_plain(
        workspace: &str,
        user: &str,
        page: &Page,
        known: &Message,
        initialized: &PlainSeed,
        policy: &OwnEditPolicy,
        now_ms: u64,
    ) -> Result<Self, ListError> {
        let ts = known
            .ts
            .as_deref()
            .filter(|ts| timestamp(ts))
            .ok_or_else(invalid)?;
        policy.admit_mode(
            workspace,
            user,
            page.target(),
            ts,
            now_ms,
            EditMode::PlainText,
        )?;
        seed::root_available(page)?;
        let current = page
            .messages()
            .iter()
            .find(|row| row.ts.as_deref() == Some(ts))
            .ok_or_else(invalid)?;
        if !Self::same_body(page.target(), current, known) {
            return Err(invalid());
        }
        let response = page.ordered_response().ok_or_else(invalid)?;
        let original = message(response, ts)?;
        if !initialized.source_current(&original) {
            return Err(invalid());
        }
        let seed = Self {
            workspace: workspace.into(),
            user: user.into(),
            target: page.target().clone(),
            ts: ts.into(),
            http: page.raw().clone(),
            editable: Vec::new(),
            preserved: initialized.preserved_blocks().to_vec(),
            plain: Some(PlainSource {
                version: 1,
                ordered_response: response.ordered_json()?,
                initial: PlainDelta::capture(initialized.delta_ops().to_vec())?,
            }),
        };
        seed.validate()?;
        if !seed.current(known) {
            return Err(invalid());
        }
        Ok(seed)
    }
    pub fn mode(&self) -> EditMode {
        if self.plain.is_some() {
            EditMode::PlainText
        } else {
            EditMode::Rich
        }
    }
    pub fn plain_delta(&self) -> Option<&PlainDelta> {
        self.plain.as_ref().map(|source| &source.initial)
    }
    /// Restored ops are not proof of their original initialization. Freshly
    /// acquired reached observations must reproduce the same initial stream.
    pub fn revalidate_plain_initialization(
        &self,
        initialized: &PlainSeed,
    ) -> Result<(), ListError> {
        self.validate()?;
        let original = self.ordered_message()?.ok_or_else(invalid)?;
        let initial = self.plain_delta().ok_or_else(invalid)?;
        if !initialized.source_current(&original)
            || initial.ops() != initialized.delta_ops()
            || self.preserved() != initialized.preserved_blocks()
        {
            return Err(invalid());
        }
        Ok(())
    }
    /// Original-only inert witness; fresh authority still requires revalidate.
    pub fn ordered_message(&self) -> Result<Option<MessageWitness>, ListError> {
        self.plain
            .as_ref()
            .map(|source| {
                let response = source.response(self)?;
                message(&response, self.ts())
            })
            .transpose()
    }
}

impl OwnEditIntent {
    /// Separate Save path: exact native Delta, completed preparation, freshly
    /// observed encoding dependencies, and current ordered HTTP/policy preflight.
    /// A missing encoding observation is unavailable, never empty output.
    #[allow(clippy::too_many_arguments)]
    pub fn capture_plain(
        seed: &OwnEditSeed,
        initialized: &PlainSeed,
        page: &Page,
        known: &Message,
        policy: &OwnEditPolicy,
        edited: &PlainDelta,
        prepared: &PreparedPlain,
        headers: bool,
        observations: &EncodingObservations,
        now_ms: u64,
    ) -> Result<Self, ListError> {
        if seed.mode() != EditMode::PlainText {
            return Err(invalid());
        }
        seed.revalidate_plain_initialization(initialized)?;
        seed.revalidate(page, known, policy, now_ms)?;
        edited.validate()?;
        if edited.ops() != prepared.original_ops() {
            return Err(invalid());
        }
        let EncodingStep::Complete(mut blocks) = prepared
            .encoding(headers)
            .and_then(|plan| plan.advance(observations))
            .map_err(|_| invalid())?
        else {
            return Err(ListError("Plain encoding observations are unavailable."));
        };
        let text = content::fallback(&blocks).map_err(|_| invalid())?;
        if text.trim().is_empty() {
            return Err(ListError("An empty comment cannot be saved."));
        }
        blocks.extend_from_slice(seed.preserved());
        let record = OwnEditRecord {
            version: 2,
            seed: seed.clone(),
            blocks,
            text,
            plain_delta: Some(edited.clone()),
        };
        record.validate()?;
        Ok(Self { record })
    }
}
pub(super) fn ordered_message(page: &Page, ts: &str) -> Result<MessageWitness, ListError> {
    message(page.ordered_response().ok_or_else(invalid)?, ts)
}
fn message(response: &ResponseWitness, ts: &str) -> Result<MessageWitness, ListError> {
    let rows = response.response()["messages"]
        .as_array()
        .ok_or_else(invalid)?;
    let mut indices = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| row["ts"].as_str() == Some(ts));
    let (index, _) = indices.next().ok_or_else(invalid)?;
    if indices.next().is_some() {
        return Err(invalid());
    }
    response.message(index)
}
fn invalid() -> ListError {
    ListError("The plain comment content or original receipt is unavailable.")
}

#[cfg(test)]
mod tests;
