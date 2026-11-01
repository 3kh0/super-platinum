//! Supplied Files-search reading data. No File/store/action conversion exists.
//! The raw incoming display gate precedes the constructive timestamp-knownness
//! fold. Missing is consumed only by the pinned JS operations below, never
//! materialized as a visible/access/knownness field.
use serde_json::{Map, Value};

// Defensive native limits, not Slack wire/schema or complete-file bounds.
const MAX_BYTES: usize = 16 * 1024;
const MAX_UTF16_UNITS: usize = 4096;
const MAX_PARTS: usize = 256;
const MAX_TIMESTAMP: f64 = 253_402_300_799.0;

/// Detached immutable text only. It carries no acquisition/currentness proof.
pub struct Excerpt {
    parts: Vec<Part>,
}
/// A typed reading fragment; no URL, HTML, resource, member or callback field.
pub struct Part {
    text: String,
    highlighted: bool,
}
impl Excerpt {
    pub fn parts(&self) -> &[Part] {
        &self.parts
    }
}
impl Part {
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn highlighted(&self) -> bool {
        self.highlighted
    }
}

#[derive(Clone, Copy)]
enum Raw<'a> {
    Missing,
    Present(&'a Value),
}
struct Original<'a>(&'a Map<String, Value>);
impl<'a> Original<'a> {
    fn get(&self, key: &str) -> Raw<'a> {
        self.0.get(key).map_or(Raw::Missing, Raw::Present)
    }
}
impl Raw<'_> {
    /// The exact strict-string-comparison operation sees undefined as unequal;
    /// this does not declare a missing access value visible. Other JSON shapes
    /// are outside this native supported domain.
    fn equals_string(self, expected: &str) -> Option<bool> {
        match self {
            Self::Missing | Self::Present(Value::Null) => Some(false),
            Self::Present(Value::String(s)) if s.len() <= 128 => Some(s == expected),
            _ => None,
        }
    }
    fn equals_number(self, expected: f64) -> Option<bool> {
        match self {
            Self::Missing | Self::Present(Value::Null) => Some(false),
            Self::Present(Value::Number(n)) => Some(n.as_f64()? == expected),
            _ => None,
        }
    }
    /// Only the source !!field operation consumes Missing here. Malformed
    /// scalar/container flag shapes are refused more strictly than JavaScript.
    fn flag_truthiness(self) -> Option<bool> {
        match self {
            Self::Missing | Self::Present(Value::Null) => Some(false),
            Self::Present(Value::Bool(b)) => Some(*b),
            _ => None,
        }
    }
    /// Full JSON truthiness is needed to select updated||created||timestamp.
    /// A truthy malformed earlier operand must block fallback to a later one.
    fn timestamp_truthiness(self) -> bool {
        match self {
            Self::Missing | Self::Present(Value::Null) => false,
            Self::Present(Value::Bool(b)) => *b,
            Self::Present(Value::Number(n)) => n.as_f64().is_none_or(|n| n != 0.0),
            Self::Present(Value::String(s)) => !s.is_empty(),
            Self::Present(Value::Array(_) | Value::Object(_)) => true,
        }
    }
}

/// Exact default includeTempQuipDocs:false footprint on supported raw shapes.
/// The temporary-ID branch is unreachable: truthy isNonExistent denies entry.
fn incoming_show(raw: &Original<'_>) -> Option<bool> {
    for key in [
        "isNonExistent",
        "is_hidden_by_limit",
        "is_deleted",
        "is_tombstoned",
    ] {
        if raw.get(key).flag_truthiness()? {
            return Some(false);
        }
    }
    for mode in [
        "tombstone",
        "not_found",
        "hidden_by_limit",
        "access_denied",
        "canvas_slack_connect_blocked",
        "lists_slack_connect_blocked",
        "not_visible",
        "template_not_visible",
    ] {
        if raw.get("mode").equals_string(mode)? {
            return Some(false);
        }
    }
    for access in [
        "file_not_found",
        "access_denied",
        "not_visible",
        "template_not_visible",
        "canvas_globally_disabled",
        "slack_connect_team_blocked",
        "slack_connect_canvas_sharing_blocked",
        "canvas_disabled_user_team",
        "canvas_disabled_file_team",
        "slack_connect_lists_team_blocked",
        "slack_connect_lists_sharing_blocked",
    ] {
        if raw.get("file_access").equals_string(access)? {
            return Some(false);
        }
    }
    // Pinned +6fk.Wk.DSA_RESTRICTED is numeric 1, using strict equality.
    Some(!raw.get("restriction_type").equals_number(1.0)?)
}

