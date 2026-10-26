use super::*;
use crate::slack::api::profile_sections::custom::texty::file_titles::{
    Kind, QuipTitle, Status, label_with_context, label_with_emoji_context, user_ids,
    user_ids_with_emoji_context,
};
use serde_json::json;
use std::collections::BTreeSet;

const ENGLISH: Option<EmojiText> = Some(EmojiText::EnglishCanonicalNames);

fn file(leaf: Value) -> Value {
    json!({"title":"Do not substitute","title_blocks":[{"type":"rich_text","elements":[
        {"type":"rich_text_section","elements":[{"type":"text","text":"Before "},leaf]}]}]})
}

#[test]
fn retained_original_title_vectors_preserve_selection_and_source() {
    let oracle: Value = serde_json::from_str(include_str!("oracle.json")).unwrap();
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 15);
    for case in cases {
        let source = case["file"].clone();
        let quip = match case["overrideName"].as_str().unwrap() {
            "nullish" => QuipTitle::Nullish,
            "empty" => QuipTitle::Text(""),
            "text" => QuipTitle::Text("Independent title"),
            _ => panic!("unexpected original vector"),
        };
        let ids = user_ids_with_emoji_context(&source, ENGLISH).unwrap();
        assert_eq!(
            ids,
            case["userIds"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect::<BTreeSet<_>>()
        );
        let result = label_with_emoji_context(
            Kind::Canvas,
            Status::Normal,
            &source,
            quip,
            Some("Untitled"),
            ENGLISH,
            |id| {
                assert_eq!(id, "UEMOJITITLE1");
                Ok("@Dana".into())
            },
        )
        .unwrap();
        assert_eq!(result, case["finalLabel"].as_str().unwrap(), "{case}");
        assert_eq!(source, case["file"]);
        assert_eq!(user_ids(&source), Err(Unavailable::UnsupportedTitle));
        assert_eq!(
            label_with_context(
                Kind::Canvas,
                Status::Normal,
                &source,
                quip,
                Some("Untitled"),
                |_| Ok("@Dana".into())
            ),
            Err(Unavailable::UnsupportedTitle)
        );
    }
}

#[test]
fn canonical_projection_matches_both_retained_catalogs_without_aliases() {
    use crate::composer_emoji::standard::{Catalog, Version};
    let names = |version| {
        Catalog::build(version)
            .unwrap()
            .entries
            .into_iter()
            .filter(|e| {
                e.alias_of.is_none() && !e.name.contains("::") && !e.name.starts_with("skin-tone-")
            })
            .map(|e| e.name)
            .collect::<BTreeSet<_>>()
    };
    let (v16, v17) = (names(Version::V16), names(Version::V17));
    let expected: Vec<_> = v16
        .intersection(&v17)
        .map(String::as_str)
        .collect::<Vec<_>>();
    assert_eq!(canonical_names::NAMES, expected);
    for name in ["wave", "smile", "memo", "rocket"] {
        assert_eq!(
            text(&json!({"type":"emoji","name":name}), ENGLISH).unwrap(),
            format!(":{name}:")
        );
    }
}

#[test]
fn unsupported_semantics_refuse_in_both_scanner_and_extractor() {
    let mut leaves = vec![
        json!({"type":"emoji"}),
        json!({"type":"emoji","name":null}),
        json!({"type":"emoji","name":42}),
        json!({"type":"emoji","name":""}),
        json!({"type":"emoji","name":":wave:"}),
        json!({"type":"emoji","name":"WAVE"}),
        json!({"type":"emoji","name":"wave "}),
        json!({"type":"emoji","name":"custom_company"}),
        json!({"type":"emoji","name":"poop"}),
        json!({"type":"emoji","name":"skin-tone-2"}),
    ];
    for key in ["unicode", "alias", "custom", "url"] {
        for value in [Value::Null, json!(false), json!("value")] {
            let mut leaf = json!({"type":"emoji","name":"wave"});
            leaf[key] = value;
            leaves.push(leaf);
        }
    }
    for leaf in leaves {
        let raw = file(leaf);
        assert_eq!(
            user_ids_with_emoji_context(&raw, ENGLISH),
            Err(Unavailable::UnsupportedTitle)
        );
        // Even a usable override cannot hide an unsafe underlying source.
        assert_eq!(
            label_with_emoji_context(
                Kind::Canvas,
                Status::Normal,
                &raw,
                QuipTitle::Text("override"),
                Some("Untitled"),
                ENGLISH,
                |_| unreachable!()
            ),
            Err(Unavailable::UnsupportedTitle)
        );
    }
}

mod tone;

#[test]
fn emoji_context_does_not_supply_fallback_quip_or_status_authority() {
    let raw = file(json!({"type":"emoji","name":"wave"}));
    assert_eq!(
        label_with_emoji_context(
            Kind::Canvas,
            Status::Normal,
            &raw,
            QuipTitle::Unavailable,
            Some("Untitled"),
            ENGLISH,
            |_| unreachable!()
        ),
        Err(Unavailable::UnsupportedTitle)
    );
    assert_eq!(
        label_with_emoji_context(
            Kind::Canvas,
            Status::Unavailable,
            &raw,
            QuipTitle::Nullish,
            Some("Untitled"),
            ENGLISH,
            |_| unreachable!()
        ),
        Err(Unavailable::Status)
    );
    let empty = file(json!({"type":"text","text":""}));
    let mut empty = empty;
    empty["title_blocks"][0]["elements"][0]["elements"] = json!([]);
    assert_eq!(
        label_with_emoji_context(
            Kind::Canvas,
            Status::Normal,
            &empty,
            QuipTitle::Nullish,
            None,
            ENGLISH,
            |_| unreachable!()
        ),
        Err(Unavailable::LocalizedFallback)
    );
}
