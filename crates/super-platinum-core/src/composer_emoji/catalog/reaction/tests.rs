use super::super::CustomRecord;
use super::*;
use crate::composer_emoji::standard::Version;
use serde_json::json;
fn catalog(custom: Value) -> Catalog {
    Catalog::build_records(
        Version::V17,
        custom
            .as_object()
            .unwrap()
            .iter()
            .map(|(name, value)| CustomRecord {
                name: name.clone(),
                value: value.as_str().unwrap().into(),
                is_nonexistent: false,
            })
            .collect(),
    )
    .unwrap()
}
fn prefs(tone: Value) -> Preferences {
    let mut prefs = Preferences::default();
    prefs.replace(&std::collections::BTreeMap::from([(
        "preferred_skin_tone".into(),
        tone,
    )]));
    prefs
}
fn rows(raw: Value) -> Vec<Reaction> {
    serde_json::from_value(raw).unwrap()
}
fn name(c: &Catalog, p: &Preferences, raw: Value, requested: &str) -> String {
    ReactionChoice::resolve(c, p, &rows(raw), requested)
        .unwrap()
        .name()
        .into()
}

#[test]
fn picker_resolution_matches_executed_standard_alias_and_tone_cases() {
    let c = catalog(json!({}));
    let p = prefs(json!(4));
    for (requested, expected) in [
        ("wave", "wave::skin-tone-4"),
        ("wave::skin-tone-3", "wave::skin-tone-3"),
        ("handshake", "handshake"),
        ("handshake::skin-tone-2-3", "handshake::skin-tone-2-3"),
        ("thumbsup", "+1::skin-tone-4"),
        ("thumbsup::skin-tone-3", "+1::skin-tone-3"),
    ] {
        assert_eq!(name(&c, &p, json!([]), requested), expected);
    }
    assert_eq!(
        name(
            &c,
            &p,
            json!([{"name":"thumbsup","count":1,"users":["U_OTHER"]}]),
            "thumbsup"
        ),
        "thumbsup::skin-tone-4"
    );
    assert_eq!(name(&c, &prefs(Value::Null), json!([]), "wave"), "wave");
}

#[test]
fn custom_standard_precedence_one_hop_aliases_and_custom_tone_derivation() {
    let c = catalog(json!({"thumbsup":"https://fixture.test/shadow.png",
        "party":"https://fixture.test/party.png","party_alias":"alias:party",
        "standard_alias":"alias:thumbsup","twice":"alias:standard_alias"}));
    let p = prefs(json!(4));
    for (requested, expected) in [
        ("thumbsup", "+1::skin-tone-4"),
        ("party", "party"),
        ("party_alias", "party"),
        ("standard_alias", "thumbsup::skin-tone-4"),
        ("twice", "standard_alias::skin-tone-4"),
    ] {
        assert_eq!(name(&c, &p, json!([]), requested), expected);
    }
    // Composer insertion remains deliberately different.
    assert_eq!(
        c.resolve("party_alias", Some("4")).unwrap().name,
        "party_alias"
    );
    assert!(c.resolve("wave::skin-tone-3", Some("4")).is_none());
}

#[test]
fn actual_catalogs_have_no_obsolete_entries_but_source_branch_is_tested_synthetically() {
    for version in [Version::V16, Version::V17] {
        assert!(
            crate::composer_emoji::standard::Catalog::build(version)
                .unwrap()
                .entries
                .iter()
                .all(|entry| entry.obsoletes.is_none())
        );
    }
    let mut c = catalog(json!({}));
    // Clearly synthetic metadata; no claim these names were actually replaced.
    c.standard
        .entries
        .iter_mut()
        .find(|e| e.name == "wave")
        .unwrap()
        .obsoletes = Some("raised_hand".into());
    let p = prefs(Value::Null);
    assert_eq!(
        name(
            &c,
            &p,
            json!([{"name":"raised_hand","count":1,"users":[]}]),
            "wave"
        ),
        "raised_hand"
    );
    assert_eq!(name(&c, &p, json!([]), "wave"), "wave");
    assert_eq!(
        name(
            &c,
            &p,
            json!([{"name":"raised_hand::skin-tone-3","count":1,"users":[]}]),
            "wave::skin-tone-3"
        ),
        "raised_hand::skin-tone-3"
    );
}

