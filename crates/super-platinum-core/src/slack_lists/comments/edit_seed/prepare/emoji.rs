//! bm2k on the exact rNCs path: inline/WYSIWYG on, native replacement off,
//! no explicit teamId. Positive catalog/translation facts remain host-owned.
use super::*;
use document::Unit;
mod limits;
mod observations;
mod patterns;
mod standard;
pub use observations::{EmojiObservations, EmojiTranslation, ToneSupport};
pub use standard::StandardEmoji;
#[cfg(test)]
mod tests;

/// Exact existing ue25 parsed name; this grants no catalog membership.
pub fn emoji_name(text: &str) -> Result<Option<String>> {
    Ok(observations::parse(text)?.map(|parsed| parsed.name))
}

pub fn replace_colon_emoji(
    ops: &[Value],
    plain: bool,
    observed: &EmojiObservations,
) -> Result<Vec<Value>> {
    limits::check(ops)?;
    let mut document = Document::capture(ops)?;
    let p = patterns::get()?;
    let mut change = Change::default();
    let mut offset = 0usize;
    let mut last_match_end = 0usize;
    let mut previous: Option<Value> = None;
    let mut prefix_text = None;
    let mut count = 0;
    let mut budget = limits::Budget::default();
    let mut tone_index = None;
    for (_, mut line, newline) in document.lines() {
        line.0.push(Unit::new(Insert::Unit(10), newline.clone()));
        for op in line.ops()? {
            let length = op["insert"]
                .as_str()
                .map_or(1, |text| text.encode_utf16().count());
            let attrs = op
                .get("attributes")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let skip = truthy(newline.get("code-block"))
                || [
                    "code",
                    "link",
                    "slackmention",
                    "slackslug",
                    "slacksearchfilter",
                ]
                .iter()
                .any(|key| truthy(attrs.get(*key)));
            if skip || !op["insert"].is_string() {
                offset += length;
                previous = Some(op);
                continue;
            }
            let text = op["insert"].as_str().unwrap();
            let text = if observed.fold_fullwidth()? {
                text.replace('\u{ff1a}', ":")
            } else {
                text.to_owned()
            };
            let units: Vec<_> = text.encode_utf16().collect();
            let urls = if plain {
                p.url
                    .find_from_ucs2(&units, 0)
                    .map(|m| m.range)
                    .collect::<Vec<_>>()
            } else {
                vec![]
            };
            let ipv6 = p
                .ipv6
                .find_from_ucs2(&units, 0)
                .map(|m| m.range)
                .collect::<Vec<_>>();
            let mut matches = Vec::new();
            for found in p.emoji.find_from_ucs2(&units, 0) {
                let index = found.range.start;
                if plain && urls.iter().any(|r| index >= r.start && index <= r.end) {
                    continue;
                }
                if plain {
                    if prefix_text.is_none() {
                        prefix_text = Some(document.text(false, false, 32)?)
                    }
                    let prefix = prefix_text.as_ref().unwrap();
                    if patterns::in_code(&prefix[..(offset + index).min(prefix.len())]) {
                        continue;
                    }
                }
                if ipv6.iter().any(|r| index >= r.start && index <= r.end) {
                    continue;
                }
                let raw = String::from_utf16(&units[found.range.clone()])
                    .map_err(|_| SeedUnavailable::InvalidUnicode)?;
                let Some(parsed) = observed.canonical(&raw)? else {
                    continue;
                };
                budget.replacement(&parsed.name, &parsed.tone, &attrs)?;
                let replacement = parsed.text();
                if numeric_shortcode(&replacement) && preceding_number(&units[..index]) {
                    continue;
                }
                if count >= 32_768 {
                    return Err(SeedUnavailable::Limit);
                }
                count += 1;
                matches.push((index, found.range.len(), replacement));
            }
            let mut previous_index = 0;
            let mut previous_length = 0;
            for (ordinal, (index, length, mut replacement)) in matches.into_iter().enumerate() {
                let mut distance = if ordinal == 0 {
                    (offset + index) as isize - last_match_end as isize
                } else {
                    index as isize - previous_index as isize - previous_length as isize
                };
                let mut remove = length;
                let mut merged = false;
                let replacement_units: Vec<_> = replacement.encode_utf16().collect();
                let modifier = p.skin.find_from_ucs2(&replacement_units, 0).next();
                let tones = modifier.as_ref().map(|m| {
                    let first = m
                        .group(2)
                        .and_then(|r| replacement_units.get(r.start))
                        .copied();
                    let second = m
                        .group(3)
                        .and_then(|r| replacement_units.get(r.start))
                        .copied();
                    (first, second)
                });
                if ordinal == 0 && index == 0 && modifier.is_some() {
                    if let Some(emoji) = previous
                        .as_ref()
                        .and_then(|p| p.get("insert"))
                        .and_then(|i| i.get("slackemoji"))
                        .filter(|v| truthy(Some(v)))
                    {
                        let raw = match emoji.get("text") {
                            None | Some(Value::Null) | Some(Value::Bool(false)) => "",
                            Some(Value::String(s)) => s,
                            Some(Value::Number(n)) if n.as_f64() == Some(0.0) => "",
                            _ => return Err(SeedUnavailable::MalformedMessage),
                        };
                        let preceding = observations::parse(raw)?;
                        let name = preceding.as_ref().map(|p| p.name.as_str()).unwrap_or("");
                        let (first, second) = tones.unwrap();
                        let query = if second.is_none() {
                            Some(false)
                        } else if first != second {
                            Some(true)
                        } else {
                            None
                        };
                        let support = if let Some(multiple) = query {
                            if tone_index.is_none() {
                                tone_index =
                                    Some(observations::ToneIndex::capture(&observed.tones)?);
                            }
                            tone_index.as_ref().unwrap().get(name, multiple)?
                        } else {
                            false
                        };
                        if support {
                            distance -= 1;
                            remove += 1;
                            replacement = format!(
                                ":{}:{replacement}",
                                preceding
                                    .as_ref()
                                    .map(|p| p.name.as_str())
                                    .unwrap_or("undefined")
                            );
                            merged = true
                        }
                    }
                }
                previous_index = index;
                previous_length = length;
                last_match_end = offset + index + length;
                if merged || tones.is_none_or(|(_, second)| second.is_none()) {
                    if distance > 0 {
                        change.retain(distance as usize)
                    }
                    change.delete(remove);
                    let mut insert = json!({"insert":{"slackemoji":{"text":replacement}}});
                    if !attrs.is_empty() {
                        insert["attributes"] = Value::Object(attrs.clone())
                    }
                    change.insert_document(Document::capture(&[insert])?);
                }
            }
            offset += length;
            previous = Some(op);
        }
    }
    document.apply(&change)?;
    let output = document.ops()?;
    limits::check(&output)?;
    Ok(output)
}
fn numeric_shortcode(value: &str) -> bool {
    value
        .strip_prefix(':')
        .and_then(|s| s.strip_suffix(':'))
        .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
}
fn preceding_number(prefix: &[u16]) -> bool {
    let start = prefix
        .iter()
        .rposition(|c| !matches!(c, 48..=57))
        .map_or(0, |i| i + 1);
    start < prefix.len()
        && (start == 0
            || char::from_u32(prefix[start - 1] as u32).is_some_and(patterns::whitespace))
}
