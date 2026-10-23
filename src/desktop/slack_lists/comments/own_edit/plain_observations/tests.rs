use super::*;
use super_platinum_core::composer_emoji::standard::Version;

#[test]
fn original_complete_experiment_map_selects_exact_catalog_version() {
    for (assignments, expected) in [
        (json!({}), Version::V16),
        (json!({"emoji_v17":{"group":"on"}}), Version::V17),
        (json!({"emoji_v17":{"group":"treatment"}}), Version::V16),
        (json!({"emoji_v17":{"group":"ON"}}), Version::V16),
        (json!({"emoji_v17":null}), Version::V16),
    ] {
        assert_eq!(
            preferences::emoji_version(&json!({"ok":true,"assignments":assignments})).unwrap(),
            expected
        );
    }
    for response in [
        json!({}),
        json!({"ok":true}),
        json!({"ok":false,"assignments":{}}),
        json!({"ok":true,"assignments":{"emoji_v17":{"group":true}}}),
    ] {
        assert!(preferences::emoji_version(&response).is_err());
    }
}

#[test]
fn acquired_preferences_preserve_keywords_membership_order_and_exact_mode() {
    let prefs = json!({"ok":true,"prefs":{"emoji_mode":"as_text",
        "all_notifications_prefs":{"channels":{},"global":{"global_keywords":" Fox ,,FOX,中 "}}}});
    assert!(preferences::emoji_as_text(&prefs).unwrap());
    let groups = vec!["S2".into(), "S1".into(), "S2".into()];
    let words = preferences::highlights(&prefs, "@U1", &groups).unwrap();
    assert_eq!(
        words,
        vec![
            "<@U1",
            " Fox ",
            "",
            "FOX",
            "中 ",
            "<!subteam^S2",
            "<!subteam^S1",
            "<!subteam^S2"
        ]
    );
    let mut encoded = prefs.clone();
    encoded["prefs"]["all_notifications_prefs"] =
        prefs["prefs"]["all_notifications_prefs"].to_string().into();
    assert_eq!(
        preferences::highlights(&encoded, "@U1", &groups).unwrap(),
        words
    );
    let mut other = prefs;
    other["prefs"]["emoji_mode"] = "As_Text".into();
    assert!(!preferences::emoji_as_text(&other).unwrap());
}

#[test]
fn missing_or_failed_preference_carriers_never_become_empty_observations() {
    for prefs in [
        json!({}),
        json!({"ok":false,"prefs":{"emoji_mode":"as_text"}}),
        json!({"ok":true}),
        json!({"ok":true,"error":"missing_scope","prefs":{"emoji_mode":"as_text"}}),
    ] {
        assert!(preferences::emoji_as_text(&prefs).is_err());
        assert!(preferences::highlights(&prefs, "U1", &[]).is_err());
    }
    let acquired =
        json!({"ok":true,"prefs":{"all_notifications_prefs":{"channels":{},"global":{}}}});
    assert!(!preferences::emoji_as_text(&acquired).unwrap());
    let alias_only = json!({"ok":true,"prefs":{"emoji_as_text":true}});
    assert!(!preferences::emoji_as_text(&alias_only).unwrap());
    assert!(preferences::highlights(&alias_only, "U1", &[]).is_err());
    assert_eq!(
        preferences::highlights(&acquired, "U1", &[]).unwrap(),
        vec!["<@U1"]
    );
    assert!(preferences::highlights(&acquired, "", &[]).is_err());
    assert!(preferences::highlights(&acquired, "U1", &["".into()]).is_err());
}

#[test]
fn explicit_english_broadcasts_do_not_invent_dynamic_labels() {
    assert_eq!(preferences::english_broadcast("here").unwrap(), "@here");
    assert_eq!(
        preferences::english_broadcast("channel").unwrap(),
        "@channel"
    );
    assert_eq!(
        preferences::english_broadcast("everyone").unwrap(),
        "@everyone"
    );
    for range in ["", "unknown", "assistant:einstein", "sf:dealteam"] {
        assert!(preferences::english_broadcast(range).is_err());
    }
}
