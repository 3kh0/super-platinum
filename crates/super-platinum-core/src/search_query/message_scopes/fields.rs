//! Existing native nonidentity keys, projected only outside opaque references.
use super::*;
const TEXT_KEYS: [&str; 4] = ["with", "on", "after", "before"];
const FLAGS: [&str; 5] = ["has:file", "has:link", "is:thread", "is:saved", "has:pin"];
fn text_values(s: &SearchFilters) -> [&str; 4] {
    [&s.with_person, &s.on, &s.after, &s.before]
}
fn flags(s: &SearchFilters) -> [bool; 5] {
    [s.has_file, s.has_link, s.in_thread, s.saved, s.pinned]
}
pub(super) fn project(query: &str, lex: &lex::Lex) -> SearchFilters {
    let get = |key: &str| {
        lex.tokens
            .iter()
            .find_map(|r| {
                query[r.clone()]
                    .strip_prefix(&format!("{key}:"))
                    .map(|s| s.trim_matches('"').to_owned())
            })
            .unwrap_or_default()
    };
    let has = |value: &str| lex.tokens.iter().any(|r| &query[r.clone()] == value);
    SearchFilters {
        with_person: get("with"),
        on: get("on"),
        after: get("after"),
        before: get("before"),
        has_file: has(FLAGS[0]),
        has_link: has(FLAGS[1]),
        in_thread: has(FLAGS[2]),
        saved: has(FLAGS[3]),
        pinned: has(FLAGS[4]),
        ..Default::default()
    }
}
pub(super) fn edits(
    query: &str,
    lex: &lex::Lex,
    baseline: &SearchFilters,
    fields: &SearchFilters,
) -> Result<(Vec<Range<usize>>, Vec<String>), &'static str> {
    if !fields.channel.is_empty() || !fields.sender.is_empty() {
        return Err("Use the channel and author choices for these filters.");
    }
    let mut remove = Vec::new();
    let mut append = Vec::new();
    for (index, ((value, old), key)) in text_values(fields)
        .into_iter()
        .zip(text_values(baseline))
        .zip(TEXT_KEYS)
        .enumerate()
    {
        if value == old {
            continue;
        }
        let value = value.trim();
        if index > 0
            && !value.is_empty()
            && chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").is_err()
        {
            return Err("Use YYYY-MM-DD for dates.");
        }
        if value.contains(['<', '>', '"', '\\', '\n', '\r']) {
            return Err("Use a plain name or date without brackets, quotes or line breaks.");
        }
        let prefix = format!("{key}:");
        remove.extend(
            lex.tokens
                .iter()
                .filter(|r| query[(*r).clone()].starts_with(&prefix))
                .cloned(),
        );
        if !value.is_empty() {
            let value = if value.chars().any(char::is_whitespace) {
                format!("\"{value}\"")
            } else {
                value.to_owned()
            };
            append.push(format!("{key}:{value}"));
        }
    }
    for ((enabled, old), token) in flags(fields).into_iter().zip(flags(baseline)).zip(FLAGS) {
        if enabled == old {
            continue;
        }
        if enabled {
            append.push(token.into());
        } else {
            remove.extend(
                lex.tokens
                    .iter()
                    .filter(|r| &query[(*r).clone()] == token)
                    .cloned(),
            );
        }
    }
    Ok((remove, append))
}
