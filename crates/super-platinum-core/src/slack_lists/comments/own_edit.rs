//! One ordinary, own, rich List reply edit. HTTP provenance is retained; this
//! is neither the socket projection nor a general message editing framework.
//! Host code owns current account/visit, observed visibility and policy leases.
use super::{ListError, Message, Page, Target, identity, item_root, parse_message, timestamp};
use crate::canvas::comments::content;
use serde::{Deserialize, Serialize};
use serde_json::Value;

mod comparison;
mod plain;
use super::edit_seed::mode::EditMode;
pub use plain::PlainDelta;
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    plain_delta: Option<PlainDelta>,
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
        if seed.mode() != EditMode::Rich {
            return Err(ListError("Rich capture cannot accept plain Delta content."));
        }
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
            plain_delta: None,
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
        let mode_valid = match (&self.plain_delta, self.seed.mode(), self.version) {
            (None, EditMode::Rich, 1) => true,
            (Some(delta), EditMode::PlainText, 2) => delta.validate().is_ok(),
            _ => false,
        };
        if !mode_valid
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
    /// Original plain editor capture, never outgoing Block Kit blocks.
    pub fn plain_delta(&self) -> Option<&PlainDelta> {
        self.plain_delta.as_ref()
    }
    pub fn text(&self) -> &str {
        &self.text
    }
}

#[cfg(test)]
mod tests;
