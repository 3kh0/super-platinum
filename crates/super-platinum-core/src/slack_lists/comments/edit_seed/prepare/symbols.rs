//! nC51 text-presentation selection. The caller owns the positively observed
//! composer_smart_symbols/disable_smart_symbols gate; this pure operation does
//! not infer a preference or enable an editor mode.
use super::*;
use document::Unit;
#[cfg(test)]
mod tests;

pub fn add_text_presentation_selectors(
    ops: &[Value],
    skip_markdown_code: bool,
) -> Result<Vec<Value>> {
    let mut document = Document::capture(ops)?;
    let mut change = Change::default();
    let mut last = 0;
    let mut previous = Vec::new();
    for (start, mut line, newline) in document.lines() {
        let block_code = truthy(newline.get("code-block"));
        line.0.push(Unit::new(Insert::Unit(10), newline));
        let mut at = start;
        for op in line.ops()? {
            let attributes = op
                .get("attributes")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            if let Some(text) = op["insert"].as_str() {
                let text: Vec<_> = text.encode_utf16().collect();
                let inline_code = truthy(attributes.get("code"));
                let token = ["slackmention", "slackslug", "slacksearchfilter"]
                    .iter()
                    .any(|key| truthy(attributes.get(*key)));
                for index in 0..text.len() {
                    if !block_code
                        && !inline_code
                        && !token
                        && text[index] == 0x2194
                        && index.checked_sub(1).is_none_or(|i| text[i] != 0x200d)
                        && text
                            .get(index + 1)
                            .is_none_or(|c| !matches!(c, 0xfe0e | 0xfe0f))
                    {
                        let markdown = skip_markdown_code && {
                            let mut prefix = previous.clone();
                            prefix.extend_from_slice(&text[..index]);
                            in_code(&prefix)
                        };
                        if !markdown {
                            let end = at + index + 1;
                            change.retain(end - last);
                            change.insert("\u{fe0e}", attributes.clone());
                            last = end;
                        }
                    }
                }
                at += text.len();
                previous.extend_from_slice(&text);
            } else {
                at += 1;
                previous.push(0xfffc);
            }
        }
    }
    document.apply(&change)?;
    document.ops()
}

/// Exact yzsb then hmcY: count non-overlapping ASCII triple backticks, remove
/// closed triple-backtick ranges, then test odd remaining single backticks.
fn in_code(text: &[u16]) -> bool {
    let mut triples = Vec::new();
    let mut at = 0;
    while at + 3 <= text.len() {
        if text[at..at + 3] == [96, 96, 96] {
            triples.push(at);
            at += 3;
        } else {
            at += 1;
        }
    }
    if triples.len() % 2 == 1 {
        return true;
    }
    let mut ticks = 0;
    let mut previous = 0;
    for pair in triples.chunks_exact(2) {
        ticks += text[previous..pair[0]].iter().filter(|c| **c == 96).count();
        previous = pair[1] + 3;
    }
    ticks += text[previous..].iter().filter(|c| **c == 96).count();
    ticks % 2 == 1
}
