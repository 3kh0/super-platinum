use super::*;

fn label(source: &Value, quip: QuipTitle<'_>) -> Result<String, Unavailable> {
    label_with_emoji_context(
        Kind::Canvas,
        Status::Normal,
        source,
        quip,
        Some("Untitled"),
        ENGLISH,
        |id| {
            assert_eq!(id, "UTONETITLE1");
            Ok("@Dana".into())
        },
    )
}

#[test]
fn retained_tone_fields_default_precedence_and_title_composition() {
    let oracle: Value = serde_json::from_str(include_str!("tone-oracle.json")).unwrap();
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 43);
    let mut matched = 0;
    let mut excluded = 0;
    for case in cases {
        let source = &case["file"];
        let before = source.clone();
        let quip = match case["overrideName"].as_str().unwrap() {
            "nullish" => QuipTitle::Nullish,
            "empty" => QuipTitle::Text(""),
            "text" => QuipTitle::Text("Independent title"),
            other => panic!("unexpected Quip case {other}"),
        };
        // These original methods run, but their separate Unicode/name authority
        // is intentionally unchanged. Do not claim all43 are admitted.
        let outside_scope = matches!(
            case["name"].as_str().unwrap(),
            "boundary-thumbsup-oracle"
                | "boundary-unicode-falsey-null"
                | "boundary-unicode-falsey-empty"
        );
        if outside_scope {
            assert_eq!(label(source, quip), Err(Unavailable::UnsupportedTitle));
            assert_eq!(
                user_ids_with_emoji_context(source, ENGLISH),
                Err(Unavailable::UnsupportedTitle)
            );
            excluded += 1;
        } else {
            assert_eq!(
                text(&case["token"], ENGLISH).unwrap(),
                case["renderedEmoji"].as_str().unwrap()
            );
            assert_eq!(
                label(source, quip).unwrap(),
                case["finalLabel"].as_str().unwrap(),
                "{}",
                case["name"]
            );
            let ids = user_ids_with_emoji_context(source, ENGLISH).unwrap();
            assert_eq!(
                ids,
                case["userIds"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().to_owned())
                    .collect::<BTreeSet<_>>()
            );
            matched += 1;
        }
        assert_eq!(*source, before);
        assert_eq!(user_ids(source), Err(Unavailable::UnsupportedTitle));
    }
    assert_eq!((matched, excluded), (40, 3));
}

#[test]
fn original_numeric_array_object_and_exact_desktop_title_sidecar() {
    let oracle: Value = serde_json::from_str(include_str!("tone-sidecar.json")).unwrap();
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 38);
    let (mut matched, mut thrown, mut desktop) = (0, 0, 0);
    for case in cases {
        let mut source = case["file"].clone();
        if let Some(literal) = case["toneLiteral"].as_str() {
            // Preserve the original raw integer/decimal JSON spelling across
            // the test boundary rather than trusting Node's serialized value.
            source["title_blocks"][0]["elements"][0]["elements"][1]["skin_tone"] =
                serde_json::from_str(literal).unwrap();
        }
        let before = source.clone();
        let expected = &case["result"];
        if expected["errorName"] == "TypeError" {
            assert_eq!(
                label(&source, QuipTitle::Nullish),
                Err(Unavailable::UnsupportedTitle)
            );
            assert_eq!(
                user_ids_with_emoji_context(&source, ENGLISH),
                Err(Unavailable::UnsupportedTitle)
            );
            thrown += 1;
        } else {
            assert_eq!(
                label(&source, QuipTitle::Nullish).unwrap(),
                expected["finalLabel"].as_str().unwrap(),
                "{}",
                case["name"]
            );
            assert!(
                user_ids_with_emoji_context(&source, ENGLISH)
                    .unwrap()
                    .is_empty()
            );
            matched += 1;
        }
        desktop += usize::from(case["desktop"] == true);
        assert_eq!(source, before);
        assert_eq!(user_ids(&source), Err(Unavailable::UnsupportedTitle));
    }
    assert_eq!((matched, thrown, desktop), (35, 3, 4));
}

#[test]
fn tone_coercion_remains_bounded_and_does_not_inspect_unselected_value() {
    let bad = json!({"toString":null});
    assert_eq!(
        text(
            &json!({"type":"emoji","name":"wave","skinTone":3,"skin_tone":bad}),
            ENGLISH
        )
        .unwrap(),
        ":wave::skin-tone-3:"
    );
    let mut deep = json!(3);
    for _ in 0..34 {
        deep = json!([deep]);
    }
    assert_eq!(
        text(
            &json!({"type":"emoji","name":"wave","skin_tone":deep}),
            ENGLISH
        ),
        Err(Unavailable::UnsupportedTitle)
    );
    assert_eq!(
        text(
            &json!({"type":"emoji","name":"wave","skin_tone":"x".repeat(1_048_577)}),
            ENGLISH
        ),
        Err(Unavailable::UnsupportedTitle)
    );
    assert_eq!(
        text(
            &json!({"type":"emoji","name":"wave","skin_tone":vec![1;10_001]}),
            ENGLISH
        ),
        Err(Unavailable::UnsupportedTitle)
    );
}
