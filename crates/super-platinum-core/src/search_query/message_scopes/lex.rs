//! Opaque reference labels cannot expose nested modifier-looking text.
use std::ops::Range;
#[derive(Debug, Clone)]
pub(super) struct Lex {
    pub tokens: Vec<Range<usize>>,
    pub incomplete_reference: bool,
    pub unfinished_quote: bool,
}
pub(super) fn scan(query: &str) -> Lex {
    let mut tokens = Vec::new();
    let (mut start, mut quoted, mut escaped, mut reference) = (None, false, false, false);
    for (index, ch) in query.char_indices() {
        if ch.is_whitespace() && !quoted && !reference {
            if let Some(start) = start.take() {
                tokens.push(start..index);
            }
            escaped = false;
            continue;
        }
        let beginning = *start.get_or_insert(index);
        if reference {
            if ch == '>' {
                reference = false;
            }
        } else if ch == '"' && !escaped {
            quoted = !quoted;
        } else if ch == '<' && !quoted && query[beginning..index].contains(':') {
            // This is lexical protection, not recognition or resolution. It also
            // keeps unknown/localized/negative references opaque during edits.
            reference = true;
        }
        escaped = ch == '\\' && !escaped;
    }
    if let Some(start) = start {
        tokens.push(start..query.len());
    }
    Lex {
        tokens,
        incomplete_reference: reference,
        unfinished_quote: quoted,
    }
}
