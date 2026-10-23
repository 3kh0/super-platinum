//! Exact 8J14 text/style branch. No state lookup or native editor operation.
use super::*;
use regress::Regex;
use std::sync::OnceLock;

pub(super) fn annotate(mut chunk: Value, op: &Value) -> Value {
    let attributes = op.get("attributes");
    let mut style = Map::new();
    for key in ["bold", "italic", "underline", "strike", "code"] {
        if truthy(attributes.and_then(|a| a.get(key))) {
            style.insert(key.into(), json!(true));
        }
    }
    let mention =
        op["insert"].is_string() && truthy(attributes.and_then(|a| a.get("slackmention")));
    let emoji = truthy(op.get("insert").and_then(|v| v.get("slackemoji")));
    if truthy(attributes.and_then(|a| a.get("unlink"))) && !mention && !emoji {
        style.insert("unlink".into(), json!(true));
    }
    if !style.is_empty() {
        chunk["style"] = Value::Object(style);
    }
    chunk
}

pub(super) fn chunks(
    op: &Value,
    strip: bool,
    preformatted: bool,
    skip_conversions: bool,
) -> Result<Vec<Value>> {
    let original = op["insert"].as_str().unwrap_or("");
    let original = if strip {
        original.trim_end_matches(js_space)
    } else {
        original
    };
    let text = if preformatted {
        original.to_owned()
    } else {
        strip_line_spaces(original)?
    };
    if text.is_empty() {
        return Ok(vec![]);
    }
    let convert = !skip_conversions
        && !preformatted
        && !truthy(op.get("attributes").and_then(|a| a.get("code")));
    let make = |value| {
        if preformatted {
            value
        } else {
            annotate(value, op)
        }
    };
    if !convert {
        return Ok(vec![make(json!({"type":"text","text":text}))]);
    }
    let units: Vec<_> = text.encode_utf16().collect();
    let mut colors = vec![];
    for found in patterns()?.color.find_from_ucs2(&units, 0) {
        let prefix = found.group(1).ok_or(SeedUnavailable::Patterns)?;
        let name = found.group(2).ok_or(SeedUnavailable::Patterns)?;
        let mut end = name.end;
        while end > name.start && matches!(units[end - 1], 95 | 45 | 46 | 0x2026) {
            end -= 1;
        }
        let hex = &units[name.start..end];
        if hex.len() == 6 && hex.iter().all(|c| matches!(c,48..=57|65..=70|97..=102)) {
            let start = found.range.start + prefix.len();
            colors.push((
                start,
                start + hex.len() + 1,
                scalar(hex)?.to_ascii_uppercase(),
            ));
        }
    }
    if colors.is_empty() {
        return Ok(vec![make(json!({"type":"text","text":text}))]);
    }
    let mut out = vec![];
    let mut at = 0;
    for (start, end, color) in colors {
        if at < start {
            out.push(annotate(
                json!({"type":"text","text":scalar(&units[at..start])?}),
                op,
            ));
        }
        out.push(annotate(
            json!({"type":"color","value":format!("#{color}")}),
            op,
        ));
        at = end;
    }
    if at < units.len() {
        out.push(annotate(
            json!({"type":"text","text":scalar(&units[at..])?}),
            op,
        ));
    }
    Ok(out)
}
fn scalar(units: &[u16]) -> Result<String> {
    String::from_utf16(units).map_err(|_| SeedUnavailable::InvalidUnicode)
}
fn strip_line_spaces(text: &str) -> Result<String> {
    let units: Vec<_> = text.encode_utf16().collect();
    let mut out = Vec::new();
    let mut at = 0;
    for found in patterns()?.spaces.find_from_ucs2(&units, 0) {
        out.extend_from_slice(&units[at..found.range.start]);
        for group in [1, 3] {
            out.extend_from_slice(&units[found.group(group).ok_or(SeedUnavailable::Patterns)?]);
        }
        at = found.range.end;
    }
    out.extend_from_slice(&units[at..]);
    scalar(&out)
}
pub(in super::super) fn js_space(c: char) -> bool {
    matches!(c,'\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{00a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}')
}
struct Patterns {
    spaces: Regex,
    color: Regex,
}
fn patterns() -> Result<&'static Patterns> {
    static VALUE: OnceLock<Result<Patterns>> = OnceLock::new();
    VALUE
        .get_or_init(|| {
            let data: Value = serde_json::from_str(include_str!("text-patterns.json"))
                .map_err(|_| SeedUnavailable::Patterns)?;
            let read = |key: &str| {
                Regex::with_flags(
                    data[key]["source"]
                        .as_str()
                        .ok_or(SeedUnavailable::Patterns)?,
                    data[key]["flags"]
                        .as_str()
                        .ok_or(SeedUnavailable::Patterns)?,
                )
                .map_err(|_| SeedUnavailable::Patterns)
            };
            Ok(Patterns {
                spaces: read("spaces")?,
                color: read("color")?,
            })
        })
        .as_ref()
        .map_err(Clone::clone)
}
