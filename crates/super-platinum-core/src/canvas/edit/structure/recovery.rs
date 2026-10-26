//! Inert evidence for one structural dispatch. Validation may plan ephemeral
//! history patches, but never recreates executable intent, accepted receipts,
//! transport authority or permission to replay.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub mod journal;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    pub workspace: String,
    pub user: String,
    pub file: String,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
enum Action {
    Insert {
        anchor: String,
        before: bool,
        position: String,
    },
    Delete,
    EndSequence {
        members: Vec<SequenceMember>,
    },
    EndSequenceParent {
        parent_updates: Vec<String>,
        members: Vec<SequenceMember>,
    },
    EndRepeatTyping {
        child: String,
        position: String,
        reused: bool,
        second_child: String,
        second_position: String,
        second_child_updates: Vec<String>,
    },
    EndRepeat {
        child: String,
        position: String,
        reused: bool,
        second_child: String,
        second_position: String,
    },
    EmptyEnter {
        child: String,
        position: String,
        reused: bool,
    },
    EmptyTyping {
        child: String,
        position: String,
        reused: bool,
        child_updates: Vec<String>,
    },
    StartTyping {
        child: String,
        position: String,
        original_updates: Vec<String>,
    },
    End {
        child: String,
        position: String,
        offset_utf16: usize,
    },
    EndReuse {
        child: String,
        position: String,
        offset_utf16: usize,
    },
    EndTyping {
        child: String,
        position: String,
        offset_utf16: usize,
        reused: bool,
        child_updates: Vec<String>,
    },
    EndClearing {
        child: String,
        position: String,
        offset_utf16: usize,
        child_updates: Vec<String>,
    },
    Split {
        child: String,
        position: String,
        offset_utf16: usize,
        parent_draft: String,
        child_draft: String,
    },
    SplitSequence {
        child: String,
        position: String,
        offset_utf16: usize,
        parent_draft: String,
        child_draft: String,
        parent_updates: Vec<String>,
        child_updates: Vec<String>,
        second_child: String,
        second_position: String,
        second_target: super::SplitSequenceTarget,
        second_enter_draft: String,
        second_offset_utf16: usize,
        second_parent_draft: String,
        second_child_draft: String,
        final_updates: [Vec<String>; 3],
    },
    SplitParent {
        child: String,
        position: String,
        offset_utf16: usize,
        enter_draft: String,
        parent_draft: String,
        child_draft: String,
        pre_enter_updates: Vec<String>,
        parent_updates: Vec<String>,
        child_updates: Vec<String>,
    },
    SplitPreEnter {
        child: String,
        position: String,
        offset_utf16: usize,
        enter_draft: String,
        parent_draft: String,
        child_draft: String,
        pre_enter_updates: Vec<String>,
        child_updates: Vec<String>,
    },
    SplitTyping {
        child: String,
        position: String,
        offset_utf16: usize,
        parent_draft: String,
        child_draft: String,
        child_updates: Vec<String>,
    },
    Replace {
        replacement: String,
        position: String,
    },
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SequenceMember {
    id: String,
    position: String,
    reused: bool,
    updates: Vec<String>,
}

/// Credential-free, byte-preserving evidence. Fields and serialization stay
/// private; only the durable journal can decode this into an inert receipt.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    version: u8,
    scope: Scope,
    thread: String,
    document: String,
    target: String,
    session: String,
    sequence: u64,
    action: Action,
    draft: String,
    sections: Vec<Vec<u8>>,
    request: Vec<u8>,
    /// Inert historic evidence only; decoding never restores a writable certificate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    provenance: Option<provenance::Evidence>,
}
impl Checkpoint {
    pub(super) fn capture(intent: &StructureIntent, data: &RawMessage) -> Result<Self, Error> {
        let owner = &intent.inventory.owner;
        let action = match &intent.operation {
            Operation::Insert {
                anchor,
                side,
                position,
                ..
            } => Action::Insert {
                anchor: anchor.clone(),
                before: *side == Side::Before,
                position: position.clone(),
            },
            Operation::Delete { .. } => Action::Delete,
        };
        let result = Self {
            version: 1,
            scope: Scope {
                workspace: owner.workspace.clone(),
                user: owner.user.clone(),
                file: owner.file.clone(),
            },
            thread: owner.thread.clone(),
            document: owner.document.clone(),
            target: intent.operation.id().into(),
            session: intent.session.clone(),
            sequence: 1,
            action,
            draft: intent.operation.draft().rtml()?,
            sections: intent
                .inventory
                .sections
                .values()
                .map(|s| s.raw().raw().to_vec())
                .collect(),
            request: data.raw().to_vec(),
            provenance: intent.inventory.provenance_evidence(),
        };
        result.validate()?;
        Ok(result)
    }
    pub fn scope(&self) -> &Scope {
        &self.scope
    }
    pub fn target_section(&self) -> &str {
        &self.target
    }
    /// A v2 pair remains an inert whole-file lock. The existing single-target
    /// review UI must not label it as an insertion or archive only one member.
    pub fn is_replacement(&self) -> bool {
        matches!(self.action, Action::Replace { .. })
    }
    /// A split is a whole-file unresolved pair, never a single-target review.
    pub fn is_split(&self) -> bool {
        matches!(
            self.action,
            Action::Split { .. }
                | Action::SplitTyping { .. }
                | Action::SplitPreEnter { .. }
                | Action::SplitParent { .. }
                | Action::SplitSequence { .. }
        )
    }
    pub fn is_split_typing(&self) -> bool {
        if let Action::SplitSequence {
            child_updates,
            final_updates,
            ..
        } = &self.action
        {
            return !child_updates.is_empty()
                || !final_updates[1].is_empty()
                || !final_updates[2].is_empty();
        }
        matches!(self.action, Action::SplitTyping { .. })
            || matches!(&self.action, Action::SplitPreEnter { child_updates, .. } | Action::SplitParent { child_updates, .. } if !child_updates.is_empty())
    }
    pub fn is_sequence(&self) -> bool {
        matches!(
            self.action,
            Action::EndSequence { .. } | Action::EndSequenceParent { .. }
        )
    }
    pub fn is_repeat(&self) -> bool {
        matches!(
            self.action,
            Action::EndRepeat { .. } | Action::EndRepeatTyping { .. }
        )
    }
    pub fn is_repeat_typing(&self) -> bool {
        matches!(self.action, Action::EndRepeatTyping { .. })
    }
    /// End Enter retains an unchanged parent and its child, optionally with typing.
    pub fn is_end(&self) -> bool {
        matches!(
            self.action,
            Action::End { .. }
                | Action::EndSequence { .. }
                | Action::EndSequenceParent { .. }
                | Action::EndRepeat { .. }
                | Action::EndRepeatTyping { .. }
                | Action::EmptyEnter { .. }
                | Action::EmptyTyping { .. }
                | Action::EndReuse { .. }
                | Action::EndTyping { .. }
                | Action::EndClearing { .. }
        )
    }
    pub fn is_end_reuse(&self) -> bool {
        if let Action::EndSequence { members } | Action::EndSequenceParent { members, .. } =
            &self.action
        {
            return members.first().is_some_and(|m| m.reused);
        }
        matches!(
            self.action,
            Action::EndReuse { .. }
                | Action::EndRepeat { reused: true, .. }
                | Action::EndRepeatTyping { reused: true, .. }
                | Action::EmptyEnter { reused: true, .. }
                | Action::EmptyTyping { reused: true, .. }
                | Action::EndTyping { reused: true, .. }
                | Action::EndClearing { .. }
        )
    }
    pub fn is_empty_enter(&self) -> bool {
        matches!(
            self.action,
            Action::EmptyEnter { .. } | Action::EmptyTyping { .. }
        )
    }
    pub fn is_empty_typing(&self) -> bool {
        matches!(self.action, Action::EmptyTyping { .. })
    }
    pub fn is_start_typing(&self) -> bool {
        matches!(self.action, Action::StartTyping { .. })
    }
    pub fn is_delete(&self) -> bool {
        matches!(self.action, Action::Delete)
    }
    pub fn draft_rtml(&self) -> &str {
        &self.draft
    }

