use super::*;
use serde::Deserialize;

#[test]
fn normalization_composes_without_trimming_or_compatibility_folding() {
    assert_eq!(normalize("  Jose\u{301}  "), "  José  ");
    assert_eq!(normalize("\u{1100}\u{1161}"), "가");
    assert_eq!(normalize("① Ａ"), "① Ａ");
    assert_eq!(
        normalize_and_validate(Field::FullName, " A\u{30a} ").unwrap(),
        " Å "
    );
}

#[test]
fn required_clear_and_utf16_limits_match_each_field() {
    assert_eq!(
        validate(Field::FullName, " \u{feff}"),
        Err(ValidationError::Required)
    );
    for field in [Field::DisplayName, Field::Pronouns] {
        assert!(validate(field, "").is_ok());
        assert!(validate(field, " \u{feff}").is_ok());
    }
    for field in [Field::FullName, Field::DisplayName, Field::Pronouns] {
        assert!(validate(field, &format!(" {} ", "𐐀".repeat(40))).is_ok());
        assert_eq!(
            validate(field, &"𐐀".repeat(41)),
            Err(ValidationError::TooLong { max_utf16: 80 })
        );
        assert_eq!(
            normalize_and_validate(field, &"e\u{301}".repeat(80)).unwrap(),
            "é".repeat(80)
        );
    }
    // Pronouns have only the length rule, not the member-name exclusions.
    assert!(validate(Field::Pronouns, "@all 🦊").is_ok());
    assert!(validate(Field::FullName, "\u{85}").is_ok());
}

#[test]
fn exclusions_are_exact_not_generic_unicode_or_reserved_substring_filters() {
    for field in [Field::FullName, Field::DisplayName] {
        assert_eq!(validate(field, "@name"), Err(ValidationError::LeadingAt));
        assert_eq!(
            validate(field, " @name "),
            Err(ValidationError::RestrictedCharacter)
        );
        assert_eq!(
            validate(field, " SLACK "),
            Err(ValidationError::ReservedName)
        );
        assert_eq!(
            validate(field, "Slack 機器人"),
            Err(ValidationError::ReservedName)
        );
        assert_eq!(
            validate(field, "SLACK 機器人"),
            Err(ValidationError::ReservedName)
        );
        assert!(validate(field, "Slack engineer").is_ok());
        assert_eq!(
            validate(field, "---"),
            Err(ValidationError::PunctuationOnly)
        );
        assert!(validate(field, " --- ").is_ok()); // Captured regex is anchored to the original value.
        assert_eq!(validate(field, "A 🦊"), Err(ValidationError::Emoji));
        assert!(validate(field, "A :fox_face:").is_err()); // Colon is independently restricted.
        assert!(validate(field, "Māori 李 Δ").is_ok());
    }
}

#[test]
fn captured_javascript_profile_and_emoji_oracles_match() {
    #[derive(Deserialize)]
    struct Oracle {
        cases: Vec<Case>,
        #[serde(rename = "emojiCases")]
        emoji_cases: Vec<EmojiCase>,
    }
    #[derive(Deserialize)]
    struct Case {
        field: String,
        input: String,
        normalized: String,
        error: Option<String>,
    }
    #[derive(Deserialize)]
    struct EmojiCase {
        input: String,
        expected: bool,
    }
    let oracle: Oracle = serde_json::from_str(include_str!("oracle.json")).unwrap();
    assert_eq!(oracle.cases.len(), 675);
    assert_eq!(oracle.emoji_cases.len(), 1725);
    for case in oracle.cases {
        let field = match case.field.as_str() {
            "FullName" => Field::FullName,
            "DisplayName" => Field::DisplayName,
            "Pronouns" => Field::Pronouns,
            _ => panic!("unexpected fixture field"),
        };
        let normalized = normalize(&case.input);
        assert_eq!(normalized, case.normalized, "NFC {:?}", case.input);
        let actual = validate(field, &normalized).err().map(|e| match e {
            ValidationError::Required => "Required",
            ValidationError::TooLong { .. } => "TooLong",
            ValidationError::LeadingAt => "LeadingAt",
            ValidationError::RestrictedCharacter => "RestrictedCharacter",
            ValidationError::ReservedName => "ReservedName",
            ValidationError::PunctuationOnly => "PunctuationOnly",
            ValidationError::Emoji => "Emoji",
        });
        assert_eq!(
            actual,
            case.error.as_deref(),
            "{:?}: {:?}",
            field,
            case.input
        );
    }
    for case in oracle.emoji_cases {
        assert_eq!(
            emoji::contains(&case.input),
            case.expected,
            "emoji {:?}",
            case.input
        );
    }
}

#[test]
fn every_captured_native_glyph_is_rejected_but_custom_shortcode_is_not_an_emoji() {
    let glyphs: Vec<String> = serde_json::from_str(include_str!("emoji.json")).unwrap();
    assert_eq!(glyphs.len(), 3786);
    for glyph in glyphs {
        assert!(emoji::contains(&glyph), "{glyph:?}");
    }
    assert!(!emoji::contains(":custom_workspace_emoji:"));
    assert!(!emoji::contains("plain profile name"));
}
