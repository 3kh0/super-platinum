use super::*;
use std::collections::{BTreeMap, BTreeSet};

fn observation(locale: &str) -> EmojiObservations {
    EmojiObservations {
        current_locale: Some(locale.into()),
        locale: Some(locale.into()),
        names: Some(
            [
                "smile",
                "wave",
                "party",
                "123",
                "handshake",
                "foo_bar",
                "foo-bar",
            ]
            .into_iter()
            .map(String::from)
            .collect::<BTreeSet<_>>(),
        ),
        translation: if locale == "fr-FR" {
            EmojiTranslation::Known(BTreeMap::from([("sourire".into(), "smile".into())]))
        } else {
            EmojiTranslation::KnownAbsent
        },
        tones: vec![
            ToneSupport {
                name: "wave".into(),
                single: true,
                multiple: false,
            },
            ToneSupport {
                name: "handshake".into(),
                single: false,
                multiple: true,
            },
            ToneSupport {
                name: "party".into(),
                single: false,
                multiple: false,
            },
        ],
        positive_names: BTreeSet::new(),
    }
}

#[test]
fn positive_standard_membership_never_invents_custom_absence_or_reorders_normalization() {
    let mut observed = observation("en-US");
    observed.names = None;
    observed.positive_names = ["smile", "foo_bar", "wave"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let input = vec![json!({"insert":":smile: :wave::skin-tone-4:\n"})];
    let before = input.clone();
    let output = replace_colon_emoji(&input, true, &observed).unwrap();
    assert!(
        output
            .iter()
            .any(|op| op["insert"]["slackemoji"]["text"] == ":smile:")
    );
    assert!(
        output
            .iter()
            .any(|op| op["insert"]["slackemoji"]["text"] == ":wave::skin-tone-4:")
    );
    assert_eq!(input, before);
    for name in ["custom_unknown", "foo-bar"] {
        assert_eq!(
            replace_colon_emoji(&[json!({"insert":format!(":{name}:\n")})], true, &observed),
            Err(SeedUnavailable::MissingEmoji(name.into()))
        );
    }
    // A complete actual set can establish the first candidate's absence and
    // proceed to the hyphen/underscore alternative. Positive-only cannot.
    observed.names = Some(observed.positive_names.clone());
    let result = replace_colon_emoji(&[json!({"insert":":foo-bar:\n"})], true, &observed).unwrap();
    assert_eq!(result[0]["insert"]["slackemoji"]["text"], ":foo_bar:");
}

#[test]
fn exact_save_omitted_team_tone_observation_does_not_attach_to_previous_emoji() {
    let mut observed = observation("en-US");
    observed.names = None;
    observed.positive_names = ["wave", "skin-tone-3"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    observed.tones = vec![ToneSupport {
        name: "wave".into(),
        single: false,
        multiple: false,
    }];
    let input = vec![
        json!({"insert":{"slackemoji":{"text":":wave:"}}}),
        json!({"insert":":skin-tone-3:\n"}),
    ];
    let result = replace_colon_emoji(&input, true, &observed).unwrap();
    assert_eq!(result[0], input[0]);
    assert_eq!(result[1]["insert"]["slackemoji"]["text"], ":skin-tone-3:");
    observed.tones.clear();
    assert_eq!(
        replace_colon_emoji(&input, true, &observed),
        Err(SeedUnavailable::MissingEmoji("wave".into()))
    );
}
#[test]
fn actual_colon_replacement_cases_match() {
    let data: Value = serde_json::from_str(include_str!("vectors.json")).unwrap();
    for (index, v) in data["results"].as_array().unwrap().iter().enumerate() {
        let input = v["input"].as_array().unwrap();
        let mut observed = observation(v["locale"].as_str().unwrap());
        observed.names = Some(
            v["names"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| s.as_str().unwrap().to_owned())
                .collect(),
        );
        let known: BTreeSet<_> = v["facts"]["single"]
            .as_array()
            .unwrap()
            .iter()
            .chain(v["facts"]["multiple"].as_array().unwrap())
            .map(|s| s.as_str().unwrap().to_owned())
            .chain(["wave".into(), "handshake".into(), "party".into(), "".into()])
            .collect();
        observed.tones = known
            .into_iter()
            .map(|name| ToneSupport {
                single: v["facts"]["single"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(name)),
                multiple: v["facts"]["multiple"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(name)),
                name,
            })
            .collect();
        if let Some(locale) = v.get("currentLocale").and_then(Value::as_str) {
            observed.current_locale = Some(locale.into());
        }
        let result = replace_colon_emoji(input, v["plain"].as_bool().unwrap(), &observed);
        if data["unsupported"]
            .as_array()
            .unwrap()
            .contains(&json!(index))
        {
            assert!(result.is_err(), "explicit inherited-map boundary {index}");
            continue;
        }
        assert_eq!(json!(result.unwrap()), v["output"], "case {index}");
    }
}

#[test]
fn inherited_translation_and_previous_cache_keys_are_not_plain_dictionary_misses() {
    let mut observed = observation("fr-FR");
    observed
        .names
        .as_mut()
        .unwrap()
        .insert("constructor".into());
    assert!(replace_colon_emoji(&[json!({"insert":":constructor:"})], false, &observed).is_err());
    assert!(observations::parse("constructor").is_err());
    observed = observation("en-US");
    observed
        .names
        .as_mut()
        .unwrap()
        .insert("constructor".into());
    assert_eq!(
        replace_colon_emoji(&[json!({"insert":":constructor:"})], false, &observed).unwrap(),
        vec![json!({"insert":{"slackemoji":{"text":":constructor:"}}})]
    );
}
#[test]
fn missing_reached_observations_refuse_without_default_substitution() {
    let input = vec![json!({"insert":":smile:"})];
    let mut observed = observation("en-US");
    observed.current_locale = None;
    assert!(replace_colon_emoji(&input, true, &observed).is_err());
    observed = observation("en-US");
    observed.names = None;
    assert!(replace_colon_emoji(&input, true, &observed).is_err());
    observed = observation("en-US");
    observed.translation = EmojiTranslation::Unobserved;
    assert!(replace_colon_emoji(&input, true, &observed).is_err());
    let token = vec![json!({"insert":":smile:","attributes":{"slackmention":{"id":"U1"}}})];
    assert_eq!(
        replace_colon_emoji(
            &token,
            true,
            &EmojiObservations {
                current_locale: Some("en-US".into()),
                ..Default::default()
            }
        )
        .unwrap(),
        token
    );
    observed = observation("en-US");
    observed.tones.clear();
    let input = vec![
        json!({"insert":{"slackemoji":{"text":":wave:"}}}),
        json!({"insert":":skin-tone-3:"}),
    ];
    assert!(replace_colon_emoji(&input, true, &observed).is_err());
    observed = observation("en-US");
    observed.tones.push(observed.tones[0].clone());
    assert!(replace_colon_emoji(&input, true, &observed).is_err());
}

#[test]
fn tone_duplicates_only_refuse_reached_names_and_unreachable_facts_are_not_needed() {
    let input = vec![
        json!({"insert":{"slackemoji":{"text":":wave:"}}}),
        json!({"insert":":skin-tone-3:"}),
    ];
    let mut observed = observation("en-US");
    let expected = replace_colon_emoji(&input, true, &observed).unwrap();
    observed.tones.push(observed.tones[2].clone());
    assert_eq!(
        replace_colon_emoji(&input, true, &observed).unwrap(),
        expected
    );
    observed.tones.push(observed.tones[0].clone());
    assert!(replace_colon_emoji(&input, true, &observed).is_err());
    assert!(replace_colon_emoji(&[json!({"insert":":smile:"})], true, &observed).is_ok());
}

#[test]
fn impossible_own_proto_translation_is_not_accepted_as_positive_source_state() {
    let mut observed = observation("fr-FR");
    let EmojiTranslation::Known(map) = &mut observed.translation else {
        unreachable!()
    };
    map.insert("__proto__".into(), "smile".into());
    assert!(observed.canonical(":__proto__:").is_err());
}
