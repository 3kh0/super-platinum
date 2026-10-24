//! Ordered original-row reports after one middle split. The initial boundary
//! stays immutable; final content never substitutes for captured SplitInfo.
use super::*;
use automerge::{Automerge, ReadDoc};

impl SplitWitness {
    pub fn with_parent_updates(mut self, updates: &[TextDraft]) -> Result<Self, Error> {
        if self.second.is_some() {
            return Err(Error::Unsupported);
        }
        super::super::end::typing::validate_updates_from(&self.parent, updates)?;
        pre_enter::budget(&self.pre_updates, updates, &self.updates)?;
        self.parent_updates = updates.to_vec();
        Ok(self)
    }
    pub fn initial_parent_draft(&self) -> &TextDraft {
        &self.parent
    }
    pub fn parent_draft(&self) -> &TextDraft {
        self.sequence_final(0)
            .unwrap_or_else(|| self.parent_updates.last().unwrap_or(&self.parent))
    }
    pub fn parent_updates(&self) -> &[TextDraft] {
        &self.parent_updates
    }
}

pub(super) fn validate(
    seed: &RawMessage,
    original: &Section,
    live: &Automerge,
    live_bytes: &[u8],
    wire: &RawMessage,
    witness: &SplitWitness,
    session: &str,
) -> Result<(), Error> {
    let ordinary = wire.remove(10)?;
    let lineage = wire.message(10)?.ok_or(Error::Unsupported)?;
    // The caller has checked every lineage field against the initial boundary.
    // Preserve the exact ordered-report session plus its final lineage field.
    if *wire != ordinary.replace_fields(10, &[F::message(10, &lineage)?])? {
        return Err(Error::Changed);
    }
    let doc = text::crdt::validate(&ordinary, original, witness.parent_draft(), None)?;
    let heads = live.get_heads();
    let additions = doc.get_changes(&heads);
    let split = additions.first().ok_or(Error::Unsupported)?;
    if split.actor_id().to_hex_string() != format!("{session}00000000")
        || split.deps() != heads
        || live
            .get_changes(&[])
            .iter()
            .any(|c| doc.get_change_by_hash(&c.hash()).is_none())
        || doc.fork_at(&heads).map_err(|_| Error::Unsupported)?.save() != live_bytes
    {
        return Err(Error::Changed);
    }
    text::crdt::check_document_state(
        &doc.fork_at(&[split.hash()])
            .map_err(|_| Error::Unsupported)?,
        witness.initial_parent_draft(),
        false,
    )?;
    // Prove original -> prelude -> exact truncation -> every post-parent report.
    // The shared proof requires one linear actor sequence, exact live patch
    // boundaries and the full P + [final] + P session projection.
    super::super::end::typing::validate_reports(seed, &ordinary, true, &reports(witness), session)
}

pub(super) fn build(
    seed: &RawMessage,
    witness: &SplitWitness,
    session: &str,
    now: u64,
) -> Result<RawMessage, Error> {
    let reports = reports(witness);
    // One coalesced parent save: prior processed + final head + prior processed.
    // Intermediate pre-Enter heads stay in history, never become separate ACKs.
    super::super::end::typing::build_reports(seed, true, &reports, session, now)
}

pub(super) fn reports(witness: &SplitWitness) -> Vec<TextDraft> {
    let mut reports = witness.pre_updates.clone();
    reports.push(witness.parent.clone());
    reports.extend(witness.parent_updates.iter().cloned());
    reports
}
