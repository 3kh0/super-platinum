//! One-pass pxlU entity semantics. Named values are WHATWG HTML entities from
//! Python's standard-library html.entities.html5, restricted to semicolon keys.
//! The checked-in table is independently compared with the retained source map;
//! no third-party decoder or browser's permissive partial-entity rules are used.
use std::{collections::HashMap, sync::LazyLock};

static NAMED: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("entities.json")).expect("checked-in HTML named entity table")
});

/// Decode named, decimal and hexadecimal entities once, with a mandatory `;`.
/// Call this only on text leaves/labels after Slack-token parsing: decoded
/// mentions, links and ampersands must not be parsed again or URLs rewritten.
/// Unknown entities retain their original spelling and named lookup is case
/// sensitive. Numeric values are literal code points (no HTML C1 remapping).
/// JS can represent isolated UTF-16 surrogates; Rust strings cannot. Those
/// numeric entities deliberately remain unchanged, rather than being replaced.
pub fn decode_entities(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut cursor = 0;
    while let Some(relative) = text[cursor..].find('&') {
        let start = cursor + relative;
        result.push_str(&text[cursor..start]);
        let bytes = text.as_bytes();
        let mut end = start + 1;
        if bytes.get(end) == Some(&b'#') {
            end += 1;
        }
        let body_start = end;
        while bytes.get(end).is_some_and(u8::is_ascii_alphanumeric) {
            end += 1;
        }
        if end == body_start || bytes.get(end) != Some(&b';') {
            result.push('&');
            cursor = start + 1;
            continue;
        }
        let entity = &text[start..=end];
        if let Some(value) = NAMED.get(entity) {
            result.push_str(value);
        } else if let Some(value) = numeric(entity) {
            result.push(value);
        } else {
            result.push_str(entity);
        }
        cursor = end + 1;
    }
    result.push_str(&text[cursor..]);
    result
}

fn numeric(entity: &str) -> Option<char> {
    let body = entity.strip_prefix("&#")?.strip_suffix(';')?;
    let (digits, radix) =
        if let Some(hex) = body.strip_prefix('x').or_else(|| body.strip_prefix('X')) {
            if hex.is_empty() || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            (hex, 16)
        } else {
            if body.is_empty() || !body.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            (body, 10)
        };
    char::from_u32(u32::from_str_radix(digits, radix).ok()?)
}

#[cfg(test)]
mod tests;
