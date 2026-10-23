//! Actual l8g0 linkification used before and after plain preparation.
use super::*;
use crate::slack_lists::cell_edit::link::{MessageLinkOptions, detect_message_links};
use document::Unit;

pub(super) fn detect(
    text: &[u16],
    options: MessageLinkOptions,
) -> Result<Vec<crate::slack_lists::cell_edit::link::MessageLink>> {
    let text = String::from_utf16(text).map_err(|_| SeedUnavailable::InvalidUnicode)?;
    detect_message_links(&text, options).map_err(|_| SeedUnavailable::MalformedMessage)
}

pub(super) fn linkify(document: &mut Document, final_pass: bool) -> Result<()> {
    let had_newline = document.ends_newline();
    let mut output = Document::default();
    for (_, mut line, newline_attributes) in document.lines() {
        line.0
            .push(Unit::new(Insert::Unit(10), newline_attributes.clone()));
        let code_block = truthy(newline_attributes.get("code-block"));
        for op in line.ops()? {
            let attributes = op
                .get("attributes")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let Some(text) = op["insert"].as_str() else {
                output.0.extend(Document::capture(&[op])?.0);
                continue;
            };
            // l8g0 short-circuits iRqW before inspecting link metadata.
            if ["slackmention", "slackslug", "slacksearchfilter"]
                .iter()
                .any(|key| truthy(attributes.get(*key)))
            {
                output.0.extend(Document::capture(&[op])?.0);
                continue;
            }
            let linked = truthy(attributes.get("link"));
            if final_pass && linked || truthy(attributes.get("unlink")) {
                output.0.extend(Document::capture(&[op])?.0);
                continue;
            }
            let link = attributes.get("link").and_then(Value::as_str);
            if linked && link.is_none() {
                return Err(SeedUnavailable::MalformedMessage);
            }
            let skip = link.is_some_and(|url| truncated(text, url));
            let manual = if let Some(url) = link {
                !fragment(text, url) && {
                    let found = detect(
                        &text.encode_utf16().collect::<Vec<_>>(),
                        MessageLinkOptions::default(),
                    )?;
                    found.len() != 1 || found[0].text != text
                }
            } else {
                false
            };
            if skip || manual {
                output.0.extend(Document::capture(&[op])?.0);
                continue;
            }
            let text: Vec<u16> = text.encode_utf16().collect();
            let limited = final_pass && code_block;
            let found = detect(
                &text,
                MessageLinkOptions {
                    ignore_phone_matches: limited,
                    ignore_email_matches: limited,
                    ignore_links_without_http: limited,
                    ignore_unbalanced_parentheses: !final_pass,
                    ..Default::default()
                },
            )?;
            let original = Document::capture(&[op])?;
            let mut at = 0;
            for found in found {
                let start = found.offset_utf16;
                let end = start
                    .checked_add(found.text.encode_utf16().count())
                    .ok_or(SeedUnavailable::Limit)?;
                if start < at || end > text.len() {
                    return Err(SeedUnavailable::MalformedMessage);
                }
                output.0.extend(original.slice(at, start)?.0);
                let mut part = original.slice(start, end)?;
                for unit in &mut part.0 {
                    unit.attributes.insert("link".into(), json!(found.url));
                }
                output.0.extend(part.0);
                at = end;
            }
            output.0.extend(original.slice(at, original.len())?.0);
        }
    }
    // CnpR removes only a terminal newline without any of these own keys.
    if !had_newline
        && output.0.last().is_some_and(|unit| {
            unit.insert == Insert::Unit(10)
                && !["code-block", "blockquote", "list"]
                    .iter()
                    .any(|key| unit.attributes.contains_key(*key))
        })
    {
        output.0.pop();
    }
    if output.len() > 262_144 {
        return Err(SeedUnavailable::Limit);
    }
    output.normalize_insert_attributes();
    *document = output;
    Ok(())
}
fn fragment(text: &str, url: &str) -> bool {
    let address = url.find(':').map_or(url, |at| {
        let after = &url[at + 1..];
        after.strip_prefix("//").unwrap_or(after)
    });
    !text.is_empty()
        && text.encode_utf16().count() < address.encode_utf16().count()
        && (address.starts_with(text) || address.ends_with(text))
}
fn truncated(text: &str, url: &str) -> bool {
    let pieces: Vec<_> = text.split("[\u{2026}]").collect();
    pieces.len() == 2
        && !pieces[0].is_empty()
        && !pieces[1].is_empty()
        && url
            .strip_prefix(pieces[0])
            .is_some_and(|rest| rest.contains(pieces[1]))
}

#[cfg(test)]
mod token_tests;