#[test]
fn nonexistent_custom_is_not_valid_but_unknown_alone_does_not_hide_it() {
    use crate::slack::models::Emoji;
    for value in [json!(true), json!(1), json!("x"), json!([]), json!({})] {
        let raw: Emoji = serde_json::from_value(
            json!({"name":"party","value":"https://fixture.test/x","isNonExistent":value}),
        )
        .unwrap();
        assert!(CustomRecord::nonexistent(&raw));
        assert!(CustomRecord::from(&raw).is_nonexistent);
        let c = Catalog::build_records(Version::V17, vec![CustomRecord::from(raw)]).unwrap();
        assert!(
            !ReactionChoice::resolve(&c, &prefs(Value::Null), &[], "party")
                .unwrap()
                .addition_allowed()
        );
    }
    let raw: Emoji = serde_json::from_value(
        json!({"name":"party","value":"https://fixture.test/x","isUnknown":true}),
    )
    .unwrap();
    assert!(!CustomRecord::nonexistent(&raw));
    let c = Catalog::build(Version::V17, vec![raw]).unwrap();
    assert_eq!(name(&c, &prefs(Value::Null), json!([]), "party"), "party");
}

#[test]
fn existing_url_is_separate_validity_evidence_and_dangling_alias_is_not() {
    let c = catalog(json!({"dangling":"alias:missing"}));
    let p = prefs(json!(4));
    assert!(
        !ReactionChoice::resolve(&c, &p, &[], "dangling")
            .unwrap()
            .addition_allowed()
    );
    assert!(ReactionChoice::resolve(&c, &p, &[], "missing").is_err());
    assert_eq!(
        name(
            &c,
            &p,
            json!([{"name":"missing","count":1,"users":[],"url":"https://fixture.test/old"}]),
            "missing"
        ),
        "missing"
    );
}

#[test]
fn choice_retires_on_catalog_identity_complete_preferences_or_membership_changes() {
    let c = catalog(json!({}));
    let mut p = prefs(json!(4));
    let r = rows(json!([]));
    let choice = ReactionChoice::resolve(&c, &p, &r, "wave").unwrap();
    assert!(choice.current(&c, &p, &r));
    assert!(!choice.current(&catalog(json!({})), &p, &r));
    p.apply_change("emoji_mode", json!("as_text"));
    assert!(!choice.current(&c, &p, &r));
    assert!(!choice.current(
        &c,
        &prefs(json!(4)),
        &rows(json!([{"name":"smile","count":1,"users":[]}] ))
    ));
    assert!(ReactionChoice::resolve(&c, &Preferences::default(), &[], "wave").is_err());
}

#[test]
fn malformed_or_unsupported_variant_refusal_is_explicit_native_boundary() {
    let c = catalog(json!({}));
    let p = prefs(json!(4));
    for requested in [
        "wave::skin-tone-2-3",
        "wave::anything-4",
        "wave::skin-tone-04",
        "wave::skin-tone-3::skin-tone-4",
        ":wave:",
        " wave ",
    ] {
        assert!(
            ReactionChoice::resolve(&c, &p, &[], requested).is_err(),
            "{requested}"
        );
    }
    for preference in [json!("04"), json!("2-3"), json!(true), json!([4])] {
        assert_eq!(name(&c, &prefs(preference), json!([]), "wave"), "wave");
    }
}

