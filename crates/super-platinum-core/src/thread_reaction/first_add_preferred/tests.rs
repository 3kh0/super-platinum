use super::*;
use crate::composer_emoji::catalog::CustomRecord;
use serde_json::json;
const C: &str = "C1";
const TS: &str = "1710000001.000001";
const ME: &str = "U1";
fn raw() -> Value {
    json!({"type":"message","channel":C,"ts":TS,"thread_ts":TS,"user":"U2","text":"Preferred root","is_locked":false,"is_beyond_free_limit":false})
}
fn policy(original: &OriginalRoot, pref: Value) -> RootPolicy {
    RootPolicy::capture(original,
        &json!({"ok":true,"messages":[raw()],"has_more":false}),
        &json!({"ok":true,"channel":{"id":C,"name":"ship","is_channel":true,"is_member":true,"is_private":false,"is_frozen":false}}),
        &json!({"ok":true,"prefs":{"hidden_users":"","preferred_skin_tone":pref}}),
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
fn selected(
    domain: &PreferredDomain,
    catalog: &Catalog,
    policy: &RootPolicy,
    base: &str,
) -> PreferredChoice {
    domain
        .choices(catalog, policy.preferences())
        .into_iter()
        .find(|v| v.base() == base)
        .unwrap()
}

#[test]
fn preferred_joint310_exact_six_tones_glyphs_membership_changed_preferences_and_quota_corpus() {
    let domain = PreferredDomain::build().unwrap();
    let a = Catalog::build_records(Version::V16, vec![]).unwrap();
    let b = Catalog::build_records(Version::V17, vec![]).unwrap();
    let original = OriginalRoot::capture(C, TS, &raw()).unwrap();
    let empty = full(&original, json!([]));
    let old = super::super::first_add::FirstAddDomain::build()
        .unwrap()
        .choices(&a);
    assert_eq!(
        old.len(),
        1582,
        "the old tone-free contract stays unchanged"
    );
    let mut toned = 0;
    for tone in 1..=6 {
        let p = policy(&original, json!(tone));
        let choices = domain.choices(&a, p.preferences());
        let newer = domain.choices(&b, p.preferences());
        assert_eq!(choices.len(), 1892);
        assert!(choices.windows(2).all(|w| w[0].base() < w[1].base()));
        assert!(
            choices.iter().zip(&newer).all(|(a, b)| a.base() == b.base()
                && a.name() == b.name()
                && a.glyph() == b.glyph())
        );
        for choice in &choices {
            let builtin = a.standard.get(choice.base()).unwrap();
            let expected = a
                .standard
                .insertion(
                    choice.base(),
                    (tone > 1).then_some(tone.to_string()).as_deref(),
                )
                .unwrap();
            assert_eq!(choice.name(), expected.name);
            assert_eq!(choice.glyph(), expected.glyph);
            let toggle = empty
                .first_add_preferred_selection(&p, &domain, &a, p.preferences(), choice, ME)
                .unwrap();
            assert!(toggle.adding());
            assert_eq!(toggle.name(), choice.name());
            if builtin.skin_variations.is_empty() {
                assert!(
                    old.iter()
                        .any(|o| o.name() == choice.name() && o.glyph() == choice.glyph())
                );
            } else {
                if tone > 1 {
                    toned += 1;
                }
                assert!(!builtin.multi_skin_tone && builtin.alias_of.is_none());
                assert_eq!(builtin.skin_variations.len(), 5);
            }
        }
    }
    assert_eq!(toned, 1550, "310 bases times five explicit variants");
    let p = policy(&original, json!(3));
    let wave = selected(&domain, &a, &p, "wave");
    assert_eq!(wave.name(), "wave::skin-tone-3");
    assert_eq!(wave.glyph(), "👋🏼");
    assert_eq!(selected(&domain, &a, &p, "+1").name(), "+1::skin-tone-3");
    for invalid in [
        Value::Null,
        json!("3"),
        json!(true),
        json!(0),
        json!(7),
        json!(3.0),
        json!([]),
        json!({}),
    ] {
        let malformed = policy(&original, invalid);
        let choices = domain.choices(&a, malformed.preferences());
        assert_eq!(choices.len(), 1582);
        assert!(!choices.iter().any(|v| v.base() == "wave"));
    }
    assert!(domain.choices(&a, &Preferences::default()).is_empty());
    for pref in [json!(1), json!(2), json!(4), json!("3"), Value::Null] {
        let changed = policy(&original, pref);
        assert!(
            empty
                .first_add_preferred_selection(
                    &changed,
                    &domain,
                    &a,
                    changed.preferences(),
                    &wave,
                    ME
                )
                .is_err(),
            "a displayed choice cannot silently recolor"
        );
    }
    let own_other = full(&original, json!([row("wave::skin-tone-4", ME)]));
    assert!(
        own_other
            .first_add_preferred_selection(&p, &domain, &a, p.preferences(), &wave, ME)
            .unwrap()
            .adding()
    );
    let own_same = full(
        &original,
        json!([row("wave::skin-tone-3", ME), row("wave::skin-tone-4", ME)]),
    );
    let remove = own_same
        .first_add_preferred_selection(&p, &domain, &a, p.preferences(), &wave, ME)
        .unwrap();
    assert!(!remove.adding());
    assert_eq!(remove.name(), wave.name());
    assert!(remove.current(&p, &domain, &a, p.preferences(), &own_same, ME));
    assert!(!remove.current(&p, &domain, &a, p.preferences(), &own_other, ME));
    assert!(!remove.current(&p, &domain, &b, p.preferences(), &own_same, ME));
    assert!(!remove.current(
        &p,
        &PreferredDomain::build().unwrap(),
        &a,
        p.preferences(),
        &own_same,
        ME
    ));
    assert!(!remove.current(&p, &domain, &a, p.preferences(), &own_same, "U3"));
    assert!(
        empty
            .first_add_preferred_selection(
                &p,
                &PreferredDomain::build().unwrap(),
                &a,
                p.preferences(),
                &wave,
                ME
            )
            .is_err()
    );
    let base_policy = policy(&original, json!(1));
    let base = selected(&domain, &a, &base_policy, "wave");
    assert_eq!(base.name(), "wave");
    assert_eq!(base.glyph(), "👋");
    assert!(
        empty
            .first_add_preferred_selection(
                &base_policy,
                &domain,
                &a,
                base_policy.preferences(),
                &base,
                ME
            )
            .unwrap()
            .adding()
    );
    for value in ["https://fixture.invalid/wave.png", "alias:heart"] {
        let shadow = Catalog::build_records(
            Version::V16,
            vec![CustomRecord {
                name: "wave".into(),
                value: value.into(),
                is_nonexistent: false,
            }],
        )
        .unwrap();
        assert_eq!(domain.choices(&shadow, p.preferences()).len(), 1891);
        assert!(
            !domain
                .choices(&shadow, p.preferences())
                .iter()
                .any(|v| v.base() == "wave")
        );
        assert!(
            empty
                .first_add_preferred_selection(&p, &domain, &shadow, p.preferences(), &wave, ME)
                .is_err()
        );
    }
    let own23 = (0..23)
        .map(|n| row(&format!("existing_{n}"), ME))
        .collect::<Vec<_>>();
    let capped = full(&original, json!(own23));
    assert!(
        capped
            .first_add_preferred_selection(&p, &domain, &a, p.preferences(), &wave, ME)
            .is_err()
    );
    let mut own_remove = own23;
    own_remove[0] = row(wave.name(), ME);
    assert!(
        !full(&original, json!(own_remove))
            .first_add_preferred_selection(&p, &domain, &a, p.preferences(), &wave, ME)
            .unwrap()
            .adding()
    );
    let fifty = (0..50)
        .map(|n| row(&format!("existing_{n}"), "U2"))
        .collect::<Vec<_>>();
    assert!(
        full(&original, json!(fifty))
            .first_add_preferred_selection(&p, &domain, &a, p.preferences(), &wave, ME)
            .is_err()
    );
    let mut existing = fifty;
    existing[0] = row(wave.name(), "U2");
    assert!(
        full(&original, json!(existing))
            .first_add_preferred_selection(&p, &domain, &a, p.preferences(), &wave, ME)
            .unwrap()
            .adding()
    );
}
