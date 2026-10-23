use super::*;

#[test]
fn actual_text_presentation_source_vectors_match() {
    let data: Value = serde_json::from_str(include_str!("vectors.json")).unwrap();
    for (index, row) in data["results"].as_array().unwrap().iter().enumerate() {
        let input = row["input"].as_array().unwrap();
        assert_eq!(
            json!(
                add_text_presentation_selectors(input, row["skipMarkdownCode"].as_bool().unwrap())
                    .unwrap()
            ),
            row["output"],
            "source {index}"
        );
    }
}

#[test]
fn existing_semantic_tokens_remain_exact() {
    for key in ["slackmention", "slackslug", "slacksearchfilter"] {
        for value in [
            json!(true),
            json!({"id":"U1","label":"\u{2194}","opaque":[false,3]}),
        ] {
            let input = vec![json!({"insert":"\u{2194}","attributes":{key:value}})];
            assert_eq!(
                add_text_presentation_selectors(&input, false).unwrap(),
                input
            );
            assert_eq!(
                add_text_presentation_selectors(&input, true).unwrap(),
                input
            );
        }
    }
}

#[test]
fn source_code_range_uses_ascii_triples_then_remaining_single_ticks() {
    for (text, expected) in [
        ("", false),
        ("`", true),
        ("``", false),
        ("```", true),
        ("``````", false),
        ("```a```x`", true),
        ("\u{ff40}", false),
        ("a```x```b", false),
    ] {
        assert_eq!(
            in_code(&text.encode_utf16().collect::<Vec<_>>()),
            expected,
            "{text}"
        );
    }
}