fn known_after_entry(raw: &Original<'_>) -> Option<()> {
    if !incoming_show(raw)? {
        return None;
    }
    // Stronger native supported policy domains. Missing/null stay distinct;
    // neither is stored as visible or numeric zero.
    match raw.get("file_access") {
        Raw::Missing | Raw::Present(Value::Null) => {}
        Raw::Present(Value::String(s)) if s == "visible" => {}
        _ => return None,
    }
    match raw.get("restriction_type") {
        Raw::Missing | Raw::Present(Value::Null) => {}
        value if value.equals_number(0.0) == Some(true) => {}
        _ => return None,
    }
    for key in ["updated", "created", "timestamp"] {
        let selected = raw.get(key);
        if !selected.timestamp_truthiness() {
            continue;
        }
        // Actual VP operand selected by JS ||, not a scan for any valid date.
        let Raw::Present(Value::Number(n)) = selected else {
            return None;
        };
        let n = n.as_f64()?;
        return (n.is_finite() && n > 0.0 && n <= MAX_TIMESTAMP).then_some(());
    }
    None
}

pub(super) fn from_original_item(item: &Value) -> Option<Excerpt> {
    let object = item.as_object()?;
    let id = object.get("id")?.as_str()?;
    if !super::identifier(id)
        || object.get("mode")?.as_str()? != "snippet"
        || object.get("filetype")?.as_str()? != "text"
        || object.get("mimetype")?.as_str()? != "text/plain"
    {
        return None;
    }
    // processFile must pass the ORIGINAL incoming guard before computeIsUnknown
    // constructively writes false knownness markers. Raw isUnknown absence (or
    // a cached id/default DTO) is deliberately not inspected as evidence.
    known_after_entry(&Original(object))?;
    let first = object
        .get("contents_extracts")?
        .as_array()?
        .first()?
        .as_str()?;
    if first.len() > MAX_BYTES
        || first.encode_utf16().take(MAX_UTF16_UNITS + 1).count() > MAX_UTF16_UNITS
        || first.contains(':')
    {
        return None;
    }
    let text = first.trim_start_matches(ecmascript_space);
    if text.is_empty() {
        return None;
    }
    // Both source formatters receive inputIsEscaped:false. The fixed emoji
    // pattern requires a literal colon, so this subset emits only text and
    // paired-marker highlights. No HTML/mention/mrkdwn/emoji parser is used.
    Some(Excerpt {
        parts: highlighted_parts(text)?,
    })
}

fn ecmascript_space(c: char) -> bool {
    matches!(
        c,
        '\u{0009}'
            | '\u{000a}'
            | '\u{000b}'
            | '\u{000c}'
            | '\u{000d}'
            | '\u{0020}'
            | '\u{00a0}'
            | '\u{1680}'
            | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

fn highlighted_parts(text: &str) -> Option<Vec<Part>> {
    const OPEN: char = '\u{e000}';
    const CLOSE: char = '\u{e001}';
    let mut parts = Vec::new();
    let mut cursor = 0;
    while let Some(offset) = text[cursor..].find(OPEN) {
        let open = cursor + offset;
        let content = open + OPEN.len_utf8();
        let Some(offset) = text[content..].find(CLOSE) else {
            break;
        };
        let close = content + offset;
        if open > cursor {
            append_part(&mut parts, &text[cursor..open], false)?;
        }
        // The source regex permits an empty capture and additional OPEN markers
        // inside it; this is not a balancing/nesting parser.
        append_part(&mut parts, &text[content..close], true)?;
        cursor = close + CLOSE.len_utf8();
    }
    if cursor < text.len() {
        append_part(&mut parts, &text[cursor..], false)?;
    }
    Some(parts)
}

fn append_part(parts: &mut Vec<Part>, text: &str, highlighted: bool) -> Option<()> {
    if parts.len() >= MAX_PARTS {
        return None;
    }
    parts.push(Part {
        text: text.into(),
        highlighted,
    });
    Some(())
}

#[cfg(test)]
mod tests;
