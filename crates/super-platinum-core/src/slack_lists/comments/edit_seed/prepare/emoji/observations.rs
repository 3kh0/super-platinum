use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, PartialEq)]
pub enum EmojiTranslation {
    #[default]
    Unobserved,
    KnownAbsent,
    /// Exact initialized HSTN localizedToCanonical map, including its source
    /// deburred aliases. A generic localized label map is not this observation.
    Known(BTreeMap<String, String>),
}
#[derive(Clone, Debug, PartialEq)]
pub struct ToneSupport {
    pub name: String,
    pub single: bool,
    pub multiple: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EmojiObservations {
    /// Exact tTLk last-viewed-team locale used for fullwidth colon folding.
    pub current_locale: Option<String>,
    /// Exact rNCs YlrH user locale passed to HSTN translation (empty is known).
    pub locale: Option<String>,
    /// Complete positively observed UHpe.CK set, including custom names.
    pub names: Option<BTreeSet<String>>,
    /// Positive membership only, used when the complete standard+custom set is
    /// unavailable. A missing entry is unknown, never a negative lookup. The
    /// first unknown normalization candidate must stop resolution.
    pub positive_names: BTreeSet<String>,
    pub translation: EmojiTranslation,
    /// Exact UHpe.Fm/pA results for reached preceding-emoji names only.
    pub tones: Vec<ToneSupport>,
}
impl EmojiObservations {
    pub(super) fn fold_fullwidth(&self) -> Result<bool> {
        let locale = self
            .current_locale
            .as_deref()
            .ok_or(SeedUnavailable::MalformedMessage)?;
        Ok(matches!(locale, "ja-JP" | "zh-CN" | "zh-TW" | "ko-KR"))
    }
    pub(super) fn canonical(&self, text: &str) -> Result<Option<Parsed>> {
        for (index, source) in [
            text.to_owned(),
            text.replace('-', "_"),
            text.replace('_', "-"),
        ]
        .into_iter()
        .enumerate()
        {
            let translated = self.translate(&source)?;
            let parsed = parse(&translated)?;
            if index == 0 {
                if let Some(parsed) = &parsed {
                    if patterns::get()?
                        .skin
                        .find(&format!(":{}:", parsed.name))
                        .is_some()
                    {
                        return Ok(Some(Parsed {
                            name: parsed.name.clone(),
                            tone: None,
                        }));
                    }
                }
            }
            if let Some(candidate) = &parsed {
                let present = match &self.names {
                    Some(names) => names.contains(&candidate.name),
                    None if self.positive_names.contains(&candidate.name) => true,
                    None => return Err(SeedUnavailable::MissingEmoji(candidate.name.clone())),
                };
                if present {
                    return Ok(parsed);
                }
            }
        }
        Ok(None)
    }
    fn translate(&self, text: &str) -> Result<String> {
        let locale = self
            .locale
            .as_deref()
            .ok_or(SeedUnavailable::MalformedMessage)?;
        if locale.is_empty() {
            return Ok(String::new());
        }
        let map = match &self.translation {
            EmojiTranslation::Unobserved => return Err(SeedUnavailable::MalformedMessage),
            EmojiTranslation::KnownAbsent => return Ok(text.to_owned()),
            EmojiTranslation::Known(map) => map,
        };
        let body = text.strip_prefix(':').unwrap_or(text);
        let body = body.strip_suffix(':').unwrap_or(body).to_lowercase();
        let mut pieces = body.split("::");
        let name = pieces.next().unwrap_or("");
        if name == "__proto__" && map.contains_key(name) {
            // HSTN initializes an ordinary object by assignment. A string
            // assignment to this inherited setter cannot create an own entry.
            return Err(SeedUnavailable::InvalidObservation);
        }
        if !map.contains_key(name) && inherited(name) {
            return Err(SeedUnavailable::InvalidObservation);
        }
        let Some(translated) = map.get(name).filter(|s| !s.is_empty()) else {
            return Ok(text.to_owned());
        };
        limits::check(translated)?;
        let mut value = translated.clone();
        if let Some(tone) = pieces.next().filter(|s| !s.is_empty()) {
            if tone == "__proto__" && map.contains_key(tone) {
                return Err(SeedUnavailable::InvalidObservation);
            }
            if !map.contains_key(tone) && inherited(tone) {
                return Err(SeedUnavailable::InvalidObservation);
            }
            let Some(translated) = map.get(tone).filter(|s| !s.is_empty()) else {
                return Ok(text.to_owned());
            };
            limits::joined(&value, translated)?;
            value.push_str("::");
            value.push_str(translated);
        }
        if text.starts_with(':')
            && text.ends_with(':')
            && !text.chars().any(super::patterns::whitespace)
        {
            Ok(format!(":{value}:"))
        } else {
            Ok(value)
        }
    }
}
pub(super) struct ToneIndex<'a>(BTreeMap<&'a str, Option<&'a ToneSupport>>);
impl<'a> ToneIndex<'a> {
    pub fn capture(values: &'a [ToneSupport]) -> Result<Self> {
        if values.len() > 32768 {
            return Err(SeedUnavailable::Limit);
        }
        let mut index = BTreeMap::new();
        let mut bytes = 0usize;
        for value in values {
            bytes = bytes
                .checked_add(value.name.len())
                .filter(|n| *n <= 8 * 1024 * 1024)
                .ok_or(SeedUnavailable::Limit)?;
            index
                .entry(value.name.as_str())
                .and_modify(|v| *v = None)
                .or_insert(Some(value));
        }
        Ok(Self(index))
    }
    pub fn get(&self, name: &str, multiple: bool) -> Result<bool> {
        let value = self
            .0
            .get(name)
            .ok_or_else(|| SeedUnavailable::MissingEmoji(name.into()))?
            .ok_or(SeedUnavailable::InvalidObservation)?;
        Ok(if multiple {
            value.multiple
        } else {
            value.single
        })
    }
}
#[derive(Clone, Debug)]
pub(super) struct Parsed {
    pub name: String,
    pub tone: Option<String>,
}
impl Parsed {
    pub fn text(&self) -> String {
        match &self.tone {
            Some(tone) => format!(":{}::skin-tone-{tone}:", self.name),
            None => format!(":{}:", self.name),
        }
    }
}
pub(super) fn parse(text: &str) -> Result<Option<Parsed>> {
    if text.is_empty() {
        return Ok(None);
    }
    if inherited(text) {
        return Err(SeedUnavailable::MalformedMessage);
    }
    let lower = text.to_lowercase();
    let value = lower.strip_prefix(':').unwrap_or(&lower);
    let value = value.strip_suffix(':').unwrap_or(value);
    let units: Vec<_> = value.encode_utf16().collect();
    let pattern = &patterns::get()?.parsed;
    let found = pattern.find_from_ucs2(&units, 0).next();
    let (name, tone) = if let Some(found) = found {
        let tone = found
            .group(1)
            .or_else(|| found.group(2))
            .map(|r| String::from_utf16(&units[r]))
            .transpose()
            .map_err(|_| SeedUnavailable::InvalidUnicode)?;
        let mut name = units[..found.range.start].to_vec();
        name.extend_from_slice(&units[found.range.end..]);
        (
            String::from_utf16(&name).map_err(|_| SeedUnavailable::InvalidUnicode)?,
            tone,
        )
    } else {
        (value.to_owned(), None)
    };
    if name.is_empty() || name.contains(':') {
        return Ok(None);
    }
    Ok(Some(Parsed { name, tone }))
}

// Retained ue25/HSTN use ordinary JS objects. Missing own keys with these names
// read inherited functions/objects; no engine-specific conversion is invented.
fn inherited(name: &str) -> bool {
    matches!(
        name,
        "constructor"
            | "toString"
            | "toLocaleString"
            | "valueOf"
            | "hasOwnProperty"
            | "isPrototypeOf"
            | "propertyIsEnumerable"
            | "__proto__"
            | "__defineGetter__"
            | "__defineSetter__"
            | "__lookupGetter__"
            | "__lookupSetter__"
    )
}
