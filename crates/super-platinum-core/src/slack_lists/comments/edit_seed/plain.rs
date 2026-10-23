//! Xc/Q and W05w EDIT initialization, retaining an exact neutral Delta stream.
//! This is not Save preparation: rNCs/JfMk/k1tX must run before encoding a write.
use super::{SeedObservations, SeedUnavailable, delta, mode::*};
use crate::slack_lists::comments::ordered::MessageWitness;
use serde_json::{Value, json};
mod chunks;
mod input;
mod ops;
use ops::Delta;

#[derive(Clone, Debug, Default)]
pub struct PlainObservations {
    pub seed: SeedObservations,
    /// Positively observed xQRb output. None does not mean an empty list.
    pub highlight_words: Option<Vec<String>>,
    /// Exact original block channel token -> observed dfPN label. A known label
    /// for the same ID cannot authorize another token's tab-specific branch.
    pub channel_labels: Vec<(Value, String)>,
    /// Exact source aBXg text for an original emoji element in as_text mode.
    pub emoji_text: Vec<(Value, String)>,
    /// Exact stateful source slug result for an original semantic element.
    /// Some(None) is a positively observed source omission, not failed lookup.
    pub slugs: Vec<(Value, Option<Value>)>,
}

#[derive(Clone, Debug)]
pub struct PlainSeed {
    original: MessageWitness,
    ops: Vec<Value>,
    preserved: Vec<Value>,
}
impl PlainSeed {
    pub fn capture(
        original: &MessageWitness,
        observations: &PlainObservations,
    ) -> Result<Self, SeedUnavailable> {
        let plan = SeedPlan::capture(original.raw_message(), EditMode::PlainText)?;
        let delta = match plan.input() {
            SeedInput::Text {
                text,
                mode: TextMode::Edit,
            } => text_delta(text, observations)?,
            SeedInput::PlainBlocks(blocks) => input::blocks(original, blocks, observations)?,
            _ => return Err(SeedUnavailable::InvalidObservation),
        };
        Ok(Self {
            original: original.clone(),
            ops: delta.finish()?,
            preserved: plan.preserved_blocks().to_vec(),
        })
    }
    pub fn delta_ops(&self) -> &[Value] {
        &self.ops
    }
    pub fn preserved_blocks(&self) -> &[Value] {
        &self.preserved
    }
    pub fn source_current(&self, fresh: &MessageWitness) -> bool {
        self.original.same_seed(fresh)
    }
}

fn text_delta(text: &str, observations: &PlainObservations) -> Result<Delta, SeedUnavailable> {
    if text.is_empty() {
        return Ok(Delta::default());
    }
    observations
        .seed
        .emoji_as_text
        .ok_or(SeedUnavailable::UnknownEmojiPreference)?;
    let tokens = super::tokenizer::tokenize_edit(text)?;
    let converted = delta::convert(&tokens, &observations.seed)?;
    let mut out = Delta::default();
    for op in converted {
        match op.insert {
            delta::Insert::Text(text) => out.text(&text),
            delta::Insert::Atom(atom, label) => {
                let (id, highlights) = match atom["type"].as_str() {
                    Some("user") => (required(&atom, "user_id")?.to_owned(), true),
                    Some("channel") => (required(&atom, "channel_id")?.to_owned(), false),
                    Some("usergroup") => (required(&atom, "usergroup_id")?.to_owned(), true),
                    Some("broadcast") => (format!("BK{}", required(&atom, "range")?), true),
                    _ => return Err(SeedUnavailable::UnsupportedToken("EDIT atom".into())),
                };
                out.insert(
                    json!(label),
                    mention(&id, &label, observations, highlights)?,
                );
            }
        }
    }
    if !out.ends_newline() {
        out.text("\n");
    }
    Ok(out)
}

fn required<'a>(value: &'a Value, key: &str) -> Result<&'a str, SeedUnavailable> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or(SeedUnavailable::MalformedMessage)
}
fn mention(
    id: &str,
    label: &str,
    obs: &PlainObservations,
    use_highlights: bool,
) -> Result<Value, SeedUnavailable> {
    if id.is_empty() {
        return Err(SeedUnavailable::InvalidObservation);
    }
    let unverified = id == "UNVERIFIED";
    let words = if use_highlights {
        obs.highlight_words
            .as_deref()
            .ok_or(SeedUnavailable::MissingHighlights)?
    } else {
        &[]
    };
    let user = format!("<@{}", id.strip_prefix('@').unwrap_or(id));
    let group = format!("<!subteam^{id}");
    let highlighted = !unverified
        && (matches!(
            id,
            "BKhere"
                | "BKchannel"
                | "BKeveryone"
                | "BKgroup"
                | "BKsf:accountteam"
                | "BKsf:dealteam"
                | "BKassistant:einstein"
        ) || words.iter().any(|word| word == &user || word == &group));
    Ok(
        json!({"slackmention":{"id":id,"label":label,"mention":highlighted,"unverified":unverified}}),
    )
}

#[cfg(test)]
mod tests;
