use super::*;
use serde_json::json;

fn blocks(text: &str) -> Value {
    json!({"title":"ignored", "opaque":{"keep":true}, "title_blocks":[
    {"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
        {"type":"text","text":text,"style":{"bold":true}}
    ]}]}]})
}
fn resolve(
    raw: &Value,
    quip: QuipTitle<'_>,
    fallback: Option<&str>,
) -> Result<String, Unavailable> {
    label_with_context(Kind::Canvas, Status::Normal, raw, quip, fallback, |_| {
        Err(Unavailable::UnsupportedTitle)
    })
}

#[test]
fn original_localized_fallback_vectors_preserve_quip_order_and_raw() {
    let oracle: Value = serde_json::from_str(include_str!("blocks/oracle.json")).unwrap();
    let cases = oracle["fallbackCases"].as_array().unwrap();
    assert_eq!(cases.len(), 8);
    let raw = blocks(" \t\n");
    let before = raw.clone();
    for case in cases {
        let quip = case["state"]["quipTitle"]["F00000001"]["titleString"]
            .as_str()
            .map_or(QuipTitle::Nullish, QuipTitle::Text);
        assert_eq!(
            resolve(&raw, quip, case["localized"].as_str()),
            Ok(case["label"].as_str().unwrap().to_owned()),
            "{case}"
        );
        assert_eq!(
            resolve(&raw, quip, None),
            Err(Unavailable::LocalizedFallback)
        );
    }
    assert_eq!(raw, before);
}

#[test]
fn plain_canvas_fallback_ignores_quip_but_does_not_widen_list_or_status() {
    for raw in [
        json!({}),
        json!({"title":null}),
        json!({"title":" \t\n","title_blocks":[]}),
    ] {
        for quip in [
            QuipTitle::Unavailable,
            QuipTitle::Nullish,
            QuipTitle::Text(""),
            QuipTitle::Text("wrong"),
        ] {
            assert_eq!(resolve(&raw, quip, Some("Untitled")), Ok("Untitled".into()));
            assert_eq!(
                resolve(&raw, quip, None),
                Err(Unavailable::LocalizedFallback)
            );
        }
    }
    assert_eq!(
        label_with_context(
            Kind::WholeList,
            Status::Normal,
            &json!({}),
            QuipTitle::Nullish,
            Some("Untitled"),
            |_| unreachable!()
        ),
        Err(Unavailable::LocalizedFallback)
    );
    assert_eq!(
        label_with_context(
            Kind::Canvas,
            Status::Unavailable,
            &json!({}),
            QuipTitle::Nullish,
            Some("Untitled"),
            |_| unreachable!()
        ),
        Err(Unavailable::Status)
    );
}

#[test]
fn fallback_precedes_sanitization_and_does_not_repair_unsupported_grammar() {
    assert_eq!(
        resolve(&blocks("\u{ad}"), QuipTitle::Nullish, Some("Untitled")),
        Ok(String::new())
    );
    assert_eq!(
        resolve(
            &json!({"title":"\u{ad}"}),
            QuipTitle::Nullish,
            Some("Untitled")
        ),
        Ok(String::new())
    );
    assert_eq!(
        resolve(&blocks(""), QuipTitle::Nullish, Some("\u{ad}")),
        Ok(String::new())
    );
    assert_eq!(
        resolve(&blocks(""), QuipTitle::Text(""), Some("Untitled")),
        Ok(String::new())
    );
    assert_eq!(
        resolve(&blocks(""), QuipTitle::Unavailable, Some("Untitled")),
        Err(Unavailable::UnsupportedTitle)
    );
    let mut raw = blocks("");
    raw["title_blocks"][0]["elements"][0]["elements"][0] =
        json!({"type":"link","url":{"malformed":true}});
    assert_eq!(
        resolve(&raw, QuipTitle::Text("override"), Some("Untitled")),
        Err(Unavailable::UnsupportedTitle)
    );
    assert_eq!(
        resolve(&json!({"title":17}), QuipTitle::Nullish, Some("Untitled")),
        Err(Unavailable::InvalidMetadata)
    );
}

#[test]
fn actual_user_extraction_bypasses_quip_with_explicit_context() {
    let mut raw = blocks("");
    raw["title_blocks"][0]["elements"][0]["elements"][0] = json!({"type":"user","user_id":"U1"});
    assert_eq!(
        label_with_context(
            Kind::Canvas,
            Status::Normal,
            &raw,
            QuipTitle::Text("conflict"),
            Some("Untitled"),
            |_| Ok("@Ada".into())
        ),
        Ok("@Ada".into())
    );
    // Empty user text is a synthetic formatter probe, not a native name-policy grant.
    assert_eq!(
        label_with_context(
            Kind::Canvas,
            Status::Normal,
            &raw,
            QuipTitle::Text("conflict"),
            Some("Untitled"),
            |_| Ok(String::new())
        ),
        Ok("Untitled".into())
    );
}
