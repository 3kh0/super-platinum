//! Staged plain rNCs preparation: syntax, conditional symbols, colon emoji,
//! observed automatic mentions, and final links. EncodePlan and EditComparison
//! retain the subsequent 8J14/IHC0 boundaries. No mode or mutation authority.
use super::SeedUnavailable;
use serde_json::{Value, json};
type Result<T> = std::result::Result<T, SeedUnavailable>;
mod blocks;
mod compare;
pub use compare::EditComparison;
mod document;
mod embeds;
mod encode;
pub use encode::{EmojiEncoding, EncodePlan, EncodingNeed, EncodingObservations, EncodingStep};
mod emoji;
pub use emoji::{
    EmojiObservations, EmojiTranslation, StandardEmoji, ToneSupport, emoji_name,
    replace_colon_emoji,
};
mod inline;
mod links;
mod mentions;
pub use mentions::{
    MentionCandidate, MentionChoice, MentionKind, MentionLookup, MentionNames, MentionNeed,
    MentionObservations, MentionPlan, MentionSearch, MentionStep,
};
mod pipeline;
pub use pipeline::{PlainPreparation, PreparationSettings, PreparationStep, PreparedPlain};
mod patterns;
mod strip;
mod symbols;
pub use symbols::add_text_presentation_selectors;
#[cfg(test)]
mod tests;
use document::{Change, Document, Insert};
pub use strip::SyntaxObservations;

#[derive(Clone, Debug)]
pub struct PlainSyntax {
    original: Vec<Value>,
    prepared: Vec<Value>,
}
impl PlainSyntax {
    pub fn capture(ops: &[Value], observations: &SyntaxObservations) -> Result<Self> {
        let mut document = Document::capture(ops)?;
        links::linkify(&mut document, false)?;
        embeds::repair(&mut document)?;
        markdown(&mut document, observations)?;
        Ok(Self {
            original: ops.to_vec(),
            prepared: document.ops()?,
        })
    }
    pub fn original_ops(&self) -> &[Value] {
        &self.original
    }
    pub fn delta_ops(&self) -> &[Value] {
        &self.prepared
    }
}

fn markdown(document: &mut Document, observations: &SyntaxObservations) -> Result<()> {
    let quote = blocks::lists(document, false)?;
    let quote = blocks::code(document, quote)?;
    inline::format(document, quote, &|text| {
        Ok(links::detect(
            text,
            crate::slack_lists::cell_edit::link::MessageLinkOptions {
                include_phone_matches_with_tel_prefix: true,
                ..Default::default()
            },
        )?
        .into_iter()
        .map(|link| inline::Link {
            text: link.text.encode_utf16().collect(),
            url: link.url,
        })
        .collect())
    })?;
    strip::formatting(document, observations)
}

fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Number(n)) => n.as_f64().is_some_and(|n| n != 0.0),
        Some(Value::Object(_) | Value::Array(_)) => true,
    }
}
fn regex_text(document: &Document) -> Result<Vec<u16>> {
    let p = patterns::Patterns::get()?;
    let literal = document.text(false, false, b' ' as u16)?;
    let replace_links = !p.test("SM/6:zI", &literal);
    document.text(replace_links, true, b'!' as u16)
}
