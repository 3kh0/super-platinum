use super::*;
use regress::Regex;
use std::sync::OnceLock;
pub(super) struct Patterns {
    pub emoji: Regex,
    pub url: Regex,
    pub ipv6: Regex,
    pub parsed: Regex,
    pub skin: Regex,
}
pub(super) fn get() -> Result<&'static Patterns> {
    static VALUE: OnceLock<Result<Patterns>> = OnceLock::new();
    VALUE
        .get_or_init(|| {
            let data: Value = serde_json::from_str(include_str!("patterns.json"))
                .map_err(|_| SeedUnavailable::Patterns)?;
            let read = |name: &str| {
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
            Ok(Patterns {
                emoji: read("emoji")?,
                url: read("url")?,
                ipv6: read("ipv6")?,
                parsed: read("parsed")?,
                skin: read("skin")?,
            })
        })
        .as_ref()
        .map_err(Clone::clone)
}
pub(super) fn whitespace(c: char) -> bool {
    matches!(c,'\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{00a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}')
}
pub(super) fn in_code(text: &[u16]) -> bool {
    let mut triples = Vec::new();
    let mut at = 0;
    while at + 3 <= text.len() {
        if text[at..at + 3] == [96, 96, 96] {
            triples.push(at);
            at += 3
        } else {
            at += 1
        }
    }
    if triples.len() % 2 == 1 {
        return true;
    }
    let mut ticks = 0;
    let mut previous = 0;
    for pair in triples.chunks_exact(2) {
        ticks += text[previous..pair[0]].iter().filter(|c| **c == 96).count();
        previous = pair[1] + 3
    }
    ticks += text[previous..].iter().filter(|c| **c == 96).count();
    ticks % 2 == 1
}
