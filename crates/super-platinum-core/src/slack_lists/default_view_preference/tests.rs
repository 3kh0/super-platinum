use super::*;
fn document() -> Document {
    Document::parse(json!({"id":"F1","mode":"list","access":"write","list_metadata":{"schema":[],"views":[{"id":"V1","name":"Delivery","type":"board","opaque":7}]}})).unwrap()
}
fn response(map: Value) -> Value {
    json!({"ok":true,"prefs":{"lists_default_view":map.to_string()}})
}
#[test]
fn whole_map_and_exact_prior_snapshot_are_preserved() {
    let doc = document();
    let before = response(json!({"F_OTHER":"V_OTHER","F1":"OLD"}));
    let intent = ListDefaultViewPreference::capture(&before, &doc, "V1").unwrap();
    assert_eq!(intent.expected(), &json!({"F_OTHER":"V_OTHER","F1":"V1"}));
    assert!(intent.revalidate(&before, &doc).is_ok());
    assert!(
        intent
            .revalidate(&response(json!({"F_OTHER":"CHANGED","F1":"OLD"})), &doc)
            .is_err()
    );
    assert!(!intent.observes_desired(&response(json!({"F1":"V1"}))));
    assert!(intent.observes_desired(&response(intent.expected().clone())));
}
#[test]
fn metadata_ack_never_confirms_preference_and_attempt_never_replays() {
    let doc = document();
    let before = response(json!({"F_OTHER":"V_OTHER"}));
    let intent = ListDefaultViewPreference::capture(&before, &doc, "V1").unwrap();
    let expected = response(intent.expected().clone());
    let mut a = ListDefaultViewPreferenceAttempt::new(intent);
    assert!(!a.observe(&expected));
    let _permit = a.admit(&before, &doc).unwrap();
    assert!(a.finish(false));
    assert!(!a.observe(&json!({"ok":true,"list":doc.raw})));
    assert!(!a.observe(&before));
    assert!(a.admit(&before, &doc).is_err());
    assert!(a.observe(&expected));
    assert!(!a.observe(&expected));
}
#[test]
fn malformed_or_duplicate_map_is_refused_without_partial_replacement() {
    for encoded in [
        " ",
        "[]",
        "null",
        r#"{"F_OTHER":true}"#,
        r#"{"F_OTHER":"A","F_OTHER":"B"}"#,
    ] {
        assert!(
            ListDefaultViewPreference::capture(
                &json!({"ok":true,"prefs":{"lists_default_view":encoded}}),
                &document(),
                "V1"
            )
            .is_err()
        );
    }
    assert!(
        ListDefaultViewPreference::capture(&response(json!({})), &document(), "UNKNOWN").is_err()
    );
}

#[test]
fn explicitly_empty_observation_can_save_first_default_without_weakening_presence_or_exactness() {
    let doc = document();
    let empty = json!({"ok":true,"prefs":{"lists_default_view":""}});
    let intent = ListDefaultViewPreference::capture(&empty, &doc, "V1").unwrap();
    assert_eq!(intent.expected(), &json!({"F1":"V1"}));
    assert!(intent.revalidate(&empty, &doc).is_ok());
    assert!(intent.revalidate(&response(json!({})), &doc).is_err());
    assert!(
        ListDefaultViewPreference::capture(&json!({"ok":true,"prefs":{}}), &doc, "V1").is_err()
    );
    assert!(
        ListDefaultViewPreference::capture(
            &json!({"ok":false,"prefs":{"lists_default_view":""}}),
            &doc,
            "V1"
        )
        .is_err()
    );
    assert!(intent.observes_desired(&response(json!({"F1":"V1"}))));
}
