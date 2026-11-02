use super::*;
use crate::composer_emoji::catalog::CustomRecord;
use serde_json::json;
const C: &str = "C1";
const TS: &str = "1710000001.000001";
const ME: &str = "U1";
fn raw() -> Value {
    json!({"type":"message","channel":C,"ts":TS,"thread_ts":TS,"user":"U2","text":"Original first-Add root","is_locked":false,"is_beyond_free_limit":false})
}
fn policy(original: &OriginalRoot) -> RootPolicy {
    RootPolicy::capture(original,
        &json!({"ok":true,"messages":[raw()],"has_more":false}),
        &json!({"ok":true,"channel":{"id":C,"name":"ship","is_channel":true,"is_member":true,"is_private":false,"is_frozen":false}}),
        &json!({"ok":true,"prefs":{"hidden_users":"","preferred_skin_tone":3}}),
    ).unwrap()
}
fn full(original: &OriginalRoot, rows: Value) -> RootMembership {
    let mut message = raw();
    message["reactions"] = rows;
    RootMembership::decode(
        original,
        &json!({"ok":true,"type":"message","channel":C,"message":message}),
    )
    .unwrap()
}
fn row(name: &str, user: &str) -> Value {
    json!({"name":name,"count":1,"users":[user]})
}

#[test]
fn first_add_joint_glyph_domain_empty_root_exact_selection_quotas_and_snapshot_corpus() {
    let domain = FirstAddDomain::build().unwrap();
    let catalog = Catalog::build_records(Version::V16, vec![]).unwrap();
    let newer = Catalog::build_records(Version::V17, vec![]).unwrap();
    let choices = domain.choices(&catalog);
    assert_eq!(
        choices.len(),
        1582,
        "the joint source profile, not V16-only1588"
    );
    assert_eq!(domain.choices(&newer), choices);
    assert!(choices.windows(2).all(|r| r[0].name() < r[1].name()));
    for name in ["heart", "eyes", "fire", "tada", "white_check_mark"] {
        let choice = choices.iter().find(|c| c.name() == name).unwrap();
        assert!(!choice.glyph().is_empty());
        assert_eq!(
            catalog.resolve(name, None).unwrap().glyph.as_deref(),
            Some(choice.glyph())
        );
    }
    for excluded in [
        "dancers",
        "man-wrestling",
        "men-with-bunny-ears-partying",
        "woman-wrestling",
        "women-with-bunny-ears-partying",
        "wrestlers",
        "wave",
        "thumbsup",
        "wave::skin-tone-3",
        "__proto__",
    ] {
        assert!(!choices.iter().any(|c| c.name() == excluded), "{excluded}");
    }
    let shadow = Catalog::build_records(
        Version::V16,
        vec![CustomRecord {
            name: "eyes".into(),
            value: "https://fixture.invalid/custom.png".into(),
            is_nonexistent: false,
        }],
    )
    .unwrap();
    assert_eq!(domain.choices(&shadow).len(), 1581);
    assert!(!domain.choices(&shadow).iter().any(|c| c.name() == "eyes"));
    let alias_shadow = Catalog::build_records(
        Version::V16,
        vec![CustomRecord {
            name: "eyes".into(),
            value: "alias:heart".into(),
            is_nonexistent: false,
        }],
    )
    .unwrap();
    assert!(
        !domain
            .choices(&alias_shadow)
            .iter()
            .any(|c| c.name() == "eyes"),
        "custom aliases cannot acquire a standard glyph grant"
    );

    let original = OriginalRoot::capture(C, TS, &raw()).unwrap();
    let p = policy(&original);
    let prefs = p.preferences();
    let empty = full(&original, json!([]));
    assert!(
        empty.selection(&p, &catalog, prefs, "eyes", ME).is_err(),
        "existing family purpose remains narrower"
    );
    let add = empty
        .first_add_selection(&p, &domain, &catalog, prefs, "eyes", ME)
        .unwrap();
    assert!(add.adding());
    assert_eq!(add.name(), "eyes");
    assert!(add.current(&p, &domain, &catalog, prefs, &empty, ME));
    assert!(
        !add.current(
            &p,
            &FirstAddDomain::build().unwrap(),
            &catalog,
            prefs,
            &empty,
            ME
        ),
        "snapshot identity is distinct from equal definitions"
    );
    assert!(
        !add.current(&p, &domain, &newer, prefs, &empty, ME),
        "a different catalog identity cannot be borrowed"
    );
    assert!(!add.current(&p, &domain, &catalog, prefs, &empty, "U3"));
    assert!(
        empty
            .first_add_selection(&p, &domain, &catalog, &Preferences::default(), "eyes", ME)
            .is_err()
    );
    for unsupported in [
        "",
        ":eyes:",
        "Eyes",
        "wave",
        "thumbsup",
        "dancers",
        "wave::skin-tone-3",
    ] {
        assert!(
            empty
                .first_add_selection(&p, &domain, &catalog, prefs, unsupported, ME)
                .is_err()
        );
    }
    assert!(
        empty
            .first_add_selection(&p, &domain, &shadow, prefs, "eyes", ME)
            .is_err()
    );
    assert!(
        empty
            .first_add_selection(&p, &domain, &alias_shadow, prefs, "eyes", ME)
            .is_err()
    );

    let supplied = full(&original, json!([row("eyes", "U3")]));
    assert!(
        supplied
            .first_add_selection(&p, &domain, &catalog, prefs, "eyes", ME)
            .unwrap()
            .adding()
    );
    let owned = full(&original, json!([row("eyes", ME)]));
    let remove = owned
        .first_add_selection(&p, &domain, &catalog, prefs, "eyes", ME)
        .unwrap();
    assert!(!remove.adding());
    assert_eq!(remove.name(), "eyes");
    assert!(!add.current(&p, &domain, &catalog, prefs, &owned, ME));
    // The exact picker branch never expands a removal to an own raw family.
    // Unknown full-array names are membership data, not sendable picker names.
    let other_wire = full(&original, json!([row("eyes::skin-tone-3", ME)]));
    assert!(
        other_wire
            .first_add_selection(&p, &domain, &catalog, prefs, "eyes", ME)
            .unwrap()
            .adding()
    );
    let both = full(
        &original,
        json!([row("eyes", ME), row("eyes::skin-tone-3", ME)]),
    );
    assert_eq!(
        both.first_add_selection(&p, &domain, &catalog, prefs, "eyes", ME)
            .unwrap()
            .name(),
        "eyes"
    );
    assert!(
        !both
            .first_add_selection(&p, &domain, &catalog, prefs, "eyes", ME)
            .unwrap()
            .adding()
    );
    assert!(
        both.selection(&p, &catalog, prefs, "eyes", ME).is_err(),
        "no widening of old family selector"
    );

    let quota = |total: usize, own: usize, selected: bool| {
        let selected_rows = if selected { 1 } else { 0 };
        let mut rows = (0..total - selected_rows)
            .map(|n| row(&format!("other_{n}"), if n < own { ME } else { "U3" }))
            .collect::<Vec<_>>();
        if selected {
            rows.push(row("eyes", "U3"));
        }
        full(&original, json!(rows))
    };
    assert!(
        quota(22, 22, false)
            .first_add_selection(&p, &domain, &catalog, prefs, "eyes", ME)
            .is_ok()
    );
    assert!(
        quota(23, 23, false)
            .first_add_selection(&p, &domain, &catalog, prefs, "eyes", ME)
            .is_err()
    );
    assert!(
        quota(49, 0, false)
            .first_add_selection(&p, &domain, &catalog, prefs, "eyes", ME)
            .is_ok()
    );
    assert!(
        quota(50, 0, false)
            .first_add_selection(&p, &domain, &catalog, prefs, "eyes", ME)
            .is_err()
    );
    assert!(
        quota(50, 0, true)
            .first_add_selection(&p, &domain, &catalog, prefs, "eyes", ME)
            .is_ok()
    );
    let mut over = (0..50)
        .map(|n| row(&format!("other_{n}"), ME))
        .collect::<Vec<_>>();
    over.push(row("eyes", ME));
    assert!(
        !full(&original, json!(over))
            .first_add_selection(&p, &domain, &catalog, prefs, "eyes", ME)
            .unwrap()
            .adding(),
        "Remove does not inherit Add quotas"
    );

    let mut omitted = raw();
    assert!(
        RootMembership::decode(
            &original,
            &json!({"ok":true,"type":"message","channel":C,"message":omitted})
        )
        .is_err()
    );
    omitted["reactions"] = Value::Null;
    assert!(
        RootMembership::decode(
            &original,
            &json!({"ok":true,"type":"message","channel":C,"message":omitted})
        )
        .is_err()
    );
    let mut changed = prefs.clone();
    changed.apply_change("preferred_skin_tone", json!(4));
    assert!(!remove.current(&p, &domain, &catalog, &changed, &owned, ME));
    assert!(
        raw().get("reactions").is_none(),
        "no input normalization or fabricated empty array"
    );
}
