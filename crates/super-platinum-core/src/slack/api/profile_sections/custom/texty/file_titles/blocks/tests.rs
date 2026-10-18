use super::*;
use crate::slack::api::profile_sections::custom::texty::file_titles::{
    Kind, Status, label as legacy_label, label_with_quip,
};
use serde_json::json;

fn raw(text: &str) -> Value {
    json!({"title":"ignored", "opaque":[null,1], "title_blocks":[
    {"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
        {"type":"text","text":text,"style":{"bold":true,"unknown":[false,7]}}
    ]}]}]})
}

#[test]
fn original_block_override_vectors() {
    let oracle: Value = serde_json::from_str(include_str!("oracle.json")).unwrap();
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 128);
    for case in cases {
        let quip = match case["override"].as_str() {
            Some(text) => QuipTitle::Text(text),
            None => QuipTitle::Nullish,
        };
        let before = case["raw"].clone();
        let actual = label_with_quip(Kind::Canvas, Status::Normal, &case["raw"], quip);
        let expected = if case["error"] == "LocalizedFallback" {
            Err(Unavailable::LocalizedFallback)
        } else {
            Ok(case["label"].as_str().unwrap().to_owned())
        };
        assert_eq!(actual, expected, "{case}");
        assert_eq!(before, case["raw"]);
    }
}

#[test]
fn original_nonglobal_colon_prefix_vectors() {
    let oracle: Value = serde_json::from_str(include_str!("emoji-oracle.json")).unwrap();
    for case in oracle["cases"].as_array().unwrap() {
        assert_eq!(
            trim_emoji(case["input"].as_str().unwrap()),
            case["output"].as_str().unwrap(),
            "{case}"
        );
    }
}

#[test]
fn unobserved_quip_and_semantic_shapes_refuse_even_with_override() {
    let good = raw("fallback");
    assert_eq!(
        legacy_label(Kind::Canvas, Status::Normal, &good),
        Err(Unavailable::UnsupportedTitle)
    );
    let invalid = [
        json!([null]),
        json!([{"type":"section","elements":[]}]),
        json!([{"type":"rich_text"}]),
        json!([{"type":"rich_text","elements":[{"type":"rich_text_list","elements":[]}]}]),
        json!([{"type":"rich_text","elements":[{"type":"rich_text_section","elements":null}]}]),
    ];
    for blocks in invalid {
        let mut file = good.clone();
        file["title_blocks"] = blocks;
        assert_eq!(
            label_with_quip(
                Kind::Canvas,
                Status::Normal,
                &file,
                QuipTitle::Text("override")
            ),
            Err(Unavailable::UnsupportedTitle)
        );
    }
    for leaf in [
        json!({"type":"text"}),
        json!({"type":"text","text":null}),
        json!({"type":"text","text":1}),
        json!({"type":"user","user_id":"U1"}),
        json!({"type":"emoji","name":"wave"}),
        json!({"type":"link","url":"https://example.test"}),
        json!({"type":"date","timestamp":1}),
    ] {
        let mut file = good.clone();
        file["title_blocks"][0]["elements"][0]["elements"][0] = leaf;
        assert_eq!(
            label_with_quip(
                Kind::Canvas,
                Status::Normal,
                &file,
                QuipTitle::Text("override")
            ),
            Err(Unavailable::UnsupportedTitle)
        );
    }
}

#[test]
fn empty_override_is_authoritative_but_empty_extraction_needs_localization() {
    assert_eq!(
        label_with_quip(
            Kind::Canvas,
            Status::Normal,
            &raw("Extracted"),
            QuipTitle::Text("")
        ),
        Ok(String::new())
    );
    for quip in [
        QuipTitle::Nullish,
        QuipTitle::Text(""),
        QuipTitle::Text("Override"),
    ] {
        assert_eq!(
            label_with_quip(Kind::Canvas, Status::Normal, &raw(" \t\n"), quip),
            Err(Unavailable::LocalizedFallback)
        );
    }
    // Sanitization happens after fallback and override choice, without trimming.
    assert_eq!(
        label_with_quip(
            Kind::Canvas,
            Status::Normal,
            &raw("x"),
            QuipTitle::Text("  &amp; Cafe\u{301} \u{1f680}  ")
        ),
        Ok("  &amp; Cafe\u{301} \u{1f680}  ".into())
    );
    assert_eq!(
        label_with_quip(
            Kind::Canvas,
            Status::Normal,
            &raw("x"),
            QuipTitle::Text("\u{ad}")
        ),
        Ok(String::new())
    );
}

#[test]
fn plain_strings_and_lists_ignore_quip_without_expanding_scope() {
    for kind in [Kind::Canvas, Kind::WholeList] {
        for quip in [
            QuipTitle::Unavailable,
            QuipTitle::Nullish,
            QuipTitle::Text("override"),
        ] {
            let file = json!({"title":"A &amp; B", "title_blocks":[]});
            assert_eq!(
                label_with_quip(kind, Status::Normal, &file, quip),
                legacy_label(kind, Status::Normal, &file)
            );
            assert_eq!(
                label_with_quip(kind, Status::Unavailable, &file, quip),
                Err(Unavailable::Status)
            );
        }
    }
    assert_eq!(
        label_with_quip(
            Kind::WholeList,
            Status::Normal,
            &raw("ignored blocks"),
            QuipTitle::Text("override")
        ),
        Ok("ignored".into())
    );
}
