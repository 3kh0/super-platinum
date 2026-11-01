//! Positive supplied-marker reading only; no query or shared-state inference.
use crate::state::RichNode;
const START: char = '\u{e000}';
const END: char = '\u{e001}';
const MAX_SPANS: usize = 128;

fn spans(text: &str) -> Option<Vec<(String, bool)>> {
    if !text.contains(START) {
        return None;
    }
    let mut result = Vec::new();
    let mut inside = false;
    let mut start = 0;
    for (at, ch) in text.char_indices() {
        if ch != START && ch != END {
            continue;
        }
        if ch == START && inside || ch == END && !inside {
            return None;
        }
        if at > start {
            result.push((text[start..at].into(), inside));
        }
        if result.len() > MAX_SPANS {
            return None;
        }
        inside = ch == START;
        start = at + ch.len_utf8();
    }
    if inside {
        return None;
    }
    if start < text.len() {
        result.push((text[start..].into(), false));
    }
    (result.len() <= MAX_SPANS && result.iter().any(|(_, matched)| *matched)).then_some(result)
}

/// Native policy supports balanced, nonnested pairs inside one text/styled leaf.
/// Raw API production, universal TSF normalization, cross-leaf repair, malformed
/// Source equivalence and standalone code emphasis are deliberately unclaimed.
pub(super) fn leaf(text: String, style: Option<[bool; 4]>) -> RichNode {
    let mut budget = MAX_SPANS;
    leaf_with_budget(text, style, &mut budget)
}

pub(super) fn leaf_with_budget(
    text: String,
    style: Option<[bool; 4]>,
    budget: &mut usize,
) -> RichNode {
    let [bold, italic, strike, code] = style.unwrap_or_default();
    let marked = if *budget == 0 { None } else { spans(&text) };
    if let Some(spans) = marked.filter(|spans| spans.len() <= *budget) {
        *budget -= spans.len();
        RichNode::SearchText {
            spans,
            bold,
            italic,
            strike,
            code,
        }
    } else {
        let text = text.replace([START, END], "");
        if style.is_some() {
            RichNode::StyledText {
                text,
                bold,
                italic,
                strike,
                code,
            }
        } else {
            RichNode::Text(text)
        }
    }
}

pub(crate) fn emphasis_fixture_body() -> Vec<RichNode> {
    vec![RichNode::Paragraph(vec![leaf(
        "Review the \u{e000}release\u{e001} notes in their indexed context.".into(),
        Some([true, false, false, false]),
    )])]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn supplied_pairs_preserve_styles_plain_unicode_and_safe_bounded_malformed_fallback() {
        let raw = "Before \u{e000}release 🚀\u{e001} and \u{e000}notes\u{e001}.";
        let node = leaf(raw.into(), Some([true, true, true, false]));
        let RichNode::SearchText {
            spans,
            bold,
            italic,
            strike,
            code,
        } = &node
        else {
            panic!()
        };
        assert!(*bold && *italic && *strike && !*code);
        assert_eq!(
            spans
                .iter()
                .filter(|(_, m)| *m)
                .map(|(s, _)| s.as_str())
                .collect::<Vec<_>>(),
            ["release 🚀", "notes"]
        );
        assert_eq!(node.plain_text(), "Before release 🚀 and notes.");
        assert_eq!(
            crate::message_vm::preview::nodes(vec![node.clone()]),
            vec![node]
        );
        for raw in [
            "plain",
            "\u{e000}open",
            "close\u{e001}",
            "\u{e000}outer\u{e000}inner\u{e001}\u{e001}",
            "\u{e000}\u{e001}",
            "<HIGHLIGHT:START>literal<HIGHLIGHT:END>",
        ] {
            let node = leaf(raw.into(), None);
            assert!(matches!(node, RichNode::Text(_)));
            assert_eq!(node.plain_text(), raw.replace([START, END], ""));
        }
        let raw = (0..MAX_SPANS + 1)
            .map(|_| "a\u{e000}b\u{e001}")
            .collect::<String>();
        let node = leaf(raw.clone(), Some([false; 4]));
        assert!(matches!(node, RichNode::StyledText { .. }));
        assert_eq!(node.plain_text(), raw.replace([START, END], ""));
        let html = leaf("\u{e000}<script>visible text</script>\u{e001}".into(), None);
        assert_eq!(html.plain_text(), "<script>visible text</script>");
        assert!(matches!(html, RichNode::SearchText { .. }));
        let input = (0..300)
            .map(|_| RichNode::Text("a\u{e000}b\u{e001}".into()))
            .collect();
        let output = super::super::body::clean_nodes(input);
        assert_eq!(
            output.iter().map(RichNode::plain_text).collect::<String>(),
            "ab".repeat(300)
        );
        assert!(
            output
                .iter()
                .map(|node| match node {
                    RichNode::SearchText { spans, .. } => spans.len(),
                    _ => 0,
                })
                .sum::<usize>()
                <= 256
        );
        assert!(output.iter().any(|node| matches!(node, RichNode::Text(_))));
    }
}
