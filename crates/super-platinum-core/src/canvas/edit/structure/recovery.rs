//! Inert evidence for one structural dispatch. Loading never recreates an actor,
//! intent, PreparedStructureEdit, accepted receipt, or permission to replay.
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
    Replace {
        replacement: String,
        position: String,
    },
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
    pub fn is_delete(&self) -> bool {
        matches!(self.action, Action::Delete)
    }
    pub fn draft_rtml(&self) -> &str {
        &self.draft
    }

    fn validate(&self) -> Result<(), Error> {
        if !matches!(
            (&self.action, self.version),
            (Action::Insert { .. } | Action::Delete, 1) | (Action::Replace { .. }, 2)
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
                || s.deleted()? == Some(true)
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
        if matches!(self.action, Action::Replace { .. }) {
            return self.validate_pair();
        }
        match &self.action {
            Action::Replace { .. } => return Err(Error::Unsupported),
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
        Ok(())
    }
}

mod replacement;

#[cfg(test)]
mod tests;
