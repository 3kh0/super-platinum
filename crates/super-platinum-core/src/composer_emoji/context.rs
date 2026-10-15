//! Markdown representation boundary from Slack hmcY/yzsb. This does not parse
//! rich Block Kit styles: the rich adapter must pass its inherited code/link
//! attributes separately. Grammar remains available inside these contexts.
pub fn in_markdown_code(prefix: &str) -> bool {
    if prefix.match_indices("```").count() % 2 != 0 {
        return true;
    }
    let mut remaining = prefix;
    let mut ticks = 0;
    while let Some(start) = remaining.find("```") {
        ticks += remaining[..start].bytes().filter(|b| *b == b'`').count();
        let after = &remaining[start + 3..];
        let Some(end) = after.find("```") else {
            return true;
        };
        remaining = &after[end + 3..];
    }
    ticks += remaining.bytes().filter(|b| *b == b'`').count();
    ticks % 2 != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_code_representation_oracle() {
        let oracle: serde_json::Value =
            serde_json::from_str(include_str!("source-oracle-next.json")).unwrap();
        for row in oracle["code"].as_array().unwrap() {
            assert_eq!(
                in_markdown_code(row["text"].as_str().unwrap()),
                row["result"].as_bool().unwrap(),
                "{row}"
            );
        }
    }

    #[test]
    fn source_does_not_reuse_mention_escape_or_matching_run_policy() {
        assert!(in_markdown_code("\\`"));
        assert!(!in_markdown_code("``"));
        assert!(in_markdown_code("````"));
        assert!(!in_markdown_code("```first\nsecond```"));
    }
}
