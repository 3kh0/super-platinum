//! Inert private input adapter beside slash_commands::arguments.
//! No caller-supplied Delta: derive from the exact existing reversible Snapshot.
//! This carries content correspondence, never native/Review/Claim authority.
use crate::drafts::{DraftKey, LocalDraft, codec, sync::Snapshot};
use serde_json::{Value, json};
use std::sync::Arc;

pub(crate) struct Paired {
    key: DraftKey,
    original: Value,
    converted: super::arguments::Converted,
}

impl Paired {
    pub(crate) fn matches_frozen(&self, frozen: &super::api::Frozen, emoji: &Arc<Value>) -> bool {
        frozen.original == self.original
            && frozen.owner.team == self.key.team
            && frozen.owner.channel == self.key.channel
            && frozen.owner.thread == self.key.thread
            && frozen.display == self.converted.display()
            && frozen.argument_blocks.as_ref() == self.converted.blocks()
            && self.converted.same_emoji_catalog(emoji)
    }

    pub(crate) fn freeze(
        &self,
        owner: super::Owner,
        revision: u64,
        original: Value,
        emoji: &Arc<Value>,
        choice: &super::catalog::Choice,
        catalog: &super::catalog::Catalog,
    ) -> Result<super::api::Frozen, super::Failure> {
        if original != self.original
            || owner.team != self.key.team
            || owner.channel != self.key.channel
            || owner.thread != self.key.thread
            || !self.converted.same_emoji_catalog(emoji)
        {
            return Err(super::Failure::Stale);
        }
        super::api::Frozen::capture(
            owner,
            revision,
            original,
            self.converted.display().into(),
            self.converted.blocks().cloned(),
            choice,
            catalog,
        )
    }
}

pub(crate) fn derive(
    key: &DraftKey,
    local: &LocalDraft,
    desired: &Snapshot,
    emoji: Arc<Value>,
) -> Result<Paired, super::Failure> {
    // Same content pairing as protected original_pair, with command broadcast
    // explicitly unavailable. Selection is retained in original, not discarded.
    if !local.attachments.is_empty()
        || local.composer.reply_broadcast
        || local.composer.text.trim().is_empty()
        || codec::decode(key, desired)
            .and_then(|p| p.snapshot_for(&local.composer))
            .as_ref()
            .ok()
            != Some(desired)
    {
        return Err(super::Failure::Unsupported);
    }
    let runs = desired.blocks[0]["elements"][0]["elements"]
        .as_array()
        .ok_or(super::Failure::Unsupported)?;
    let mut ops = Vec::with_capacity(runs.len());
    for run in runs {
        // decode above proves exact canonical text-run schema and reversible
        // style meaning. No markdown stripping, visible-label lookup or loss.
        let text = run["text"].as_str().ok_or(super::Failure::Unsupported)?;
        if text.is_empty() {
            return Err(super::Failure::Unsupported);
        }
        let mut op = json!({"insert":text});
        if let Some(style) = run.get("style") {
            op["attributes"] = style.clone();
        }
        ops.push(op);
    }
    let converted = super::arguments::convert(&Value::Array(ops), emoji)
        .map_err(|_| super::Failure::Unsupported)?;
    Ok(Paired {
        key: key.clone(),
        original: json!({"format":1,"local":local,"desired":desired}),
        converted,
    })
}

#[cfg(test)]
#[path = "paired_tests.rs"]
mod tests;