#[test]
fn pill_addition_fixes_default_multi_true_and_preserves_visible_raw_names() {
    let c = catalog(json!({}));
    let p = prefs(json!(4));
    for (requested, expected) in [
        ("wave", "wave::skin-tone-4"),
        ("wave::skin-tone-3", "wave::skin-tone-3"),
        ("handshake", "handshake::skin-tone-4"),
        ("handshake::skin-tone-2-6", "handshake::skin-tone-2-6"),
        ("thumbsup", "thumbsup::skin-tone-4"),
    ] {
        let r = rows(json!([{"name":requested,"count":3,"users":["U_OTHER"]}]));
        let choice =
            ReactionChoice::resolve_pill_addition(&c, &p, &r, requested, "U_SELF").unwrap();
        assert_eq!(choice.name(), expected);
        assert!(choice.current(&c, &p, &r));
        assert!(choice.addition_only());
    }
    let r = rows(json!([{"name":"handshake","count":1,"users":[]} ]));
    assert_eq!(
        ReactionChoice::resolve(&c, &p, &r, "handshake")
            .unwrap()
            .name(),
        "handshake"
    );
}

#[test]
fn pill_addition_requires_actual_selected_row_and_empty_own_raw_base_family() {
    let c = catalog(json!({}));
    let p = prefs(json!(4));
    assert!(ReactionChoice::resolve_pill_addition(&c, &p, &[], "wave", "U_SELF").is_err());
    for owned in ["wave", "wave::skin-tone-2"] {
        let r = rows(
            json!([{"name":"wave::skin-tone-3","count":1,"users":["U_OTHER"]},
            {"name":owned,"count":9,"users":["U_SELF"]}]),
        );
        assert!(
            ReactionChoice::resolve_pill_addition(&c, &p, &r, "wave::skin-tone-3", "U_SELF")
                .is_err()
        );
    }
    let r = rows(json!([{"name":"thumbsup","count":1,"users":[]},
        {"name":"+1::skin-tone-4","count":1,"users":["U_SELF"]}]));
    assert_eq!(
        ReactionChoice::resolve_pill_addition(&c, &p, &r, "thumbsup", "U_SELF")
            .unwrap()
            .name(),
        "thumbsup::skin-tone-4"
    );
    let ambiguous = rows(json!([{"name":"wave","count":1,"users":[]},
        {"name":"wave::anything-4","count":1,"users":["U_SELF"]}]));
    assert!(ReactionChoice::resolve_pill_addition(&c, &p, &ambiguous, "wave", "U_SELF").is_err());
}

#[test]
fn pill_addition_keeps_observed_preferences_custom_and_existing_url_rules() {
    let c = catalog(json!({"party":"https://fixture.test/party","party_alias":"alias:wave"}));
    for (requested, expected) in [
        ("party", "party"),
        ("party_alias", "party_alias::skin-tone-4"),
    ] {
        let r = rows(json!([{"name":requested,"count":1,"users":["U_OTHER"]}]));
        assert_eq!(
            ReactionChoice::resolve_pill_addition(&c, &prefs(json!(4)), &r, requested, "U_SELF")
                .unwrap()
                .name(),
            expected
        );
    }
    let r = rows(json!([{"name":"wave","count":1,"users":[]} ]));
    let p = prefs(Value::Null);
    let choice = ReactionChoice::resolve_pill_addition(&c, &p, &r, "wave", "U_SELF").unwrap();
    assert_eq!(choice.name(), "wave");
    assert!(!choice.current(&c, &prefs(json!(4)), &r));
    assert!(
        ReactionChoice::resolve_pill_addition(&c, &Preferences::default(), &r, "wave", "U_SELF")
            .is_err()
    );
    let old =
        rows(json!([{"name":"old_custom","count":1,"users":[],"url":"https://fixture.test/old"}]));
    assert_eq!(
        ReactionChoice::resolve_pill_addition(&c, &p, &old, "old_custom", "U_SELF")
            .unwrap()
            .name(),
        "old_custom"
    );
    let no_url = rows(json!([{"name":"old_custom","count":1,"users":[]} ]));
    assert!(
        ReactionChoice::resolve_pill_addition(&c, &p, &no_url, "old_custom", "U_SELF").is_err()
    );
}
