use super::super::{Kind, QuipTitle, Status, label_with_context, label_with_quip, user_ids};
use super::*;
use serde_json::json;

fn raw(leaves: Value) -> Value {
    json!({"id":"FLINKTITLE1","mode":"quip","title":"Misleading plain title",
        "title_blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":leaves}]}]})
}

fn resolve(
    file: &Value,
    quip: QuipTitle<'_>,
    fallback: Option<&str>,
) -> Result<String, Unavailable> {
    label_with_context(Kind::Canvas, Status::Normal, file, quip, fallback, |id| {
        assert_eq!(id, "ULINKUSER1");
        Ok("@Dana".into())
    })
}

#[test]
fn original_link_title_vectors_and_explicit_malformed_refusals() {
    let oracle: Value = serde_json::from_str(include_str!("oracle.json")).unwrap();
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 37);
    for case in cases {
        let file = &case["file"];
        let before = file.clone();
        let quip = case["quip"]
            .as_str()
            .map_or(QuipTitle::Nullish, QuipTitle::Text);
        let actual = resolve(file, quip, Some("Untitled"));
        if case["nativeAdmitted"] == true {
            assert_eq!(
                actual,
                Ok(case["sourceLabel"].as_str().unwrap().into()),
                "{case}"
            );
            let ids: std::collections::BTreeSet<_> = case["userIds"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect();
            assert_eq!(user_ids(file).unwrap(), ids, "{case}");
        } else {
            assert_eq!(actual, Err(Unavailable::UnsupportedTitle), "{case}");
            assert_eq!(user_ids(file), Err(Unavailable::UnsupportedTitle), "{case}");
        }
        assert_eq!(
            file, &before,
            "original file, style and opaque input unchanged"
        );
    }
}

#[test]
fn link_labels_preserve_quip_localization_order_and_shared_wrapper_behavior() {
    let file =
        raw(json!([{"type":"link","text":"Literal &amp; %20","url":"https://ignored.invalid"}]));
    assert!(user_ids(&file).unwrap().is_empty());
    assert_eq!(
        label_with_quip(Kind::Canvas, Status::Normal, &file, QuipTitle::Nullish),
        Ok("Literal &amp; %20".into())
    );
    assert_eq!(
        resolve(&file, QuipTitle::Unavailable, Some("Untitled")),
        Err(Unavailable::UnsupportedTitle)
    );
    assert_eq!(resolve(&file, QuipTitle::Text(""), None), Ok(String::new()));
    assert_eq!(
        resolve(&file, QuipTitle::Text("Override &amp;"), None),
        Ok("Override &amp;".into())
    );
    let empty = raw(json!([{"type":"link","text":" \t ","url":"https://not-selected.invalid"}]));
    for quip in [
        QuipTitle::Nullish,
        QuipTitle::Text(""),
        QuipTitle::Text("Override"),
    ] {
        assert_eq!(
            resolve(&empty, quip, None),
            Err(Unavailable::LocalizedFallback)
        );
    }
    assert_eq!(
        resolve(&empty, QuipTitle::Nullish, Some("Observed Untitled")),
        Ok("Observed Untitled".into())
    );
    assert_eq!(
        resolve(&empty, QuipTitle::Text(""), Some("Observed Untitled")),
        Ok(String::new())
    );
    assert_eq!(
        resolve(
            &empty,
            QuipTitle::Text("Override"),
            Some("Observed Untitled")
        ),
        Ok("Override".into())
    );
}

#[test]
fn link_only_titles_never_acquire_users_and_mixed_users_keep_independent_policy() {
    let leaf = json!({"type":"link","text":"Review ","url":"https://fixture.invalid",
        "contents":[{"type":"user","user_id":"UNOTDEPENDENCY"}],"style":{"bold":true},"opaque":{"keep":1}});
    let file = raw(
        json!([leaf.clone(), {"type":"user","user_id":"ULINKUSER1"}, leaf,
        {"type":"user","user_id":"ULINKUSER1","style":{"italic":true}}]),
    );
    assert_eq!(user_ids(&file).unwrap(), ["ULINKUSER1".to_owned()].into());
    for quip in [
        QuipTitle::Unavailable,
        QuipTitle::Text(""),
        QuipTitle::Text("wrong"),
        QuipTitle::Nullish,
    ] {
        assert_eq!(
            resolve(&file, quip, None),
            Ok("Review @DanaReview @Dana".into())
        );
    }
    assert_eq!(
        label_with_context(
            Kind::Canvas,
            Status::Normal,
            &file,
            QuipTitle::Text("override"),
            Some("Untitled"),
            |_| Err(Unavailable::InvalidMetadata)
        ),
        Err(Unavailable::InvalidMetadata)
    );
    let only = raw(
        json!([{"type":"link","text":"label","contents":[{"type":"user","user_id":"UNOTDEPENDENCY"}]}]),
    );
    assert!(user_ids(&only).unwrap().is_empty());
    assert_eq!(
        label_with_context(
            Kind::Canvas,
            Status::Normal,
            &only,
            QuipTitle::Nullish,
            None,
            |_| panic!("link is not a user dependency")
        ),
        Ok("label".into())
    );
}

#[test]
fn unused_malformed_fields_refuse_without_coercion_or_override_rescue() {
    for value in [
        json!(false),
        json!(0),
        json!(1),
        json!([]),
        json!({}),
        json!({"toString":null}),
    ] {
        for field in ["text", "url"] {
            let mut leaf =
                json!({"type":"link","text":"Valid label","url":"https://fixture.invalid"});
            leaf[field] = value.clone();
            let file = raw(json!([leaf]));
            assert_eq!(user_ids(&file), Err(Unavailable::UnsupportedTitle));
            assert_eq!(
                resolve(&file, QuipTitle::Text("Override"), Some("Untitled")),
                Err(Unavailable::UnsupportedTitle)
            );
        }
    }
}

#[test]
fn links_keep_sanitizer_and_do_not_expand_list_or_status_authority() {
    let file = raw(
        json!([{"type":"link","text":"\u{200b}A\u{ad}\u{a0}B\u{2028}C\u{200b}","url":"https://ignored.invalid"}]),
    );
    assert_eq!(
        resolve(&file, QuipTitle::Nullish, None),
        Ok("A B\nC".into())
    );
    let empty_after_sanitize = raw(json!([{"type":"link","text":"\u{ad}"}]));
    assert_eq!(
        resolve(&empty_after_sanitize, QuipTitle::Nullish, Some("Untitled")),
        Ok(String::new())
    );
    let unavailable = label_with_context(
        Kind::Canvas,
        Status::Unavailable,
        &file,
        QuipTitle::Nullish,
        Some("Untitled"),
        |_| unreachable!(),
    );
    assert_eq!(unavailable, Err(Unavailable::Status));
    assert_eq!(
        label_with_quip(
            Kind::WholeList,
            Status::Normal,
            &file,
            QuipTitle::Text("wrong")
        ),
        Ok("Misleading plain title".into())
    );
}
