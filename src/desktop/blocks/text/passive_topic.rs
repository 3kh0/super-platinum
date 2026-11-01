//! A native data adapter for passive topic text, with no resolver or action path.
//!
//! Keep the existing inline style grammar, then decode only five lower-case,
//! semicolon-terminated entities once. Code runs use the same entity policy:
//! decoded characters remain code text. No decoded output is parsed again.
//! This is not the complete Source TSF grammar or its entity/resource renderer.

/// Text and presentation flags only; no name, link, media or action authority.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TopicRun {
    pub(crate) text: String,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    pub(crate) strike: bool,
    pub(crate) code: bool,
}

pub(crate) fn passive_topic(text: &str) -> Vec<TopicRun> {
    // ASCII shortcodes are literal, including unknown workspace names. Shield
    // their underscores from the style grammar without resolving any emoji.
    let protected = protect_shortcodes(text);
    super::mrkdwn_spans(&protected, false)
        .into_iter()
        .map(|span| TopicRun {
            text: decode_entities(&restore_shortcodes(&span.text)),
            bold: span.bold,
            italic: span.italic,
            strike: span.strike,
            code: span.code,
        })
        .collect()
}

// Escape original occurrences too, so temporary scalars never alter user data.
const ESCAPE: char = '\u{f0000}';
const UNDERSCORE: char = '\u{f0001}';

fn push_literal(output: &mut String, ch: char) {
    if matches!(ch, ESCAPE | UNDERSCORE) {
        output.push(ESCAPE);
    }
    output.push(ch);
}

fn shortcode_end(text: &str, start: usize) -> Option<usize> {
    let mut has_name = false;
    for (offset, byte) in text.as_bytes()[start + 1..].iter().copied().enumerate() {
        if byte == b':' {
            return has_name.then_some(start + offset + 2);
        }
        if !byte.is_ascii_alphanumeric() && !matches!(byte, b'_' | b'+' | b'-') {
            return None;
        }
        has_name = true;
    }
    None
}

fn protect_shortcodes(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut index = 0;
    while index < text.len() {
        if text.as_bytes()[index] == b':'
            && let Some(end) = shortcode_end(text, index)
        {
            for ch in text[index..end].chars() {
                if ch == '_' {
                    output.push(UNDERSCORE);
                } else {
                    push_literal(&mut output, ch);
                }
            }
            index = end;
            continue;
        }
        let ch = text[index..].chars().next().unwrap();
        push_literal(&mut output, ch);
        index += ch.len_utf8();
    }
    output
}

fn restore_shortcodes(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        match ch {
            ESCAPE => output.push(chars.next().unwrap_or(ESCAPE)),
            UNDERSCORE => output.push('_'),
            other => output.push(other),
        }
    }
    output
}

fn decode_entities(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        output.push_str(&rest[..start]);
        let candidate = &rest[start..];
        let decoded = if candidate.starts_with("&amp;") {
            Some(('&', 5))
        } else if candidate.starts_with("&lt;") {
            Some(('<', 4))
        } else if candidate.starts_with("&gt;") {
            Some(('>', 4))
        } else if candidate.starts_with("&quot;") {
            Some(('"', 6))
        } else if candidate.starts_with("&apos;") {
            Some(('\'', 6))
        } else {
            None
        };
        if let Some((ch, length)) = decoded {
            output.push(ch);
            rest = &candidate[length..];
        } else {
            output.push('&');
            rest = &candidate[1..];
        }
    }
    output.push_str(rest);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str, bold: bool, italic: bool, strike: bool, code: bool) -> TopicRun {
        TopicRun {
            text: text.into(),
            bold,
            italic,
            strike,
            code,
        }
    }

    fn plain(text: &str) -> TopicRun {
        run(text, false, false, false, false)
    }

    #[test]
    fn passive_topic_styles_entities_and_opaque_tokens() {
        let opaque = concat!(
            "<@U1|*raw*> <#C1|_raw_> <!here> <!subteam^S1|@crew> ",
            "<!date^1^{date}|fallback> <https://example.test/a_b_c|*label*> ",
            ":custom_party_parrot: :unknown_emoji: :wave::skin-tone-3:"
        );
        let cases = [
            ("", vec![]),
            ("  plain\ntext  ", vec![plain("  plain\ntext  ")]),
            (
                "*bold* _italic_ ~strike~ `code*literal_~`",
                vec![
                    run("bold", true, false, false, false),
                    plain(" "),
                    run("italic", false, true, false, false),
                    plain(" "),
                    run("strike", false, false, true, false),
                    plain(" "),
                    run("code*literal_~", false, false, false, true),
                ],
            ),
            (
                "*bold _both_*",
                vec![
                    run("bold ", true, false, false, false),
                    run("both", true, true, false, false),
                ],
            ),
            ("stray * and _ marker", vec![plain("stray * and _ marker")]),
            ("*trailing *", vec![plain("*trailing *")]),
            (
                "A &amp; B &lt;tag&gt; &quot;x&quot; &apos;y&apos;",
                vec![plain("A & B <tag> \"x\" 'y'")],
            ),
            ("*A &amp; B*", vec![run("A & B", true, false, false, false)]),
            (
                "&amp;lt;script&amp;gt; &lt;b&gt;data&lt;/b&gt;",
                vec![plain("&lt;script&gt; <b>data</b>")],
            ),
            (
                "&#42; &#x1F600; &nbsp; &unknown; &AMP; &amp",
                vec![plain("&#42; &#x1F600; &nbsp; &unknown; &AMP; &amp")],
            ),
            (
                "`&lt;b&gt; &amp; &#95; *literal*`",
                vec![run("<b> & &#95; *literal*", false, false, false, true)],
            ),
            (opaque, vec![plain(opaque)]),
            (
                "*:custom_party_parrot:*",
                vec![run(":custom_party_parrot:", true, false, false, false)],
            ),
            (
                "👩🏽‍💻 中文 e\u{301} \u{f0000}\u{f0001}\u{f0000}",
                vec![plain("👩🏽‍💻 中文 e\u{301} \u{f0000}\u{f0001}\u{f0000}")],
            ),
        ];
        // The API yields only text plus style booleans, never a resolved token,
        // workspace lookup, link destination, media handle or action callback.
        let _: fn(&str) -> Vec<TopicRun> = passive_topic;
        for (input, expected) in cases {
            assert_eq!(passive_topic(input), expected, "input: {input:?}");
        }
    }
}
