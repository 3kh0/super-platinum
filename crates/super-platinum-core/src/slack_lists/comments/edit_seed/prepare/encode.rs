//! Pure IHC0 -> 8J14 encoding. Reached state lookups are explicit steps.
//! Does not append qIlP preserved blocks or authorize any edit/send.
use super::{Document, Insert, Result, SeedUnavailable, truthy};
use crate::slack_lists::comments::edit_seed::mode::EditMode;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
mod budget;
mod chunks;
mod elements;
mod observations;
#[cfg(test)]
mod tests;
mod trim;
use observations::Context;
pub use observations::{EmojiEncoding, EncodingNeed, EncodingObservations};

#[derive(Clone, Debug)]
pub struct EncodePlan {
    original: Vec<Value>,
    mode: EditMode,
    headers: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub enum EncodingStep {
    Need(EncodingNeed),
    Complete(Vec<Value>),
}
impl EncodePlan {
    /// headers is the positively observed rich_text_headers_and_dividers == on.
    pub fn capture(ops: &[Value], mode: EditMode, headers: bool) -> Result<Self> {
        budget::check(ops)?;
        Document::capture(ops)?;
        Ok(Self {
            original: ops.to_vec(),
            mode,
            headers,
        })
    }
    pub fn original_ops(&self) -> &[Value] {
        &self.original
    }
    pub fn matches_input(&self, ops: &[Value], mode: EditMode, headers: bool) -> bool {
        self.original == ops && self.mode == mode && self.headers == headers
    }
    pub fn advance(&self, observed: &EncodingObservations) -> Result<EncodingStep> {
        let mut context = Context {
            observed,
            need: None,
            budget: budget::Budget::default(),
        };
        let result = encode(
            &self.original,
            self.mode == EditMode::Portable,
            self.headers,
            true,
            &mut context,
        );
        match (result, context.need) {
            (Ok(blocks), None) => {
                budget::check(&blocks)?;
                Ok(EncodingStep::Complete(blocks))
            }
            (Err(_), Some(need)) => Ok(EncodingStep::Need(need)),
            (Err(error), None) => Err(error),
            _ => Err(SeedUnavailable::InvalidObservation),
        }
    }
}

fn encode(
    ops: &[Value],
    portable: bool,
    headers: bool,
    trim_start: bool,
    context: &mut Context<'_>,
) -> Result<Vec<Value>> {
    let ops = if trim_start {
        trim::start(ops)?
    } else {
        ops.to_vec()
    };
    let mut document = Document::capture(&ops)?;
    if !document.ends_newline() {
        document
            .0
            .push(super::document::Unit::new(Insert::Unit(10), Map::new()));
    }
    if !portable {
        if document.len() <= 1 {
            return Ok(vec![]);
        }
        return Ok(vec![
            json!({"type":"rich_text","elements":elements::encode(&document, context)?}),
        ]);
    }
    let mut blocks = vec![];
    let mut segment = vec![];
    let mut header = vec![];
    let mut block_ids = BTreeSet::new();
    for (_, line, attributes) in document.lines() {
        let mut line = line;
        line.0.push(super::document::Unit::new(
            Insert::Unit(10),
            attributes.clone(),
        ));
        for op in line.ops()? {
            if let Some(block) = op["insert"]
                .get("slackopaqueblock")
                .and_then(|v| v.get("block"))
                .filter(|v| v.is_object())
            {
                flush(&mut segment, &mut blocks, context)?;
                context.reserve(block, 1)?;
                let mut block = block.clone();
                if truthy(block.get("block_id")) {
                    // Slack block IDs are strings. No JSON object identity guess.
                    let id = block["block_id"]
                        .as_str()
                        .ok_or(SeedUnavailable::MalformedMessage)?;
                    if !block_ids.insert(id.to_owned()) {
                        block.as_object_mut().unwrap().remove("block_id");
                    }
                }
                blocks.push(block);
            } else if op["insert"]
                .as_object()
                .is_some_and(|v| v.contains_key("slackopaqueblock"))
            {
                // Actual 8J14 reports and drops this malformed opaque operation.
            } else if truthy(op["insert"].get("slackhorizontalrule")) && !headers {
                flush(&mut segment, &mut blocks, context)?;
                blocks.push(json!({"type":"divider"}));
            } else if truthy(attributes.get("header")) && !headers {
                if op["insert"] == "\n" {
                    flush(&mut segment, &mut blocks, context)?;
                    if let Some(block) =
                        elements::header_block(&header, attributes.get("header"), context)?
                    {
                        blocks.push(block);
                    }
                    header.clear();
                } else {
                    header.push(op);
                }
            } else {
                segment.push(op);
            }
        }
    }
    flush(&mut segment, &mut blocks, context)?;
    Ok(blocks)
}
fn flush(
    segment: &mut Vec<Value>,
    blocks: &mut Vec<Value>,
    context: &mut Context<'_>,
) -> Result<()> {
    if segment.is_empty() {
        return Ok(());
    }
    for block in encode(&std::mem::take(segment), false, false, false, context)? {
        let empty = block["elements"].as_array().is_some_and(|elements| {
            elements.iter().all(|e| {
                e["type"] == "rich_text_section"
                    && e["elements"].as_array().is_none_or(Vec::is_empty)
            })
        });
        if !empty {
            blocks.push(block);
        }
    }
    Ok(())
}
