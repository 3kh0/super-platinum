//! Current Source AST/math examples, not historical cohort substitution.
use super::*;
use serde_json::json;
use super_platinum_core::{composer_emoji::standard::Version, thread_reaction::OriginalRoot};
fn model(version: Version) -> (Model, Vec<Selected>) {
    let raw = json!({"type":"message","channel":"C1","ts":"1712000001.000001","thread_ts":"1712000001.000001","user":"U1","text":"Names","is_locked":false,"is_beyond_free_limit":false});
    let original = OriginalRoot::capture("C1", "1712000001.000001", &raw).unwrap();
    let policy=RootPolicy::capture(&original,&json!({"ok":true,"messages":[raw],"has_more":false}),
        &json!({"ok":true,"channel":{"id":"C1","name":"ship","is_channel":true,"is_member":true,"is_private":false,"is_frozen":false}}),
        &json!({"ok":true,"prefs":{"hidden_users":"","preferred_skin_tone":3}})).unwrap();
    let mut message = raw;
    message["reactions"] = json!([]);
    let membership = RootMembership::decode(
        &original,
        &json!({"ok":true,"type":"message","channel":"C1","message":message}),
    )
    .unwrap();
    let catalog = Catalog::build_records(version, vec![]).unwrap();
    let domain = PreferredDomain::build().unwrap();
    let alias = AliasDomain::build().unwrap();
    let mut choices = domain
        .choices(&catalog, policy.preferences())
        .into_iter()
        .map(Selected::Canonical)
        .collect::<Vec<_>>();
    choices.extend(
        alias
            .choices(&policy, &catalog, policy.preferences(), &membership)
            .into_iter()
            .map(Selected::Alias),
    );
    (Model::build(&catalog, &choices), choices)
}
#[test]
fn current_name_model_both_maps_best_raw_alias_finite_ties_blocks_caps_and_positive_eyes16() {
    let expected = [
        "eyes",
        "heart_eyes",
        "heart_eyes_cat",
        "kissing_closed_eyes",
        "kissing_smiling_eyes",
        "face_with_spiral_eyes",
        "face_with_rolling_eyes",
        "face_with_bags_under_eyes",
        "stuck_out_tongue_closed_eyes",
        "grinning_face_with_star_eyes",
        "face_with_open_eyes_and_hand_over_mouth",
        "smiling_face_with_smiling_eyes_and_hand_covering_mouth",
        "eyeglasses",
        "grey_question",
        "large_yellow_square",
        "eye-in-speech-bubble",
    ];
    for version in [Version::V16, Version::V17] {
        let (model, choices) = model(version);
        assert_eq!(model.entries.len(), 1892);
        assert_eq!(
            model.entries.iter().map(|e| e.aliases.len()).sum::<usize>(),
            55
        );
        let names = |query: &str| {
            model
                .query(query)
                .unwrap()
                .into_iter()
                .map(|i| choices[i].name().to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            names("eyes"),
            expected,
            "exact derived positive refinement, not a broad threshold"
        );
        assert_eq!(
            names("thumb"),
            [
                "thumbsup::skin-tone-3",
                "thumbsdown::skin-tone-3",
                "beach_with_umbrella",
                "hand_with_index_finger_and_thumb_crossed::skin-tone-3"
            ]
        );
        assert_eq!(names("h").len(), 250);
        assert_eq!(names("l").len(), 250);
        let h = search::candidates("h", &model.entries, keyword::Options::default());
        let poop = h.iter().find(|c| c.name == "hankey").unwrap();
        assert_eq!(poop.canonical, "shit");
        assert_eq!(poop.name_score.unwrap().to_bits(), 0x3fe8000000000000);
        let l = search::candidates("l", &model.entries, keyword::Options::default());
        let dolphin = l.iter().find(|c| c.name == "dolphin").unwrap();
        assert_eq!(dolphin.canonical, "dolphin");
        assert_eq!(dolphin.name_score.unwrap().to_bits(), 0x3feb6db6db6db6dc);
        let sh = search::candidates("sh", &model.entries, keyword::Options::default());
        assert!(
            !sh.iter().any(|c| c.name == "hankey"),
            "Source short-name block table is retained"
        );
        for unsupported in ["", "+1", "-1", "👀", "\u{85}thumb"] {
            assert!(
                model.query(unsupported).is_none(),
                "not a claim about broader Source parser modes"
            );
        }
        assert_eq!(names(" \u{feff}:thu:mb:\u{a0}"), names("thumb"));
    }
}

#[test]
fn ascii_case_and_whitespace_name_queries_both_maps_keep_raw_alias_blocks_and_bounds() {
    for version in [Version::V16, Version::V17] {
        let (model, choices) = model(version);
        let names = |query: &str| {
            model
                .query(query)
                .unwrap()
                .into_iter()
                .map(|i| choices[i].name().to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(names("THUMB"), names("thumb"));
        assert_eq!(names("HEART EYES"), ["heart_eyes", "heart_eyes_cat"]);
        assert_eq!(names("Heart Eyes"), names("heart eyes"));
        assert_eq!(names("Thumb Up"), ["thumbsup::skin-tone-3"]);
        assert_eq!(names("THUMB UP"), names("thumb up"));
        let spaces = [
            '\u{0009}', '\u{000a}', '\u{000b}', '\u{000c}', '\u{000d}', '\u{0020}', '\u{00a0}',
            '\u{1680}', '\u{2000}', '\u{2001}', '\u{2002}', '\u{2003}', '\u{2004}', '\u{2005}',
            '\u{2006}', '\u{2007}', '\u{2008}', '\u{2009}', '\u{200a}', '\u{2028}', '\u{2029}',
            '\u{202f}', '\u{205f}', '\u{3000}', '\u{feff}',
        ];
        for space in spaces {
            assert_eq!(
                names(&format!("{space}:HEART:{space}{space}EYES:{space}")),
                names("heart eyes")
            );
        }
        assert_eq!(names(" \u{feff}:ThUmB\t  Up:\u{a0}"), names("thumb up"));
        assert!(
            !names("SH").iter().any(|n| n.starts_with("shit")),
            "case folding retains Source blocked names"
        );
        assert!(
            names("S H").iter().any(|n| n == "shit"),
            "inner spaces survive until Source's blocked-table check"
        );
        for unsupported in [
            "+1",
            "-1",
            "+Thumb",
            "a+b",
            "_thumb",
            "éyes",
            "\u{85}thumb",
            "heart\u{85}eyes",
            "a\0b",
        ] {
            assert!(
                model.query(unsupported).is_none(),
                "unsupported profile: {unsupported:?}"
            );
        }
        assert_eq!(names(&format!("{}thumb", ":".repeat(4091))), names("thumb"));
        assert!(model.query(&format!("{}thumb", ":".repeat(4092))).is_none());
        assert!(model.query(&"h".repeat(256)).is_some());
        assert!(model.query(&"h".repeat(257)).is_none());
        let at_byte_limit = format!("h{}  h", "\u{2000}".repeat(84));
        let over_byte_limit = format!("h{}   h", "\u{2000}".repeat(84));
        assert_eq!(at_byte_limit.len(), 256);
        assert_eq!(over_byte_limit.len(), 257);
        for raw in [&at_byte_limit, &over_byte_limit] {
            assert_eq!(
                super_platinum_core::composer_emoji::normalize::normalize(raw),
                "h h"
            );
        }
        assert!(model.query(&at_byte_limit).is_some());
        assert!(
            model.query(&over_byte_limit).is_none(),
            "bound is bytes BEFORE normalization"
        );
    }
}
