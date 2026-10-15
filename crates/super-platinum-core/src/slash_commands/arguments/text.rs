use super::*;

pub(super) fn annotate(mut value: Value, op: &Op) -> Value {
    let mut style = Map::new();
    for key in ["bold", "italic", "underline", "strike", "code"] {
        if op.attrs.get(key) == Some(&Value::Bool(true)) {
            style.insert(key.into(), true.into());
        }
    }
    if op.attrs.get("unlink") == Some(&Value::Bool(true))
        && !op.attrs.contains_key("slackmention")
        && op.insert.is_string()
    {
        style.insert("unlink".into(), true.into());
    }
    if !style.is_empty() {
        value["style"] = style.into();
    }
    value
}
fn line_spaces(input: &str) -> String {
    let mut result = String::new();
    let lines: Vec<_> = input.split('\n').collect();
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            result.push('\n');
        }
        if index + 1 < lines.len() {
            let trimmed = line.trim_end_matches(' ');
            if trimmed.is_empty()
                || trimmed
                    .chars()
                    .next_back()
                    .is_some_and(|c| !space(c) || matches!(c, '\r' | '\u{2028}' | '\u{2029}'))
            {
                result.push_str(trimmed);
                continue;
            }
        }
        result.push_str(line);
    }
    result
}
fn prefix(c: char) -> bool {
    space(c) || "{[(><*_/\"\u{201c}\u{2018}',".contains(c)
}
fn excluded(c: char) -> bool {
    space(c) || "~`!@#$%^&*()+=[]{}\\|;:'\"\u{201d}\u{2019},.<>/?".contains(c)
}

pub(super) fn convert(op: &Op, strip: bool, skip: bool) -> Result<Vec<Value>, Unsupported> {
    let raw = op.insert.as_str().ok_or(Unsupported::Content)?;
    let raw = if strip {
        raw.trim_end_matches(space)
    } else {
        raw
    };
    let text = line_spaces(raw);
    if text.is_empty() {
        return Ok(Vec::new());
    }
    if skip || op.attrs.get("code") == Some(&Value::Bool(true)) {
        return Ok(vec![annotate(json!({"type":"text","text":text}), op)]);
    }
    let chars: Vec<_> = text.char_indices().collect();
    let mut spans = Vec::new();
    let mut position = 0;
    let mut matched_until = 0;
    while position < chars.len() {
        let (start, ch) = chars[position];
        if ch != '#'
            || position > 0
                && (!prefix(chars[position - 1].1) || chars[position - 1].0 < matched_until)
        {
            position += 1;
            continue;
        }
        let mut end = position + 1;
        while end < chars.len() && !excluded(chars[end].1) {
            end += 1;
        }
        if end == position + 1 {
            position += 1;
            continue;
        }
        let stop = chars.get(end).map(|v| v.0).unwrap_or(text.len());
        matched_until = stop;
        let token = text[start + 1..stop].trim_end_matches(['_', '-', '.', '\u{2026}']);
        if token.len() == 6 && token.bytes().all(|b| b.is_ascii_hexdigit()) {
            spans.push((
                start,
                start + 1 + token.len(),
                format!("#{}", token.to_ascii_uppercase()),
            ));
        }
        position = end;
    }
    if spans.is_empty() {
        return Ok(vec![annotate(json!({"type":"text","text":text}), op)]);
    }
    let mut result = Vec::new();
    let mut offset = 0;
    for (start, end, color) in spans {
        if offset < start {
            result.push(annotate(
                json!({"type":"text","text":text[offset..start]}),
                op,
            ));
        }
        result.push(annotate(json!({"type":"color","value":color}), op));
        offset = end;
    }
    if offset < text.len() {
        result.push(annotate(json!({"type":"text","text":text[offset..]}), op));
    }
    Ok(result)
}
