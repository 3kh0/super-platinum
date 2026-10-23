use super::super::{MessageLink, MessageLinkKind, MessageLinkOptions};
use super::*;

pub(crate) fn detect(
    input: &str,
    options: MessageLinkOptions,
) -> Result<Vec<MessageLink>, ListError> {
    let input = text(input);
    if input.len() > 262_144 {
        return Err(ListError("Message link input is too large."));
    }
    let p = whole::patterns()?;
    let mut hits: Vec<_> = urls::detect_with(
        p,
        &input,
        options.ignore_links_without_http,
        options.ignore_unbalanced_parentheses,
        false,
    )?
    .into_iter()
    .map(|hit| (MessageLinkKind::Url, hit))
    .collect();
    if !options.ignore_email_matches {
        merge_family(&mut hits, emails(p, &input), MessageLinkKind::Email);
    }
    if !options.ignore_phone_matches {
        let phones = if options.include_phone_matches_with_tel_prefix {
            phones_with_prefix(p, &input)?
        } else {
            phones(p, &input)
        };
        merge_family(&mut hits, phones, MessageLinkKind::Phone);
    }
    hits.sort_by_key(|(_, hit)| hit.offset);
    hits.into_iter()
        .map(|(kind, hit)| {
            Ok(MessageLink {
                kind,
                offset_utf16: hit.offset,
                text: string(&hit.text)?,
                url: string(&hit.url)?,
            })
        })
        .collect()
}
fn merge_family(current: &mut Vec<(MessageLinkKind, Hit)>, next: Vec<Hit>, kind: MessageLinkKind) {
    // v+tG filters against only the previously accepted families, not siblings.
    let accepted: Vec<_> = next
        .into_iter()
        .filter(|candidate| {
            !current.iter().any(|(_, old)| {
                let (a, b) = (candidate.offset, candidate.offset + candidate.text.len());
                let (c, d) = (old.offset, old.offset + old.text.len());
                a == c || b == d || a <= c && b >= d || a >= c && a < d || b >= c && b < d
            })
        })
        .collect();
    current.extend(accepted.into_iter().map(|hit| (kind, hit)));
}
fn phone_pattern() -> Result<&'static Regex, ListError> {
    static PATTERN: OnceLock<Result<Regex, ListError>> = OnceLock::new();
    PATTERN
        .get_or_init(|| {
            let value: serde_json::Value = serde_json::from_str(include_str!("patterns.json"))
                .map_err(|_| ListError("Link matching data is unavailable."))?;
            let phone = value["phone"]["source"]
                .as_str()
                .ok_or(ListError("Link matching data is unavailable."))?;
            Regex::from_unicode(
                non_unicode_pattern(&format!("tel:({phone})|{phone}")).into_iter(),
                "g",
            )
            .map_err(|_| ListError("Link matching patterns are unavailable."))
        })
        .as_ref()
        .map_err(Clone::clone)
}
fn phones_with_prefix(p: &Patterns, input: &[u16]) -> Result<Vec<Hit>, ListError> {
    let mut hits = vec![];
    for m in phone_pattern()?.find_from_ucs2(input, 0) {
        let original = &input[m.range()];
        if starts(original, "tel:") {
            hits.push(Hit {
                offset: m.start(),
                text: original.to_vec(),
                url: original.to_vec(),
            });
            continue;
        }
        if original
            .iter()
            .all(|c| (b'0' as u16..=b'9' as u16).contains(c))
        {
            continue;
        }
        let mut value = original.to_vec();
        let mut offset = m.start();
        if starts(&value, "(") && !contains(&value, ")") {
            value.remove(0);
            offset += 1;
        }
        // Preserve xt89's actual capture numbering under its combined regex,
        // including the non-prefixed branch's empty group1/group2 values.
        let plus = truthy(group(&m, input, 1)) || truthy(group(&m, input, 2));
        let mut url = text(if plus { "tel:+" } else { "tel:" });
        url.extend(
            original
                .iter()
                .copied()
                .filter(|c| (b'0' as u16..=b'9' as u16).contains(c)),
        );
        let before = offset.checked_sub(1).and_then(|i| input.get(i)).copied();
        let end = offset + value.len();
        let after = input.get(end).copied();
        let left = offset == 0
            || before.is_some_and(|c| text(" \u{3000}\n([{'\"").contains(&c))
            || p.cjk(before);
        let right = end == input.len()
            || after.is_some_and(|c| text(" \u{3000}\n.)]}'\":?!;,").contains(&c))
            || p.cjk(after);
        if left && right {
            hits.push(Hit {
                offset,
                text: value,
                url,
            });
        }
    }
    Ok(hits)
}

#[cfg(test)]
mod tests;
