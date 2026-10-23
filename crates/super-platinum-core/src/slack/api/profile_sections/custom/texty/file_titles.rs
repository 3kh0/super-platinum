//! Bounded whole-list and Canvas labels (p292/KhL1), including rich title
//! text, user and ordinary link leaves under explicit source observations.
//!
//! The caller supplies an admitted effective record and observed normal status.
//! This helper establishes neither cache history, kind/status authority, href,
//! nor a localized fallback. Optional title context is explicit caller evidence.
use serde_json::Value;

mod blocks;
mod link;
mod users;
pub use users::{member_label, user_ids};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    WholeList,
    Canvas,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Normal,
    Unavailable,
}

/// Independently observed nonpersistent Quip title state for this same file.
/// Missing file metadata is not evidence of a nullish Quip title. The caller
/// owns account/workspace/file binding and rechecking this observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuipTitle<'a> {
    Unavailable,
    Nullish,
    Text(&'a str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unavailable {
    Status,
    InvalidMetadata,
    UnsupportedTitle,
    LocalizedFallback,
}

/// Resolve only whole-object, normal-status labels. Section/record/view/label
/// override and hide-title options are deliberately not represented by this API.
pub fn label(kind: Kind, status: Status, effective: &Value) -> Result<String, Unavailable> {
    label_with_quip(kind, status, effective, QuipTitle::Unavailable)
}

/// Additionally admits text/link Canvas title blocks when the separate
/// Quip title selector was observed. Empty extraction still requires explicit
/// localization through `label_with_context`, even with an override. Link text is literal, not decoded or
/// navigated. Lists and string titles ignore `quip`.
pub fn label_with_quip(
    kind: Kind,
    status: Status,
    effective: &Value,
    quip: QuipTitle<'_>,
) -> Result<String, Unavailable> {
    if kind == Kind::Canvas
        && status == Status::Normal
        && quip == QuipTitle::Unavailable
        && effective["title_blocks"]
            .as_array()
            .is_some_and(|v| !v.is_empty())
    {
        return Err(Unavailable::UnsupportedTitle);
    }
    label_with_users(kind, status, effective, quip, |_| {
        Err(Unavailable::UnsupportedTitle)
    })
}

/// User-containing titles consume independently observed member labels and do
/// not consult Quip's editor override (laT0/JCh1). The callback owns policy.
pub fn label_with_users(
    kind: Kind,
    status: Status,
    effective: &Value,
    quip: QuipTitle<'_>,
    user: impl FnMut(&str) -> Result<String, Unavailable>,
) -> Result<String, Unavailable> {
    label_with_context(kind, status, effective, quip, None, user)
}

/// The caller supplies the independently observed localized canvas/Untitled
/// value. It applies only to empty Canvas extraction, before Quip selection and
/// sanitization. None preserves conservative behavior; this never grants file,
/// member, status, or locale authority and does not supply a List fallback.
pub fn label_with_context(
    kind: Kind,
    status: Status,
    effective: &Value,
    quip: QuipTitle<'_>,
    canvas_fallback: Option<&str>,
    mut user: impl FnMut(&str) -> Result<String, Unavailable>,
) -> Result<String, Unavailable> {
    if status != Status::Normal {
        return Err(Unavailable::Status);
    }
    if !effective.is_object() {
        return Err(Unavailable::InvalidMetadata);
    }
    let title = text(&effective["title"])?;
    let value = match kind {
        Kind::WholeList => {
            let title = unescape(title);
            let title = title.trim_matches(js_space);
            if title.is_empty() {
                let name = text(&effective["name"])?.trim_matches(js_space);
                if name.is_empty() {
                    return Err(Unavailable::LocalizedFallback);
                }
                name.to_owned()
            } else {
                title.to_owned()
            }
        }
        Kind::Canvas => {
            match &effective["title_blocks"] {
                Value::Null => {}
                Value::Array(blocks) if blocks.is_empty() => {}
                Value::Array(items) => {
                    return blocks::label_with_context(items, quip, canvas_fallback, &mut user)
                        .map(|v| sanitize(&v));
                }
                _ => return Err(Unavailable::UnsupportedTitle),
            }
            let value = plain_canvas(&unescape(title))?;
            if value.is_empty() {
                canvas_fallback
                    .ok_or(Unavailable::LocalizedFallback)?
                    .to_owned()
            } else {
                value
            }
        }
    };
    // Source chooses fallback BEFORE sanitization; a nonempty title consisting
    // only of a soft hyphen can therefore yield an intentionally empty label.
    Ok(sanitize(&value))
}

fn text(value: &Value) -> Result<&str, Unavailable> {
    match value {
        Value::Null => Ok(""),
        Value::String(value) => Ok(value),
        _ => Err(Unavailable::InvalidMetadata),
    }
}

