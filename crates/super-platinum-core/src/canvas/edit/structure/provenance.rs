//! Local witnessed replacement provenance, separate from remote row syntax.
//! The serializable evidence is inert; only a sealed accepted pair can mint
//! the live certificate carried privately by a canonical snapshot.
use super::*;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Evidence {
    scope: recovery::Scope,
    thread: String,
    document: String,
    tombstones: Vec<Vec<u8>>,
    anchors: Vec<Vec<u8>>,
}

/// No Deserialize, public fields, raw constructor or conversion from Evidence.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ReplacementProvenance {
    evidence: Evidence,
}

fn scope(owner: &inventory::Owner) -> recovery::Scope {
    recovery::Scope {
        workspace: owner.workspace.clone(),
        user: owner.user.clone(),
        file: owner.file.clone(),
    }
}

impl Evidence {
    pub(super) fn basis(inventory: &Inventory) -> Result<Self, Error> {
        let prior = inventory.provenance_evidence();
        let mut result = prior.unwrap_or_else(|| Self {
            scope: scope(&inventory.owner),
            thread: inventory.owner.thread.clone(),
            document: inventory.owner.document.clone(),
            tombstones: Vec::new(),
            anchors: Vec::new(),
        });
        let mut anchors = result
            .anchors
            .iter()
            .map(|bytes| {
                let row = Section::decode(bytes)?;
                Ok((row.id().to_owned(), bytes.clone()))
            })
            .collect::<Result<BTreeMap<_, _>, Error>>()?;
        for row in inventory.sections.values() {
            if let Some(wire) = row.crdt()?.filter(|w| w.has(10)) {
                anchors.insert(row.id().to_owned(), row.raw().raw().to_vec());
                for info in wire.messages(10)? {
                    let child = inventory.get(info.string(3)?.ok_or(Error::Unsupported)?)?;
                    anchors.insert(child.id().to_owned(), child.raw().raw().to_vec());
                }
            }
        }
        result.anchors = anchors.into_values().collect();
        Ok(result)
    }

    pub(super) fn allows(&self, row: &Section) -> bool {
        self.tombstones.iter().any(|bytes| bytes == row.raw().raw())
    }

    /// Validates inert evidence against an entire historic source inventory.
    /// This never constructs ReplacementProvenance or a writable snapshot.
    pub(super) fn validate(
        &self,
        expected: &recovery::Scope,
        thread: &str,
        document: &str,
        rows: &[Section],
    ) -> Result<(), Error> {
        if &self.scope != expected
            || self.thread != thread
            || self.document != document
            || self.tombstones.len() + self.anchors.len() > crate::canvas::wire::MAX_SECTIONS
        {
            return Err(Error::Changed);
        }
        let mut current = BTreeMap::new();
        for row in rows {
            if row.thread_id()? != Some(thread)
                || row.document_id()? != Some(document)
                || current.insert(row.id(), row).is_some()
            {
                return Err(Error::Unsupported);
            }
        }
        let mut ids = std::collections::BTreeSet::new();
        let mut bytes = 0usize;
        for (tombstone, encoded) in self
            .tombstones
            .iter()
            .map(|r| (true, r))
            .chain(self.anchors.iter().map(|r| (false, r)))
        {
            bytes = bytes.checked_add(encoded.len()).ok_or(Error::Unsupported)?;
            if bytes > crate::canvas::wire::MAX_BYTES {
                return Err(Error::Unsupported);
            }
            let row = Section::decode(encoded)?;
            if !ids.insert(row.id().to_owned())
                || current.get(row.id()).copied() != Some(&row)
                || row.thread_id()? != Some(thread)
                || row.document_id()? != Some(document)
                || row.deleted()? != Some(true)
            {
                return Err(Error::Changed);
            }
            if tombstone {
                retained::independent(rows.iter(), &row)?;
                // Membership above requires the entire sealed row, including opaque
                // extensions. Inert checkpoint evidence uses the same check but
                // cannot construct a live certificate or writable snapshot.
                split::witnessed_independent_history(&row, 0)?;
            }
        }
        Ok(())
    }
}

impl ReplacementProvenance {
    pub(super) fn evidence(&self) -> &Evidence {
        &self.evidence
    }
    pub(super) fn allows(&self, row: &Section) -> bool {
        self.evidence.allows(row)
    }
    pub(super) fn validate(&self, snapshot: &CanvasCollabSnapshot) -> Result<(), Error> {
        let owner = inventory::Owner::capture(snapshot)?;
        self.evidence.validate(
            &scope(&owner),
            &owner.thread,
            &owner.document,
            &snapshot.sections,
        )
    }
}

/// Called only after AcceptedReplacement's complete paired ACK/readback proof.
pub(super) fn certify(
    basis: &Evidence,
    fresh: &CanvasCollabSnapshot,
    deleted_id: &str,
) -> Result<CanvasCollabSnapshot, Error> {
    let mut evidence = basis.clone();
    if let Some(row) = fresh.section(deleted_id) {
        evidence.tombstones.push(row.raw().raw().to_vec());
    }
    let mut result = fresh.clone();
    if evidence.tombstones.is_empty() && evidence.anchors.is_empty() {
        result.replacement_provenance = None;
    } else {
        let proof = ReplacementProvenance { evidence };
        proof.validate(fresh)?;
        result.replacement_provenance = Some(Arc::new(proof));
    }
    Inventory::capture(&result)?;
    Ok(result)
}

/// Carry only already witnessed local provenance to a newly fetched snapshot.
/// A failed carry must not be silently retried as a writable untagged read.
pub fn carry_replacement_provenance(
    previous: &CanvasCollabSnapshot,
    fresh: &CanvasCollabSnapshot,
) -> Result<CanvasCollabSnapshot, Error> {
    let Some(proof) = &previous.replacement_provenance else {
        return Ok(fresh.clone());
    };
    proof.validate(previous)?;
    proof.validate(fresh)?;
    if fresh
        .replacement_provenance
        .as_ref()
        .is_some_and(|p| p != proof)
    {
        return Err(Error::Changed);
    }
    let mut result = fresh.clone();
    result.replacement_provenance = Some(proof.clone());
    // The fresh read may contain a new paired deletion awaiting certification.
    // Carry only prior evidence; this is not full writable-inventory admission.
    // Every intent still captures Inventory, and certify_readback checks the
    // complete new pair before it admits the additional tombstone.
    Ok(result)
}

/// Storage supplies only a record issued by a confirmed pair. Deserializing inert
/// checkpoint evidence never calls this. Every restore rechecks the fresh rows.
pub(super) fn restore(
    evidence: Option<&Evidence>,
    fresh: &CanvasCollabSnapshot,
) -> Result<CanvasCollabSnapshot, Error> {
    let proof = evidence
        .cloned()
        .map(|evidence| Arc::new(ReplacementProvenance { evidence }));
    if fresh
        .replacement_provenance
        .as_ref()
        .is_some_and(|p| Some(p) != proof.as_ref())
    {
        return Err(Error::Changed);
    }
    let mut result = fresh.clone();
    result.replacement_provenance = proof;
    Inventory::capture(&result)?;
    Ok(result)
}
