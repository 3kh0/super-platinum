//! Exact nonliteral empty reuse: Enter itself reports the mandatory clear.
use super::*;

/// Canonical unstyled original whitespace with zero or one exact BR.
/// Raw roundtrip equality prevents HTML repair/whitespace normalization from
/// turning a different original source into this bounded cohort.
pub(super) fn source_draft(section: &Section) -> Result<TextDraft, Error> {
    let draft = text::canonical_draft(section)?;
    let body = section
        .raw()
        .message(12)?
        .ok_or(Error::Unsupported)?
        .message(1)?
        .ok_or(Error::Unsupported)?;
    let raw = body.string(1)?.ok_or(Error::Unsupported)?;
    if raw.is_empty()
        || draft.rtml()? != raw
        || draft
            .runs
            .iter()
            .any(|r| r.style != super::super::super::text::TextStyle::default())
        || !source_tokens(raw)
    {
        return Err(Error::Unsupported);
    }
    Ok(draft)
}

// This is the roundtripping subset of the original qa+r.cw alphabet, not
// Unicode is_whitespace. Controls normalized by TextDraft remain excluded.
fn source_tokens(mut raw: &str) -> bool {
    let mut br = false;
    while !raw.is_empty() {
        if let Some(rest) = raw.strip_prefix("<br>") {
            if br {
                return false;
            }
            br = true;
            raw = rest;
        } else {
            let c = raw.chars().next().expect("nonempty remainder");
            if !matches!(
                c,
                '\u{20}' | '\u{a0}' | '\u{1680}' | '\u{2000}'
                    ..='\u{200b}'
                        | '\u{2028}'
                        | '\u{2029}'
                        | '\u{202f}'
                        | '\u{205f}'
                        | '\u{3000}'
                        | '\u{feff}'
            ) {
                return false;
            }
            raw = &raw[c.len_utf8()..];
        }
    }
    true
}

pub(super) fn reports(source: &Section, updates: &[TextDraft]) -> Result<Vec<TextDraft>, Error> {
    reuse::admit_clearing(source)?;
    typing::validate_updates(updates)?;
    // This is a source-derived CONTENT report, not an additional native input.
    let mut reports = Vec::with_capacity(updates.len() + 1);
    reports.push(TextDraft::default());
    reports.extend_from_slice(updates);
    Ok(reports)
}

pub(in crate::canvas::edit::structure) fn candidate<'a>(
    ordered: &[&'a Section],
    target: &str,
) -> Result<&'a Section, Error> {
    let child = reuse::candidate_current(ordered, target)?.ok_or(Error::Unsupported)?;
    reuse::admit_clearing(child)?;
    Ok(child)
}
