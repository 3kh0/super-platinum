use super::*;

#[test]
fn doc_name_defaults_require_complete_literal_raw_and_effective_agreement() {
    for expected in [false, true] {
        let mut p = snapshot(
            json!({"display_real_names_override":0}),
            json!({}),
            json!({}),
        );
        assert_eq!(p.convergent_fresh_name_preference("T1"), None);
        p.observe_team(
            TeamPrefs::decode(
                "T1",
                &json!({"ok":true,"prefs":{"display_real_names":expected}}),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            p.convergent_fresh_name_preference("T1"),
            None,
            "one observed team does not complete the inventory"
        );
        p.observe_team(
            TeamPrefs::decode(
                "T2",
                &json!({"ok":true,"prefs":{"display_real_names":expected}}),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(p.convergent_fresh_name_preference("T1"), Some(expected));
        assert_eq!(p.convergent_fresh_name_preference("T2"), Some(expected));
        assert_eq!(p.should_display_real_names(), Ok(expected));
        assert_eq!(p.convergent_fresh_name_preference("TFOREIGN"), None);
        for bad in [
            json!(!expected),
            Value::Null,
            json!("yes"),
            json!([]),
            json!({}),
        ] {
            p.observe_team(
                TeamPrefs::decode("T2", &json!({"ok":true,"prefs":{"display_real_names":bad}}))
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(
                p.convergent_fresh_name_preference("T1"),
                None,
                "a mixed/null/nonboolean raw value is not a default-false receipt"
            );
        }
    }
    let mut inherited = snapshot(json!({}), json!({"display_real_names":true}), json!({}));
    for team in ["T1", "T2"] {
        inherited
            .observe_team(
                TeamPrefs::decode(
                    team,
                    &json!({"ok":true,"prefs":{"display_real_names":false}}),
                )
                .unwrap(),
            )
            .unwrap();
    }
    assert_eq!(inherited.should_display_real_names(), Ok(true));
    assert_eq!(
        inherited.convergent_fresh_name_preference("T1"),
        None,
        "singleton-style raw false cannot stand in for inherited true"
    );
    let mut precedence = snapshot(
        json!({}),
        json!({"display_real_names":false}),
        json!({"display_real_names":true}),
    );
    for team in ["T1", "T2"] {
        precedence
            .observe_team(
                TeamPrefs::decode(
                    team,
                    &json!({"ok":true,"prefs":{"display_real_names":false}}),
                )
                .unwrap(),
            )
            .unwrap();
    }
    assert_eq!(
        precedence.convergent_fresh_name_preference("T1"),
        Some(false),
        "nonnull org preference wins before locked preference"
    );
    let membership = Membership::observe(None, &[], &BTreeMap::new()).unwrap();
    let empty = Snapshot::new(
        OrdinaryClientV2,
        membership,
        UserPrefs::decode(&json!({"ok":true,"prefs":{}})).unwrap(),
        None,
    )
    .unwrap();
    assert_eq!(empty.convergent_fresh_name_preference("T1"), None);
}
