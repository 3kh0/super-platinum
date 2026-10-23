use super::*;

#[test]
fn actual_linkifier_skips_existing_tokens_before_inspecting_links() {
    let vectors: Value = serde_json::from_str(include_str!("token-vectors.json")).unwrap();
    for (index, case) in vectors["results"].as_array().unwrap().iter().enumerate() {
        let input = case["input"].as_array().unwrap();
        let retained = input.clone();
        for (name, final_pass) in [("first", false), ("final", true)] {
            let mut document = Document::capture(input).unwrap();
            let result = linkify(&mut document, final_pass);
            if case.get(format!("{name}Error")).is_some() {
                assert!(result.is_err(), "source refusal {index}/{name}");
            } else {
                result.unwrap();
                assert_eq!(json!(document.ops().unwrap()), case[name], "{index}/{name}");
            }
        }
        assert_eq!(input, &retained);
    }
}

#[test]
fn skipped_token_does_not_disable_linkification_of_following_text() {
    let input = vec![
        json!({"insert":"https://example.com","attributes":{"slackmention":true,"link":17}}),
        json!({"insert":" https://example.org"}),
    ];
    let mut document = Document::capture(&input).unwrap();
    linkify(&mut document, false).unwrap();
    let output = document.ops().unwrap();
    assert_eq!(output[0], input[0]);
    assert_eq!(
        output.last().unwrap()["attributes"]["link"],
        "https://example.org"
    );
}
