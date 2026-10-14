//! Bounded fixture-only ownership ledger. Paths are derived from Artifact kinds;
//! untrusted serialized data can never choose registry keys or shortcut paths.
use serde::{Deserialize, Serialize};

pub(super) const MAX_LEDGER: usize = 256 * 1024;
pub(super) const VERSION: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Process {
    pub pid: u32,
    pub created: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Executable {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Kind {
    Shortcut,
    Clsid,
    LocalServer,
    AppId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RegistryValue {
    pub name: String,
    pub kind: u32,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Contents {
    File(Vec<u8>),
    Registry(Vec<RegistryValue>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Artifact {
    pub kind: Kind,
    pub expected: Contents,
    /// Durable proof that this run exclusively created the target. An intent
    /// alone never permits removing a present file or registry key.
    pub created: bool,
    pub complete: bool,
    pub removed: bool,
}

impl Artifact {
    /// Interrupted writes are recoverable only when every observed byte/value
    /// was part of the recorded intended write. Completed writes require exact
    /// equality. Additional subkeys are separately rejected by the OS adapter.
    pub fn unchanged(&self, actual: &Contents) -> bool {
        if !self.created || self.removed {
            return false;
        }
        if self.complete {
            return actual == &self.expected;
        }
        match (&self.expected, actual) {
            (Contents::File(expected), Contents::File(actual)) => expected.starts_with(actual),
            (Contents::Registry(expected), Contents::Registry(actual)) => {
                actual.iter().all(|value| expected.contains(value))
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Preview {
    pub tag: String,
    pub group: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Phase {
    Acquiring,
    Registered,
    Reserved,
    Submitted,
    Ready,
    Cleaning,
    Cleaned,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Ledger {
    pub version: u8,
    pub nonce: String,
    pub executable: Executable,
    pub owner: Process,
    pub child: Option<Process>,
    pub created_ms: u64,
    pub phase: Phase,
    pub artifacts: Vec<Artifact>,
    pub preview: Option<Preview>,
    pub preview_removed: bool,
}

pub(super) fn valid_nonce(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

impl Ledger {
    pub fn parse(bytes: &[u8], nonce: &str, executable: &Executable) -> Result<Self, String> {
        if bytes.len() > MAX_LEDGER || !valid_nonce(nonce) {
            return Err("Invalid cold fixture ledger size or nonce".into());
        }
        let ledger: Self =
            serde_json::from_slice(bytes).map_err(|_| "Invalid cold fixture ledger")?;
        if ledger.version != VERSION
            || ledger.nonce != nonce
            || ledger.executable != *executable
            || ledger.owner.pid == 0
            || ledger.owner.created == 0
            || !valid_nonce(&ledger.nonce)
            || ledger.executable.sha256.len() != 64
            || !ledger
                .executable
                .sha256
                .bytes()
                .all(|c| c.is_ascii_hexdigit())
            || ledger
                .child
                .as_ref()
                .is_some_and(|child| child.pid == 0 || child.created == 0 || child == &ledger.owner)
            || ledger.artifacts.len() > 4
        {
            return Err("Cold fixture identity does not match this executable/run".into());
        }
        let mut seen = Vec::new();
        for artifact in &ledger.artifacts {
            if seen.contains(&artifact.kind) {
                return Err("Duplicate cold artifact".into());
            }
            seen.push(artifact.kind);
            match (&artifact.kind, &artifact.expected) {
                (Kind::Shortcut, Contents::File(bytes))
                    if !bytes.is_empty() && bytes.len() <= 64 * 1024 => {}
                (Kind::Clsid | Kind::LocalServer | Kind::AppId, Contents::Registry(values))
                    if values.len() <= 4 =>
                {
                    if values.iter().any(|value| {
                        value.name.len() > 64 || value.bytes.len() > 32768 || value.kind != 1
                    }) {
                        return Err("Invalid cold registry snapshot".into());
                    }
                    for (index, value) in values.iter().enumerate() {
                        if values[..index].iter().any(|other| other.name == value.name) {
                            return Err("Duplicate cold registry value".into());
                        }
                    }
                }
                _ => return Err("Invalid cold artifact contents".into()),
            }
        }
        ledger.validate_state()?;
        Ok(ledger)
    }

    fn validate_state(&self) -> Result<(), String> {
        let invalid = || "Inconsistent cold fixture ownership state".to_string();
        let order = [Kind::Shortcut, Kind::Clsid, Kind::LocalServer, Kind::AppId];
        if !valid_nonce(&self.nonce)
            || self.created_ms == 0
            || self.owner.pid == 0
            || self.owner.created == 0
            || self
                .child
                .as_ref()
                .is_some_and(|child| child.pid == 0 || child.created <= self.owner.created)
            || self.artifacts.len() > order.len()
        {
            return Err(invalid());
        }
        // Registration is serial. Only its final planned artifact may have an
        // interrupted write or a recorded create collision. Cleanup preserves
        // those creation/completion facts and removes in reverse order.
        let mut removed_seen = false;
        for (index, artifact) in self.artifacts.iter().enumerate() {
            if artifact.kind != order[index]
                || (artifact.complete && !artifact.created)
                || (index + 1 < self.artifacts.len() && !artifact.complete)
                || (removed_seen && !artifact.removed)
            {
                return Err(invalid());
            }
            removed_seen |= artifact.removed;
        }
        let full = self.artifacts.len() == order.len()
            && self
                .artifacts
                .iter()
                .all(|artifact| artifact.created && artifact.complete);
        if let Some(preview) = &self.preview {
            if preview.tag != self.nonce[..16] || preview.group != self.nonce[16..] || !full {
                return Err(invalid());
            }
        }
        if self.child.is_some()
            && (!matches!(self.phase, Phase::Ready | Phase::Cleaning | Phase::Cleaned)
                || self.preview.is_none()
                || !full)
        {
            return Err(invalid());
        }
        match self.phase {
            Phase::Acquiring => {
                if self.preview.is_some()
                    || self.preview_removed
                    || self.child.is_some()
                    || self
                        .artifacts
                        .iter()
                        .any(|artifact| artifact.removed && artifact.created)
                {
                    return Err(invalid());
                }
            }
            Phase::Registered | Phase::Reserved | Phase::Submitted | Phase::Ready => {
                if !full
                    || removed_seen
                    || self.preview_removed
                    || (self.preview.is_some() != (self.phase != Phase::Registered))
                {
                    return Err(invalid());
                }
            }
            Phase::Cleaning | Phase::Cleaned => {
                if self
                    .artifacts
                    .iter()
                    .any(|artifact| artifact.created && artifact.removed)
                    && !self.preview_removed
                {
                    return Err(invalid());
                }
                if self.phase == Phase::Cleaned
                    && (!self.preview_removed
                        || self.artifacts.iter().any(|artifact| !artifact.removed))
                {
                    return Err(invalid());
                }
            }
        }
        Ok(())
    }

    pub fn reserve_preview(&mut self, tag: String, group: String) -> Result<(), String> {
        self.validate_state()?;
        if self.phase != Phase::Registered || self.preview.is_some() {
            return Err("Cold fixture preview was already reserved; no replay allowed".into());
        }
        if tag != self.nonce[..16] || group != self.nonce[16..] {
            return Err("Preview identity does not belong to this cold run".into());
        }
        self.preview = Some(Preview { tag, group });
        self.phase = Phase::Reserved;
        Ok(())
    }

    pub fn claim_child(&mut self, child: Process, owner_alive: bool) -> Result<(), String> {
        self.validate_state()?;
        if owner_alive
            || child.pid == 0
            || child.created <= self.owner.created
            || self.child.is_some()
            || self.phase != Phase::Ready
            || self.preview.is_none()
        {
            return Err("Cold handoff is not available for a new child".into());
        }
        self.child = Some(child);
        Ok(())
    }

    pub fn begin_cleanup(&mut self) -> Result<(), String> {
        self.validate_state()?;
        // Completed recovery may no longer have a lease. Keep that proof intact
        // if its history/registration revalidation or receipt write fails again.
        if self.phase != Phase::Cleaned {
            self.phase = Phase::Cleaning;
        }
        Ok(())
    }

    pub fn cleanup_allowed(&self, caller: &Process, owner_alive: bool, child_alive: bool) -> bool {
        self.validate_state().is_ok()
            && caller.pid != 0
            && caller.created != 0
            && (!owner_alive || caller == &self.owner)
            && (!child_alive || self.child.as_ref() == Some(caller))
    }
}

#[cfg(test)]
mod tests;
