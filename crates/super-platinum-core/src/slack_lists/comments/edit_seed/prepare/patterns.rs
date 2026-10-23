use super::*;
use regress::{Match, Regex};
use std::{collections::BTreeMap, sync::OnceLock};

struct Pattern {
    regex: Regex,
    unicode: bool,
}
pub(super) struct Patterns(BTreeMap<String, Pattern>);
impl Patterns {
    pub fn get() -> Result<&'static Self> {
        static PATTERNS: OnceLock<Result<Patterns>> = OnceLock::new();
        PATTERNS
            .get_or_init(|| {
                let data: Value = serde_json::from_str(include_str!("patterns.json"))
                    .map_err(|_| SeedUnavailable::Patterns)?;
                let mut patterns = BTreeMap::new();
                for (key, value) in data.as_object().ok_or(SeedUnavailable::Patterns)? {
                    let source = value["source"].as_str().ok_or(SeedUnavailable::Patterns)?;
                    let flags = value["flags"].as_str().ok_or(SeedUnavailable::Patterns)?;
                    patterns.insert(
                        key.clone(),
                        Pattern {
                            regex: Regex::with_flags(source, flags)
                                .map_err(|_| SeedUnavailable::Patterns)?,
                            unicode: flags.contains('u'),
                        },
                    );
                }
                Ok(Self(patterns))
            })
            .as_ref()
            .map_err(Clone::clone)
    }
    pub fn all(&self, key: &str, text: &[u16]) -> Vec<Match> {
        let p = &self.0[key];
        if p.unicode {
            p.regex.find_from_utf16(text, 0).collect()
        } else {
            p.regex.find_from_ucs2(text, 0).collect()
        }
    }
    pub fn first(&self, key: &str, text: &[u16]) -> Option<Match> {
        let p = &self.0[key];
        if p.unicode {
            p.regex.find_from_utf16(text, 0).next()
        } else {
            p.regex.find_from_ucs2(text, 0).next()
        }
    }
    pub fn test(&self, key: &str, text: &[u16]) -> bool {
        self.first(key, text).is_some()
    }
}
pub(super) fn group<'a>(m: &Match, text: &'a [u16], number: usize) -> &'a [u16] {
    m.group(number)
        .map(|range| &text[range])
        .unwrap_or_default()
}