    fn validate(&self) -> Result<(), Error> {
        if !matches!(
            (&self.action, self.version),
            (Action::Insert { .. } | Action::Delete, 1)
                | (Action::Replace { .. }, 2)
                | (Action::Split { .. }, 3)
                | (Action::End { .. }, 4)
                | (Action::EndReuse { .. }, 5)
                | (Action::EndTyping { .. }, 6)
                | (Action::EndClearing { .. }, 7)
                | (Action::SplitTyping { .. }, 8)
                | (Action::StartTyping { .. }, 9)
                | (Action::EmptyEnter { .. }, 10)
                | (Action::EmptyTyping { .. }, 11)
                | (Action::EndRepeat { .. }, 12)
                | (Action::EndRepeatTyping { .. }, 13)
                | (Action::EndSequence { .. }, 14)
                | (Action::EndSequenceParent { .. }, 15)
                | (Action::SplitPreEnter { .. }, 16)
                | (Action::SplitParent { .. }, 17)
                | (Action::SplitSequence { .. }, 18)
        ) || self.sequence != 1
            || self.session.len() != 12
            || !self.session.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(Error::Unsupported);
        }
        for id in [
            &self.scope.workspace,
            &self.scope.user,
            &self.scope.file,
            &self.thread,
            &self.document,
            &self.target,
        ] {
            if id.is_empty() || id.len() > 256 || id.chars().any(char::is_control) {
                return Err(Error::Unsupported);
            }
        }
        if TextDraft::from_rtml(&self.draft)?.rtml()? != self.draft
            || self.sections.is_empty()
            || self.sections.len() > crate::canvas::wire::MAX_SECTIONS
        {
            return Err(Error::Unsupported);
        }
        let mut ids = BTreeSet::new();
        let mut positions = BTreeSet::new();
        let mut bytes = self.request.len();
        for raw in &self.sections {
            bytes = bytes.checked_add(raw.len()).ok_or(Error::Unsupported)?;
            if bytes > crate::canvas::wire::MAX_BYTES {
                return Err(Error::Unsupported);
            }
            let s = Section::decode(raw)?;
            if s.thread_id()? != Some(self.thread.as_str())
                || s.document_id()? != Some(self.document.as_str())
                || s.sequence()?.is_none_or(|n| n == 0 || n > i64::MAX as u64)
                || !ids.insert(s.id().to_owned())
            {
                return Err(Error::Unsupported);
            }
            let p = s.raw().string(8)?.ok_or(Error::Unsupported)?;
            position::canonical(p)?;
            if !positions.insert(p.to_owned()) {
                return Err(Error::Unsupported);
            }
        }
        let source_rows = self
            .sections
            .iter()
            .map(|b| Section::decode(b))
            .collect::<Result<Vec<_>, _>>()?;
        if let Some(proof) = &self.provenance {
            proof.validate(&self.scope, &self.thread, &self.document, &source_rows)?;
        }
        for row in &source_rows {
            if row.deleted()? == Some(true) {
                if row.crdt()?.is_some_and(|wire| wire.has(10)) {
                    super::owner_delete::tombstone(&source_rows, row)?;
                } else {
                    if super::split::child_tombstone(&source_rows, row, 0).is_err() {
                        super::split::gap_tombstone(&source_rows, row).or_else(|_| {
                            self.provenance
                                .as_ref()
                                .filter(|p| p.allows(row))
                                .map(|_| ())
                                .ok_or(Error::Unsupported)
                        })?;
                    }
                }
            }
        }
        if self.is_sequence() {
            return self.validate_end_sequence();
        }
        if self.is_repeat() {
            return self.validate_end_repeat();
        }
        if self.is_empty_enter() {
            return self.validate_empty_enter();
        }
        if self.is_start_typing() {
            return self.validate_start_typing();
        }
        if matches!(self.action, Action::EndClearing { .. }) {
            return self.validate_end_clearing();
        }
        if matches!(self.action, Action::EndTyping { .. }) {
            return self.validate_end_typing();
        }
        if self.is_split() {
            return self.validate_split();
        }
        if self.is_end_reuse() {
            return self.validate_end_reuse();
        }
        if self.is_end() {
            return self.validate_end();
        }
        if matches!(self.action, Action::Replace { .. }) {
            return self.validate_pair();
        }
        match &self.action {
            Action::Replace { .. }
            | Action::EndSequence { .. }
            | Action::EndSequenceParent { .. }
            | Action::EndRepeat { .. }
            | Action::EndRepeatTyping { .. }
            | Action::EmptyEnter { .. }
            | Action::EmptyTyping { .. }
            | Action::StartTyping { .. }
            | Action::Split { .. }
            | Action::SplitTyping { .. }
            | Action::SplitPreEnter { .. }
            | Action::SplitParent { .. }
            | Action::SplitSequence { .. }
            | Action::End { .. }
            | Action::EndReuse { .. }
            | Action::EndTyping { .. }
            | Action::EndClearing { .. } => {
                return Err(Error::Unsupported);
            }
            Action::Insert {
                anchor, position, ..
            } => {
                if !ids.contains(anchor)
                    || ids.contains(&self.target)
                    || !position::modern_id(&self.target)
                    || !position.ends_with(":temp")
                {
                    return Err(Error::Unsupported);
                }
                position::base(position)?;
            }
            Action::Delete => {
                if !ids.contains(&self.target) {
                    return Err(Error::Unsupported);
                }
            }
        }
        let data = RawMessage::decode(&self.request)?;
        let updates = data.messages(1)?;
        let [update] = updates.as_slice() else {
            return Err(Error::Unsupported);
        };
        let crdt = update.message(38)?.ok_or(Error::Unsupported)?;
        if update.string(1)? != Some(self.target.as_str())
            || update.uint(20)? != Some(1)
            || update.string(33)? != Some(format!("{}-0", self.session).as_str())
            || update.boolean(3)?.unwrap_or(false) != self.is_delete()
            || data.message(14)?.is_none()
            || crdt.string(1)? != Some(self.target.as_str())
            || crdt.string(12)?.is_none()
            || crdt.bytes(5)?.is_none()
        {
            return Err(Error::Unsupported);
        }
        self.validate_owner_delete()?;
        self.validate_child_delete()?;
        Ok(())
    }
}

mod child_delete;
mod empty_enter;
mod end;
mod end_clearing;
mod end_repeat;
mod end_reuse;
mod end_sequence;
mod end_typing;
mod owner_delete;
mod replacement;
mod split;
mod start_typing;

#[cfg(test)]
mod tests;
