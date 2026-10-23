use super::*;
use serde_json::json;

fn text(value: &str) -> Value {
    json!({"type":"text","text":value})
}
fn styled(value: &str, style: Value) -> Value {
    json!({"type":"text","text":value,"style":style})
}
fn section(elements: Vec<Value>) -> Value {
    json!({"type":"rich_text_section","elements":elements})
}
fn document(elements: Vec<Value>) -> Vec<Value> {
    vec![json!({"type":"rich_text","block_id":"original","elements":elements})]
}
fn one(elements: Vec<Value>) -> Vec<Value> {
    document(vec![section(elements)])
}

#[test]
fn unicode_segmentation_with_identical_styles_is_symmetric_and_keeps_inputs() {
    for style in [
        None,
        Some(json!({})),
        Some(json!({"bold":true,"code":false})),
    ] {
        let leaf = |value: &str| match &style {
            Some(style) => styled(value, style.clone()),
            None => text(value),
        };
        let original = one(vec![leaf("caf\u{e9} \u{4e2d}\u{1f642} e\u{301}")]);
        let edited = one(vec![
            leaf("caf"),
            leaf("\u{e9} "),
            leaf("\u{4e2d}\u{1f642} e"),
            leaf("\u{301}"),
        ]);
        let before = serde_json::to_vec(&(&original, &edited)).unwrap();
        assert_ne!(original, edited);
        assert!(unchanged(&original, &edited));
        assert!(unchanged(&edited, &original));
        assert_eq!(serde_json::to_vec(&(&original, &edited)).unwrap(), before);
    }
}

#[test]
fn styles_require_exact_representation_and_supported_boolean_keys() {
    for (left, right) in [
        (text("ab"), styled("ab", json!({}))),
        (text("ab"), styled("ab", json!({"bold":false}))),
        (
            styled("ab", json!({"bold":true})),
            styled("ab", json!({"italic":true})),
        ),
    ] {
        assert!(!unchanged(&one(vec![left]), &one(vec![right])));
    }
    for style in [
        json!({"underline":true}),
        json!({"future":true}),
        json!({"bold":1}),
        Value::Null,
    ] {
        assert!(!unchanged(
            &one(vec![styled("ab", style.clone())]),
            &one(vec![styled("a", style.clone()), styled("b", style)]),
        ));
    }
    assert!(!unchanged(
        &one(vec![text("ab")]),
        &one(vec![text("a"), styled("b", json!({"bold":true}))]),
    ));
}

#[test]
fn opaque_text_and_semantic_tokens_are_exact_barriers() {
    let mut opaque = text("ab");
    opaque["opaque"] = json!({"original":7});
    let mut a = opaque.clone();
    a["text"] = json!("a");
    let mut b = opaque.clone();
    b["text"] = json!("b");
    assert!(!unchanged(&one(vec![opaque.clone()]), &one(vec![a, b])));
    let mut replaced = opaque.clone();
    replaced["opaque"]["original"] = json!(8);
    assert!(!unchanged(&one(vec![opaque]), &one(vec![replaced])));

    for token in [
        json!({"type":"user","user_id":"U1","opaque":7}),
        json!({"type":"canvas","file_id":"F1","text":"Canvas","opaque":7}),
        json!({"type":"link","url":"https://example.test","text":"Link","opaque":7}),
    ] {
        let original = one(vec![text("ab"), token.clone(), text("cd")]);
        let edited = one(vec![
            text("a"),
            text("b"),
            token.clone(),
            text("c"),
            text("d"),
        ]);
        assert!(unchanged(&original, &edited));
        let mut changed = edited.clone();
        changed[0]["elements"][0]["elements"][2]["opaque"] = json!(8);
        assert!(!unchanged(&original, &changed));
        assert!(!unchanged(&original, &one(vec![text("abcd"), token])));
    }
}

#[test]
fn empty_newline_quote_and_preformatted_runs_are_not_folded() {
    for parts in [
        vec!["a", "\nb"],
        vec!["a\r", "b"],
        vec!["", "ab"],
        vec!["ab", "\n"],
    ] {
        assert!(!unchanged(
            &one(vec![text(&parts.concat())]),
            &one(parts.into_iter().map(text).collect()),
        ));
    }
    assert!(!unchanged(&one(vec![text("ab")]), &one(vec![text("ab\n")])));
    for kind in ["rich_text_quote", "rich_text_preformatted"] {
        assert!(!unchanged(
            &document(vec![json!({"type":kind,"elements":[text("ab")]})]),
            &document(vec![json!({"type":kind,"elements":[text("a"),text("b")]})]),
        ));
    }
}

#[test]
fn sections_blocks_and_all_container_metadata_stay_exact() {
    let original = one(vec![text("ab")]);
    let edited = one(vec![text("a"), text("b")]);
    assert!(unchanged(&original, &edited));
    for path in ["block_id", "opaque"] {
        let mut changed = edited.clone();
        changed[0][path] = json!("replaced");
        assert!(!unchanged(&original, &changed));
    }
    let mut changed = edited;
    changed[0]["elements"][0]["opaque"] = json!({"text":"ab"});
    assert!(!unchanged(&original, &changed));
    assert!(!unchanged(
        &original,
        &document(vec![section(vec![text("a")]), section(vec![text("b")])])
    ));
    let mut two_blocks = one(vec![text("a")]);
    two_blocks.extend(one(vec![text("b")]));
    assert!(!unchanged(&original, &two_blocks));
}

#[test]
fn list_sections_fold_locally_without_changing_list_shape_or_invalid_content() {
    let list = |items: Vec<Value>| {
        document(vec![
            json!({"type":"rich_text_list","style":"ordered","indent":1,"offset":2,"opaque":9,"elements":items}),
        ])
    };
    let original = list(vec![section(vec![text("ab")]), section(vec![text("cd")])]);
    let edited = list(vec![
        section(vec![text("a"), text("b")]),
        section(vec![text("c"), text("d")]),
    ]);
    assert!(unchanged(&original, &edited));
    assert!(!unchanged(
        &original,
        &list(vec![section(vec![text("abcd")])])
    ));
    let mut changed = edited;
    changed[0]["elements"][0]["offset"] = json!(3);
    assert!(!unchanged(&original, &changed));
    assert!(!unchanged(
        &original,
        &one(vec![json!({"type":"text","text":1})])
    ));
    assert!(!unchanged(
        &original,
        &one(vec![json!({"type":"future","text":"ab"})])
    ));
}
