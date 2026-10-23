//! k1tX single-line formatting, with the exact v+tG detector supplied by the
//! preparation pipeline. No generic mrkdwn parser or rendered-text round trip.
use super::*;
use patterns::{Patterns, group};
use serde_json::Map;

pub(super) struct Link {
    pub text: Vec<u16>,
    pub url: String,
}
pub(super) fn format(
    document: &mut Document,
    mut continued_quote: bool,
    detect: &impl Fn(&[u16]) -> Result<Vec<Link>>,
) -> Result<bool> {
    let p = Patterns::get()?;
    let lines = document.lines();
    let original_ends_newline = document.ends_newline();
    let mut offset = 0;
    let mut remove = vec![];
    let mut long_quote = false;
    for (index, (_, line, attrs)) in lines.iter().enumerate() {
        let mut text = regex_text(line)?;
        let mut quote = false;
        let mut border = false;
        if ["code-block", "blockquote", "list"]
            .iter()
            .any(|key| attrs.contains_key(*key))
        {
            long_quote = false;
        } else if long_quote {
            quote = true;
            if let Some(prefix) = p
                .first("Y1wK:zd", &text)
                .filter(|_| !p.test("Y1wK:qv", &text) && !p.test("Y1wK:K", &text))
            {
                border = true;
                remove.extend(offset..offset + prefix.range.len());
                text.drain(..prefix.range.len());
                offset += prefix.range.len();
            }
        } else {
            let triple = p
                .first("Y1wK:qv", &text)
                .or_else(|| p.first("Y1wK:K", &text));
            let single = p.first("Y1wK:zd", &text);
            if index == 0
                && single.as_ref().is_some_and(|m| m.range.start == 0)
                && matches!(text.as_slice(), [62] | [0xff1e] | [62, 32] | [0xff1e, 32])
            {
                continue;
            }
            if triple.is_none() && continued_quote && single.is_some() {
                border = true;
            }
            if let Some(prefix) = triple.as_ref().or(single.as_ref()) {
                quote = true;
                remove.extend(offset..offset + prefix.range.len());
                text.drain(..prefix.range.len());
                offset += prefix.range.len();
                if triple.is_some() {
                    long_quote = true;
                    continued_quote = true;
                    if text.is_empty() {
                        let mut change = Change::default();
                        change.retain(offset);
                        change.delete(1);
                        document.apply(&change)?;
                        continue;
                    }
                }
                if let Some(second) = p.first("Y1wK:zd", &text) {
                    border = true;
                    remove.extend(offset..offset + second.range.len());
                    text.drain(..second.range.len());
                    offset += second.range.len();
                }
            }
        }
        let previous_border =
            border_value(index.checked_sub(1).and_then(|i| lines[i].2.get("border")))?;
        let next = lines.get(index + 1);
        let next_border = border_value(next.and_then(|(_, _, attrs)| attrs.get("border")))?;
        // k1tX tests only the first op's insert, not the joined line. The
        // lookahead can differ when a styled ">" precedes an unstyled "_x".
        let next_quote = if previous_border >= 1.0 && next_border < 1.0 {
            match next {
                Some((_, line, _)) => {
                    let ops = line.ops()?;
                    match ops.first().and_then(|op| op.get("insert")) {
                        Some(Value::String(text)) => {
                            p.test("Y1wK:zd", &text.encode_utf16().collect::<Vec<_>>())
                        }
                        Some(Value::Object(embed)) if embed.contains_key("toString") => {
                            // RegExp.test performs ToString here. A JSON own
                            // toString value cannot be callable, so no primitive
                            // conversion succeeds. Only reject on this actually
                            // reached short-circuit branch; ordinary embeds and
                            // untouched opaque objects elsewhere remain intact.
                            return Err(SeedUnavailable::MalformedMessage);
                        }
                        _ => false,
                    }
                }
                None => false,
            }
        } else {
            false
        };
        if (previous_border >= 1.0 && next_border >= 1.0
            || previous_border >= 1.0 && next_quote && continued_quote)
            && !text.is_empty()
            && attrs.is_empty()
        {
            quote = true;
        }
        if previous_border >= 1.0
            && attrs.is_empty()
            && index + 1 != lines.len() - 1
            && continued_quote
        {
            quote = true;
        }
        if quote {
            let mut attributes = Map::from_iter([("blockquote".into(), json!(true))]);
            if border {
                attributes.insert("border".into(), json!(1));
            }
            let mut change = Change::default();
            change.retain(offset + text.len());
            if original_ends_newline || index != lines.len() - 1 {
                change.annotate(1, attributes);
            } else {
                change.insert("\n", attributes);
            }
            document.apply(&change)?;
        }
        let mut code_ranges = vec![];
        let mut link_ranges = vec![];
        if !truthy(attrs.get("code-block")) {
            // Source explicitly moves code before the remaining original order.
            for (attribute, keys) in [
                ("code", &["SM/6:q6", "SM/6:j5"][..]),
                ("link", &["SM/6:p7"][..]),
                ("bold", &["SM/6:wq", "SM/6:H_"][..]),
                ("italic", &["SM/6:dc", "SM/6:QX"][..]),
                ("strike", &["SM/6:d9"][..]),
            ] {
                for key in keys {
                    for found in p.all(key, &text) {
                        if attribute == "link" {
                            let start = offset + found.range.start;
                            let end = start + found.range.len();
                            if intersects_endpoints(start, end, &code_ranges) {
                                continue;
                            }
                            let full = balanced_prefix(&text[found.range.clone()]);
                            let label = group(&found, &text, 1);
                            let address = balanced_prefix(group(&found, &text, 2));
                            let links = detect(address)?;
                            if links.len() != 1 || !valid_link(&links[0].text, address)? {
                                continue;
                            }
                            let label_start = start + 1;
                            let label_end = label_start + label.len();
                            let full_end = start + full.len();
                            let mut change = Change::default();
                            change.retain(label_start);
                            change.annotate(
                                label.len(),
                                Map::from_iter([("link".into(), json!(links[0].url))]),
                            );
                            document.apply(&change)?;
                            remove.push(label_start - 1);
                            remove.extend(label_end..full_end);
                            link_ranges.push((label_start, full_end));
                        } else {
                            let start = offset + found.range.start + group(&found, &text, 1).len();
                            let length = group(&found, &text, 2).len();
                            let end = start + length;
                            if link_ranges.iter().any(|(a, b)| start >= *a && end <= *b)
                                || attribute == "italic"
                                    && group(&found, &text, 2) == [40, 0x30c4, 41]
                            {
                                continue;
                            }
                            if attribute == "code" {
                                code_ranges.push((start, end));
                            } else if intersects_endpoints(start, end, &code_ranges) {
                                continue;
                            }
                            let mut change = Change::default();
                            change.retain(start);
                            change.annotate(
                                length,
                                Map::from_iter([(attribute.into(), json!(true))]),
                            );
                            document.apply(&change)?;
                            remove.push(start - 1);
                            remove.push(end);
                        }
                    }
                }
            }
        }
        offset += text.len() + 1;
    }
    remove.sort_unstable_by(|a, b| b.cmp(a));
    for at in remove {
        let mut change = Change::default();
        change.retain(at);
        change.delete(1);
        document.apply(&change)?;
    }
    Ok(continued_quote)
}
fn intersects_endpoints(start: usize, end: usize, ranges: &[(usize, usize)]) -> bool {
    ranges
        .iter()
        .any(|(a, b)| start >= *a && start <= *b || end >= *a && end <= *b)
}
fn balanced_prefix(text: &[u16]) -> &[u16] {
    let (mut opens, mut closes) = (0, 0);
    for (at, unit) in text.iter().enumerate() {
        if *unit == 40 {
            opens += 1;
        } else if *unit == 41 {
            closes += 1;
        }
        if closes > opens {
            return &text[..at];
        }
    }
    text
}
fn valid_link(detected: &[u16], input: &[u16]) -> Result<bool> {
    if detected == input {
        return Ok(true);
    }
    let input = String::from_utf16(input).map_err(|_| SeedUnavailable::InvalidUnicode)?;
    let detected = String::from_utf16(detected).map_err(|_| SeedUnavailable::InvalidUnicode)?;
    Ok(url::Url::parse(&input).is_ok_and(|url| url.scheme() == "mailto" && url.path() == detected))
}

// The native source's authored border is numeric. JavaScript also coerces
// truthy strings/arrays/bools in >=1; this bounded port refuses those shapes
// instead of silently substituting zero and changing quote continuation.
fn border_value(value: Option<&Value>) -> Result<f64> {
    if !truthy(value) {
        return Ok(0.0);
    }
    value
        .and_then(Value::as_f64)
        .ok_or(SeedUnavailable::MalformedMessage)
}
