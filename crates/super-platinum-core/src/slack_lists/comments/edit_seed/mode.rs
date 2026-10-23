//! Retained Bg2/ mode precedence, +zOH input routing, and qIlP preservation.
//! This plan does not convert a seed or authorize an edit. In particular, plain
//! mode requires Xc/Q/W05w EDIT followed by rNCs at Save; portable mode requires
//! opaque embed custody and 8J14 supportNonRichTextBlocks. Callers must not feed
//! either plan to the existing default-rich capture and claim equivalent output.
use super::SeedUnavailable;
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditMode {
    Rich,
    PlainText,
    Portable,
}
impl EditMode {
    /// Both arguments must come from positively acquired current observations.
    /// Missing preferences or an unavailable experiment response are not false.
    pub fn from_observed(plain_text_mode: bool, portable_treatment: bool) -> Self {
        if plain_text_mode {
            Self::PlainText
        } else if portable_treatment {
            Self::Portable
        } else {
            Self::Rich
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextMode {
    Normal,
    Edit,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SeedInput {
    Text {
        text: String,
        mode: TextMode,
    },
    /// +zOH filters only rich_text, in original array order.
    RichBlocks(Vec<Value>),
    /// Xc/Q receives the entire original array and filters with E1L.OF itself.
    PlainBlocks(Vec<Value>),
    /// E1L with supportNonRichTextBlocks receives every non-context block.
    PortableBlocks(Vec<Value>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct SeedPlan {
    mode: EditMode,
    input: SeedInput,
    preserved: Vec<Value>,
}
impl SeedPlan {
    pub fn capture(message: &Value, mode: EditMode) -> Result<Self, SeedUnavailable> {
        let object = message
            .as_object()
            .ok_or(SeedUnavailable::MalformedMessage)?;
        let blocks = match object.get("blocks") {
            None | Some(Value::Null) => &[][..],
            Some(Value::Array(v)) => v.as_slice(),
            _ => return Err(SeedUnavailable::MalformedMessage),
        };
        // Structural admission is local. Unknown string types remain raw and
        // do not become an inferred rich/portable representation here.
        if blocks
            .iter()
            .any(|b| b.get("type").and_then(Value::as_str).is_none())
        {
            return Err(SeedUnavailable::MalformedMessage);
        }
        let rich: Vec<_> = blocks
            .iter()
            .filter(|b| b["type"] == "rich_text")
            .cloned()
            .collect();
        let preserved = blocks
            .iter()
            .filter(|b| match mode {
                EditMode::Rich => b["type"] != "rich_text",
                EditMode::PlainText => !editable_block(b),
                EditMode::Portable => b["type"] == "context",
            })
            .cloned()
            .collect();
        let portable: Vec<_> = blocks
            .iter()
            .filter(|b| b["type"] != "context")
            .cloned()
            .collect();
        let input = if mode == EditMode::Portable && !portable.is_empty() {
            SeedInput::PortableBlocks(portable)
        } else if mode == EditMode::PlainText
            && (!rich.is_empty() || !blocks.is_empty() && blocks.iter().all(editable_block))
        {
            SeedInput::PlainBlocks(blocks.to_vec())
        } else if !rich.is_empty() {
            SeedInput::RichBlocks(rich)
        } else {
            let text = match object.get("text") {
                None | Some(Value::Null) => String::new(),
                Some(Value::String(v)) => v.clone(),
                _ => return Err(SeedUnavailable::MalformedMessage),
            };
            SeedInput::Text {
                text,
                mode: if mode == EditMode::PlainText {
                    TextMode::Edit
                } else {
                    TextMode::Normal
                },
            }
        };
        Ok(Self {
            mode,
            input,
            preserved,
        })
    }
    pub fn mode(&self) -> EditMode {
        self.mode
    }
    pub fn input(&self) -> &SeedInput {
        &self.input
    }
    /// qIlP's append partition only. Portable opaque blocks belong to the
    /// editable stream; appending all original non-rich blocks duplicates them.
    pub fn preserved_blocks(&self) -> &[Value] {
        &self.preserved
    }
}

/// Exact E1L.OF predicate, with JSON-value JavaScript truthiness. Conversion
/// support is a separate boundary; this predicate alone grants no capability.
fn editable_block(block: &Value) -> bool {
    match block.get("type").and_then(Value::as_str) {
        Some("rich_text" | "divider" | "header") => true,
        Some("section") => {
            truthy(block.get("text"))
                && !truthy(block.get("accessory"))
                && !truthy(block.get("fields"))
        }
        _ => false,
    }
}
fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(v)) => *v,
        Some(Value::String(v)) => !v.is_empty(),
        Some(Value::Number(v)) => v.as_f64().is_some_and(|v| v != 0.0),
        Some(Value::Array(_) | Value::Object(_)) => true,
    }
}

#[cfg(test)]
mod tests;