// L7LR is lodash unescape, not a general HTML/numeric-entity decoder. One pass.
fn unescape(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut tail = value;
    while let Some(at) = tail.find('&') {
        result.push_str(&tail[..at]);
        tail = &tail[at..];
        let entity = [
            ("&amp;", "&"),
            ("&lt;", "<"),
            ("&gt;", ">"),
            ("&quot;", "\""),
            ("&#39;", "'"),
        ]
        .into_iter()
        .find(|(entity, _)| tail.starts_with(entity));
        if let Some((entity, decoded)) = entity {
            result.push_str(decoded);
            tail = &tail[entity.len()..];
        } else {
            result.push('&');
            tail = &tail[1..];
        }
    }
    result.push_str(tail);
    result
}

// ECMAScript \s used by fEyw/1neI/PXih. Rust is_whitespace differs at U+0085
// and does not include BOM. No NFC or Unicode case normalization is performed.
fn js_space(c: char) -> bool {
    matches!(c, '\u{0009}'..='\u{000d}' | ' ' | '\u{00a0}' | '\u{1680}'
        | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}'
        | '\u{205f}' | '\u{3000}' | '\u{feff}')
}

fn line_edges(value: &str, marker: impl Fn(char) -> bool) -> String {
    let mut result = String::with_capacity(value.len());
    for part in value.split_inclusive(['\r', '\n']) {
        let terminator = part.chars().last().filter(|c| matches!(c, '\r' | '\n'));
        let content = if terminator.is_some() {
            &part[..part.len() - 1]
        } else {
            part
        };
        result.push_str(content.trim_matches(&marker));
        if let Some(c) = terminator {
            result.push(c);
        }
    }
    result
}

// 0jGF order matters: edge trimming occurs before soft-hyphen removal.
fn sanitize(value: &str) -> String {
    let value = value.replace(['\u{2028}', '\u{2029}'], "\n");
    line_edges(&value, |c| matches!(c, '\u{200b}' | '\u{feff}'))
        .replace('\u{00ad}', "")
        .replace('\u{00a0}', " ")
}

// A deliberately conservative ordinary-text grammar. The retained parser may
// accept isolated delimiters too, but classifying them requires the semantic
// mrkdwn path. Refuse them instead of silently returning a guessed title.
fn ordinary(value: &str) -> bool {
    !value.chars().any(|c| {
        matches!(
            c,
            '*' | '_'
                | '~'
                | '`'
                | '<'
                | '>'
                | ':'
                | '#'
                | '\\'
                | '\u{ff0a}'
                | '\u{ff3f}'
                | '\u{ff40}'
                | '\u{ff5e}'
                | '\u{e000}'..='\u{f8ff}'
        )
    }) && !value.contains("&lt;")
        && !value.contains("&gt;")
}

fn plain_canvas(value: &str) -> Result<String, Unavailable> {
    if !ordinary(value) {
        return Err(Unavailable::UnsupportedTitle);
    }
    // Original j+eK StringTools.trim is ASCII 9..13 and space, unlike lodash.
    let value = value
        .trim_matches(|c| matches!(c, '\u{0009}'..='\u{000d}' | ' '))
        .replace("\r\n", "\n")
        .replace(['\r', '\u{2028}', '\u{2029}'], "\n");
    let mut value = line_edges(&value, |c| c == '\u{200b}');
    // Private markers cannot occur in admitted input. They stand for original
    // LINE/PARA/SPACE tokens; bQFi ignores SPACE:HARD tokens altogether.
    const LINE: char = '\u{e000}';
    const PARA: char = '\u{e001}';
    const HARD: char = '\u{e002}';
    for _ in 0..2 {
        let chars: Vec<_> = value.chars().collect();
        value.clear();
        let mut i = 0;
        while i < chars.len() {
            if i + 2 < chars.len()
                && chars[i] != '\n'
                && chars[i + 1] == '\n'
                && chars[i + 2] != '\n'
            {
                value.extend([chars[i], LINE, chars[i + 2]]);
                i += 3;
            } else {
                value.push(chars[i]);
                i += 1;
            }
        }
    }
    let chars: Vec<_> = value.chars().collect();
    value.clear();
    let mut i = 0;
    while i < chars.len() {
        value.push(chars[i]);
        i += if chars[i] != '\n' && chars.get(i + 1) == Some(&'\n') {
            2
        } else {
            1
        };
    }
    value = value
        .replace('\n', &PARA.to_string())
        .replace("&nbsp;&nbsp;", &format!(" {LINE}"))
        .replace('\t', &HARD.to_string().repeat(4))
        .replace("  ", &format!(" {HARD}"));
    if value.starts_with(' ') {
        value.replace_range(..1, &HARD.to_string());
    }
    value = value
        .replace(HARD, "")
        .replace(LINE, "\n")
        .replace(PARA, "\n\n");
    // laT0 unescapes before parsing; bQFi unescapes the assembled text again.
    Ok(unescape(&value))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod context_tests;
