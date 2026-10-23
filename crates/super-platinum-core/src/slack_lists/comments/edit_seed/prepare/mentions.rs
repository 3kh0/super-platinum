//! Exact gcvA member/channel candidates reachable through rNCs. Its search
//! callback returns [] for emoji, so those generic gcvA candidates do not ask
//! the host for a lookup. This plans observations; it does not resolve names,
//! choose a member or issue a Slack request.
use super::*;
use document::Unit;
use regress::Regex;
use std::collections::BTreeSet;
use std::sync::{Arc, OnceLock};
mod resolve;
pub use resolve::{
    MentionChoice, MentionNames, MentionNeed, MentionObservations, MentionSearch, MentionStep,
};
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MentionKind {
    Member,
    Channel,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MentionCandidate {
    pub kind: MentionKind,
    pub query: String,
    pub label: String,
    pub text: Arc<str>,
    pub global_index_utf16: usize,
    pub local_index_utf16: usize,
    pub match_length_utf16: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MentionLookup {
    pub kind: MentionKind,
    pub query: String,
}
#[derive(Clone, Debug)]
pub struct MentionPlan {
    original: Vec<Value>,
    channel_tagging: bool,
    emoji_displayed_as_text: bool,
    candidates: Vec<MentionCandidate>,
    lookups: Vec<MentionLookup>,
}
impl MentionPlan {
    pub fn capture(
        ops: &[Value],
        use_channel_tagging: bool,
        is_emoji_displayed_as_text: bool,
    ) -> Result<Self> {
        let document = Document::capture(ops)?;
        let patterns = Patterns::get()?;
        let matcher = if use_channel_tagging {
            &patterns.channel
        } else {
            &patterns.member
        };
        let mut candidates = Vec::new();
        let mut offset = 0;
        for (_, mut line, newline) in document.lines() {
            line.0.push(Unit::new(Insert::Unit(10), newline.clone()));
            for op in line.ops()? {
                let length = op["insert"]
                    .as_str()
                    .map_or(1, |s| s.encode_utf16().count());
                let attributes = op.get("attributes");
                let skipped = truthy(newline.get("code-block"))
                    || truthy(attributes.and_then(|a| a.get("code")))
                    || truthy(attributes.and_then(|a| a.get("link")))
                    || ["slackmention", "slackslug", "slacksearchfilter"]
                        .iter()
                        .any(|key| truthy(attributes.and_then(|a| a.get(*key))));
                if !skipped && let Some(text) = op["insert"].as_str() {
                    let units: Vec<_> = text.encode_utf16().collect();
                    let shared_text: Arc<str> = Arc::from(text);
                    for found in matcher.find_from_ucs2(&units, 0) {
                        let group = found.group(1).ok_or(SeedUnavailable::Patterns)?;
                        let raw_label = &units[group];
                        let matched = &units[found.range.clone()];
                        let mut local_index = found.range.start;
                        // Source adjusts the whole-match start independently
                        // for each prefix, preserving its punctuation behavior.
                        for prefix in [64, 0xff20].into_iter().chain(
                            use_channel_tagging
                                .then_some([35, 0xff03])
                                .into_iter()
                                .flatten(),
                        ) {
                            if let Some(index) =
                                matched.iter().position(|c| *c == prefix).filter(|i| *i > 0)
                            {
                                local_index += index;
                            }
                        }
                        let trim_at = patterns
                            .trim
                            .find_from_ucs2(raw_label, 0)
                            .next()
                            .map_or(raw_label.len(), |m| m.range.start);
                        let label = &raw_label[..trim_at];
                        let prefixed = label.first().is_some_and(|c| {
                            matches!(c, 64 | 0xff20)
                                || use_channel_tagging && matches!(c, 35 | 0xff03)
                        });
                        let query = &label[usize::from(prefixed)..];
                        if query.is_empty() {
                            continue;
                        }
                        let kind = if use_channel_tagging
                            && raw_label.first().is_some_and(|c| matches!(c, 35 | 0xff03))
                        {
                            MentionKind::Channel
                        } else {
                            MentionKind::Member
                        };
                        if candidates.len() >= 32_768 {
                            return Err(SeedUnavailable::Limit);
                        }
                        candidates.push(MentionCandidate {
                            kind,
                            query: String::from_utf16(query)
                                .map_err(|_| SeedUnavailable::InvalidUnicode)?,
                            label: String::from_utf16(label)
                                .map_err(|_| SeedUnavailable::InvalidUnicode)?,
                            text: Arc::clone(&shared_text),
                            global_index_utf16: offset + local_index,
                            local_index_utf16: local_index,
                            match_length_utf16: raw_label.len(),
                        });
                    }
                }
                offset += length;
            }
        }
        let mut lookups = Vec::new();
        let mut seen = BTreeSet::new();
        for candidate in &candidates {
            let lookup = MentionLookup {
                kind: candidate.kind,
                query: candidate.query.clone(),
            };
            if seen.insert((lookup.kind, lookup.query.clone())) {
                lookups.push(lookup);
            }
        }
        Ok(Self {
            original: ops.to_vec(),
            channel_tagging: use_channel_tagging,
            emoji_displayed_as_text: is_emoji_displayed_as_text,
            candidates,
            lookups,
        })
    }
    pub fn original_ops(&self) -> &[Value] {
        &self.original
    }
    pub fn use_channel_tagging(&self) -> bool {
        self.channel_tagging
    }
    pub fn is_emoji_displayed_as_text(&self) -> bool {
        self.emoji_displayed_as_text
    }
    pub fn candidates(&self) -> &[MentionCandidate] {
        &self.candidates
    }
    /// Unique exact queries in first occurrence order. Every positive lookup
    /// must be observed in one unchanged workspace state before sharing it
    /// between duplicate candidates; this is not a cache-validity assertion.
    pub fn lookups(&self) -> &[MentionLookup] {
        &self.lookups
    }
    pub fn matches_input(
        &self,
        ops: &[Value],
        use_channel_tagging: bool,
        is_emoji_displayed_as_text: bool,
    ) -> bool {
        self.original == ops
            && self.channel_tagging == use_channel_tagging
            && self.emoji_displayed_as_text == is_emoji_displayed_as_text
    }
}

struct Patterns {
    member: Regex,
    channel: Regex,
    trim: Regex,
}
impl Patterns {
    fn get() -> Result<&'static Self> {
        static VALUE: OnceLock<Result<Patterns>> = OnceLock::new();
        VALUE
            .get_or_init(|| {
                let data: Value = serde_json::from_str(include_str!("mentions/patterns.json"))
                    .map_err(|_| SeedUnavailable::Patterns)?;
                let pattern = |name: &str| -> Result<Regex> {
                    Regex::with_flags(
                        data[name]["source"]
                            .as_str()
                            .ok_or(SeedUnavailable::Patterns)?,
                        data[name]["flags"]
                            .as_str()
                            .ok_or(SeedUnavailable::Patterns)?,
                    )
                    .map_err(|_| SeedUnavailable::Patterns)
                };
                Ok(Self {
                    member: pattern("member")?,
                    channel: pattern("channel")?,
                    trim: pattern("trim")?,
                })
            })
            .as_ref()
            .map_err(Clone::clone)
    }
}
