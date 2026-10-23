//! ECMAScript non-u matching over UTF-16, matching retained TSF's EReg.
use super::SeedUnavailable;
use regress::{Match, Regex};
use std::{collections::BTreeMap, sync::OnceLock};
pub(super) type Js = Vec<u16>;
pub(super) type Result<T> = std::result::Result<T, SeedUnavailable>;
pub(super) fn w(s: &str) -> Js {
    s.encode_utf16().collect()
}
pub(super) fn string(s: &[u16]) -> Result<String> {
    String::from_utf16(s).map_err(|_| SeedUnavailable::InvalidUnicode)
}
pub(super) fn cat(parts: &[&[u16]]) -> Js {
    parts.concat()
}
pub(super) fn replace(s: &[u16], needle: &str, value: &[u16]) -> Js {
    let needle = w(needle);
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        if s[i..].starts_with(&needle) {
            out.extend(value);
            i += needle.len();
        } else {
            out.push(s[i]);
            i += 1;
        }
    }
    out
}
pub(super) fn trim(s: &[u16]) -> &[u16] {
    let ws = |c: &u16| (9..=13).contains(c) || *c == 32;
    let a = s.iter().position(|c| !ws(c)).unwrap_or(s.len());
    let b = s.iter().rposition(|c| !ws(c)).map_or(a, |i| i + 1);
    &s[a..b]
}
struct Pattern {
    regex: Regex,
    global: bool,
}
pub(super) struct Patterns(BTreeMap<String, Pattern>);
impl Patterns {
    pub fn get() -> Result<&'static Self> {
        static PATTERNS: OnceLock<Result<Patterns>> = OnceLock::new();
        PATTERNS
            .get_or_init(|| {
                let data: serde_json::Value = serde_json::from_str(include_str!("patterns.json"))
                    .map_err(|_| SeedUnavailable::Patterns)?;
                let mut result = BTreeMap::new();
                for (key, value) in data.as_object().ok_or(SeedUnavailable::Patterns)? {
                    let source = value["source"].as_str().ok_or(SeedUnavailable::Patterns)?;
                    let flags = value["flags"].as_str().ok_or(SeedUnavailable::Patterns)?;
                    let regex = Regex::from_unicode(source.encode_utf16().map(u32::from), flags)
                        .map_err(|_| SeedUnavailable::Patterns)?;
                    result.insert(
                        key.clone(),
                        Pattern {
                            regex,
                            global: flags.contains('g'),
                        },
                    );
                }
                Ok(Patterns(result))
            })
            .as_ref()
            .map_err(Clone::clone)
    }
    pub fn find(&self, key: &str, text: &[u16], start: usize) -> Option<Match> {
        self.0.get(key)?.regex.find_from_ucs2(text, start).next()
    }
    pub fn test(&self, key: &str, text: &[u16]) -> bool {
        self.find(key, text, 0).is_some()
    }
    pub fn map(
        &self,
        key: &str,
        text: &[u16],
        mut f: impl FnMut(&Capture<'_>) -> Result<Js>,
    ) -> Result<Js> {
        let pattern = self.0.get(key).ok_or(SeedUnavailable::Patterns)?;
        let mut out = Vec::new();
        let mut at = 0;
        while at < text.len() {
            let Some(m) = pattern.regex.find_from_ucs2(text, at).next() else {
                out.extend(&text[at..]);
                break;
            };
            out.extend(&text[at..m.range.start]);
            out.extend(f(&Capture { text, m: m.clone() })?);
            at = m.range.end;
            if m.range.is_empty() {
                if let Some(c) = text.get(at) {
                    out.push(*c);
                    at += 1;
                } else {
                    break;
                }
            }
            if !pattern.global {
                out.extend(&text[at..]);
                break;
            }
            if out.len() > 262_144 {
                return Err(SeedUnavailable::Limit);
            }
        }
        Ok(out)
    }
}
pub(super) struct Capture<'a> {
    pub text: &'a [u16],
    pub m: Match,
}
impl Capture<'_> {
    pub fn g(&self, n: usize) -> &[u16] {
        self.m.group(n).map(|r| &self.text[r]).unwrap_or(&[])
    }
    pub fn has(&self, n: usize) -> bool {
        self.m.group(n).is_some()
    }
}
