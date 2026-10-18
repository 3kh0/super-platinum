use super::*;
use serde_json::json;

fn oracle() -> Value {
    serde_json::from_str(include_str!("oracle.json")).unwrap()
}

#[test]
fn original_normal_list_and_plain_canvas_label_vectors() {
    for case in oracle()["cases"].as_array().unwrap() {
        let kind = if case["kind"] == "list" {
            Kind::WholeList
        } else {
            Kind::Canvas
        };
        let result = label(kind, Status::Normal, &case["raw"]);
        if kind == Kind::Canvas && !ordinary(&unescape(case["raw"]["title"].as_str().unwrap())) {
            assert_eq!(result, Err(Unavailable::UnsupportedTitle), "{case}");
        } else if case["error"] == "LocalizedFallback" {
            assert_eq!(result, Err(Unavailable::LocalizedFallback), "{case}");
        } else {
            assert_eq!(result.unwrap(), case["label"].as_str().unwrap(), "{case}");
        }
    }
}

#[test]
fn original_sanitizer_order_and_line_edges() {
    for case in oracle()["sanitation"].as_array().unwrap() {
        assert_eq!(
            sanitize(case["input"].as_str().unwrap()),
            case["output"].as_str().unwrap(),
            "{case}"
        );
    }
    // Removing soft hyphen must not run before the edge-marker pass.
    assert_eq!(sanitize("x\u{200b}\u{ad}"), "x\u{200b}");
    assert_eq!(sanitize("\u{feff}a\u{200b}\r\n\u{200b}b\u{feff}"), "a\r\nb");
}

#[test]
fn original_lodash_whitespace_and_five_entities_only() {
    for case in oracle()["trimCases"].as_array().unwrap() {
        assert_eq!(
            case["input"].as_str().unwrap().trim_matches(js_space),
            case["output"].as_str().unwrap(),
            "{case}"
        );
    }
    assert_eq!(
        unescape("&amp;lt; &#39; &#x27; &apos; &nbsp;"),
        "&lt; ' &#x27; &apos; &nbsp;"
    );
    assert_eq!(
        label(
            Kind::WholeList,
            Status::Normal,
            &json!({"name":" &amp; plan "})
        ),
        Ok("&amp; plan".into())
    );
}

#[test]
fn unsupported_canvas_semantics_never_use_raw_or_name_fallback() {
    for case in oracle()["semanticCases"].as_array().unwrap() {
        assert_eq!(
            label(
                Kind::Canvas,
                Status::Normal,
                &json!({"title":case["input"],"name":"tempting fallback"})
            ),
            Err(Unavailable::UnsupportedTitle),
            "{case}"
        );
    }
    for blocks in [json!([{"type":"rich_text"}]), json!({}), json!("rich")] {
        assert_eq!(
            label(
                Kind::Canvas,
                Status::Normal,
                &json!({"title":"ordinary","title_blocks":blocks})
            ),
            Err(Unavailable::UnsupportedTitle)
        );
    }
    assert_eq!(
        label(
            Kind::Canvas,
            Status::Normal,
            &json!({"title":"ordinary","title_blocks":[]})
        ),
        Ok("ordinary".into())
    );
}

#[test]
fn unavailable_status_localization_and_malformed_shapes_are_explicit() {
    for kind in [Kind::Canvas, Kind::WholeList] {
        assert_eq!(
            label(kind, Status::Unavailable, &json!({"title":"known"})),
            Err(Unavailable::Status)
        );
        assert_eq!(
            label(kind, Status::Normal, &json!({})),
            Err(Unavailable::LocalizedFallback)
        );
        assert_eq!(
            label(kind, Status::Normal, &Value::Null),
            Err(Unavailable::InvalidMetadata)
        );
        assert_eq!(
            label(kind, Status::Normal, &json!({"title":42})),
            Err(Unavailable::InvalidMetadata)
        );
    }
    assert_eq!(
        label(Kind::Canvas, Status::Normal, &json!({"name":"file-name"})),
        Err(Unavailable::LocalizedFallback)
    );
    // Fallback selection precedes sanitizer, so empty final text is meaningful.
    assert_eq!(
        label(Kind::Canvas, Status::Normal, &json!({"title":"\u{ad}"})),
        Ok(String::new())
    );
}

#[test]
fn utf16_length_and_normalization_are_preserved_without_raw_mutation() {
    let raw = json!({"title":"Cafe\u{301} \u{1f680}","opaque":{"array":[null,1]}});
    let before = raw.clone();
    let title = label(Kind::Canvas, Status::Normal, &raw).unwrap();
    assert_eq!(title, "Cafe\u{301} \u{1f680}");
    assert_eq!(title.encode_utf16().count(), 8);
    assert_eq!(raw, before);
    assert_ne!(title, "Caf\u{e9} \u{1f680}");
}
