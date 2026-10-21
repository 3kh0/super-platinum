//! One ordinary, own, rich List reply edit. HTTP provenance is retained; this
//! is neither the socket projection nor a general message editing framework.
//! Host code owns current account/visit, observed visibility and policy leases.
use super::{ListError, Message, Page, Target, identity, item_root, parse_message, timestamp};
use crate::canvas::comments::content;
use serde::{Deserialize, Serialize};
use serde_json::Value;

mod policy;
mod receipt;
mod seed;
pub use policy::OwnEditPolicy;
pub use receipt::{OwnEditAccepted, OwnEditAttempt, OwnEditPhase};
pub use seed::OwnEditSeed;

/// Sendable authority is constructed from current observations, never restored.
pub struct OwnEditIntent {
    record: OwnEditRecord,
}

/// Durable inert evidence. Validation does not grant permission or recreate an
/// intent. A matching later read proves content, not which request produced it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnEditRecord {
    version: u8,
    seed: OwnEditSeed,
    blocks: Vec<Value>,
    text: String,
}

impl OwnEditIntent {
    pub fn capture(
        seed: &OwnEditSeed,
        known: &Message,
        policy: &OwnEditPolicy,
        edited: Vec<Value>,
        now_ms: u64,
    ) -> Result<Self, ListError> {
        seed.validate()?;
        policy.admit(
            seed.workspace(),
            seed.user(),
            seed.target(),
            seed.ts(),
            now_ms,
        )?;
        if !seed.current(known) {
            return Err(ListError("The original comment changed while editing."));
        }
        let text = content::fallback(&edited)
            .map_err(|_| ListError("The edited rich content is unsupported."))?;
        // An empty edit is not a delete. Deletion is a separate explicit action.
        if text.trim().is_empty() {
            return Err(ListError("An empty comment cannot be saved."));
        }
        let mut blocks = edited;
        blocks.extend_from_slice(seed.preserved());
        let record = OwnEditRecord {
            version: 1,
            seed: seed.clone(),
            blocks,
            text,
        };
        record.validate()?;
        Ok(Self { record })
    }
    pub fn record(&self) -> &OwnEditRecord {
        &self.record
    }
}

impl OwnEditRecord {
    pub fn validate(&self) -> Result<(), ListError> {
        self.seed.validate()?;
        let rich_len = self
            .blocks
            .len()
            .checked_sub(self.seed.preserved().len())
            .ok_or(ListError("The edited content is malformed."))?;
        let rich = &self.blocks[..rich_len];
        if self.version != 1
            || rich.is_empty()
            || self.blocks[rich_len..] != *self.seed.preserved()
            || content::fallback(rich).ok().as_deref() != Some(self.text.as_str())
            || self.text.trim().is_empty()
            || self.blocks.len() > 50
            || serde_json::to_vec(&self.blocks)
                .map_err(|_| ListError("The edited content is malformed."))?
                .len()
                > content::MAX_BLOCK_BYTES
        {
            return Err(ListError("The edited content is malformed."));
        }
        Ok(())
    }
    pub fn seed(&self) -> &OwnEditSeed {
        &self.seed
    }
    pub fn workspace(&self) -> &str {
        self.seed.workspace()
    }
    pub fn user(&self) -> &str {
        self.seed.user()
    }
    pub fn target(&self) -> &Target {
        self.seed.target()
    }
    pub fn ts(&self) -> &str {
        self.seed.ts()
    }
    pub fn blocks(&self) -> &[Value] {
        &self.blocks
    }
    pub fn text(&self) -> &str {
        &self.text
    }
}

#[cfg(test)]
mod tests;
