use super::*;
#[test]
fn exact_retained_title_error_precedence_and_utf16_cases() {
    for (input, expected) in [
        ("", Some(NameError::Required)),
        (" ", Some(NameError::Required)),
        ("  ﻿", Some(NameError::Required)),
        ("", None),
        ("​", None),
        ("constructor", Some(NameError::Reserved)),
        (" __proto__ ", Some(NameError::Reserved)),
        ("CONSTRUCTOR", Some(NameError::Reserved)),
        ("toString", None),
        ("general", Some(NameError::Reserved)),
        ("GÉNÉRAL", Some(NameError::Reserved)),
        ("Slack 機器人", None),
        ("all hands", None),
        ("-", Some(NameError::SpecialCharacters)),
        ("___", Some(NameError::SpecialCharacters)),
        ("...", Some(NameError::SpecialCharacters)),
        ("‘’", Some(NameError::SpecialCharacters)),
        ("name;", None),
        ("@foo", Some(NameError::RestrictedCharacters)),
        ("a@b", Some(NameError::RestrictedCharacters)),
        ("a,b", Some(NameError::RestrictedCharacters)),
        ("a/b", Some(NameError::RestrictedCharacters)),
        ("a\\b", Some(NameError::RestrictedCharacters)),
        ("a:b", Some(NameError::RestrictedCharacters)),
        ("a|b", Some(NameError::RestrictedCharacters)),
        ("aâb", Some(NameError::RestrictedCharacters)),
        ("ab", Some(NameError::RestrictedCharacters)),
        ("ab", Some(NameError::RestrictedCharacters)),
        ("日本語", None),
        ("文…字", Some(NameError::SpecialCharacters)),
        ("한글", None),
        (
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            None,
        ),
        (
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            Some(NameError::TooLong),
        ),
        (
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaé",
            None,
        ),
        (
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa ",
            Some(NameError::TooLong),
        ),
        ("😀", Some(NameError::Emoji)),
        ("𐐀", Some(NameError::Emoji)),
        ("©", Some(NameError::Emoji)),
        ("™", Some(NameError::Emoji)),
        ("1⃣", Some(NameError::Emoji)),
        ("1️⃣", Some(NameError::Emoji)),
        ("x⃣", None),
        ("a🙂", Some(NameError::Emoji)),
        ("✀", Some(NameError::Emoji)),
        ("➿", Some(NameError::Emoji)),
        ("⟀", None),
        ("　x", Some(NameError::SpecialCharacters)),
        ("x　", Some(NameError::SpecialCharacters)),
        ("　", Some(NameError::SpecialCharacters)),
        ("Ｘ", None),
        ("x！", Some(NameError::SpecialCharacters)),
        ("normal (meeting)", None),
    ] {
        assert_eq!(validate_name(input).err(), expected, "{input:?}");
    }
}
#[test]
fn description_uses_js_trim_and_utf16_length() {
    for (input, maximum, expected) in [
        ("", 80, true),
        ("  ", 80, true),
        ("", 80, false),
        ("😀", 1, true),
        ("😀", 2, false),
        (
            "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
            80,
            true,
        ),
    ] {
        assert_eq!(description_invalid(input, maximum), expected);
    }
}
