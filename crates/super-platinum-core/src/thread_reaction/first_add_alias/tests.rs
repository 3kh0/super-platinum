use super::*;
use crate::composer_emoji::catalog::CustomRecord;
use serde_json::json;
const C: &str = "C1";
const TS: &str = "1710000001.000001";
const ME: &str = "U1";
fn raw() -> Value {
    json!({"type":"message","channel":C,"ts":TS,"thread_ts":TS,"user":"U2","text":"Alias root","is_locked":false,"is_beyond_free_limit":false})
}
fn policy(original: &OriginalRoot, pref: Value) -> RootPolicy {
    RootPolicy::capture(original,&json!({"ok":true,"messages":[raw()],"has_more":false}),
        &json!({"ok":true,"channel":{"id":C,"name":"ship","is_channel":true,"is_member":true,"is_private":false,"is_frozen":false}}),
        &json!({"ok":true,"prefs":{"hidden_users":"","preferred_skin_tone":pref}})).unwrap()
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
fn alias(
    domain: &AliasDomain,
    p: &RootPolicy,
    catalog: &Catalog,
    membership: &RootMembership,
    name: &str,
) -> AliasChoice {
    domain
        .choices(p, catalog, p.preferences(), membership)
        .into_iter()
        .find(|c| c.matched_alias() == name)
        .unwrap()
}
#[test]
fn alias_joint55_query_exact_wire_branches_ownership_retarget_refusal_snapshots_and_quotas() {
    let domain = AliasDomain::build().unwrap();
    let catalog = Catalog::build_records(Version::V16, vec![]).unwrap();
    let newer = Catalog::build_records(Version::V17, vec![]).unwrap();
    let original = OriginalRoot::capture(C, TS, &raw()).unwrap();
    let empty = full(&original, json!([]));
    let query = AliasQuery::parse(" \u{feff}:thumb:sup:\u{a0}").unwrap();
    assert_eq!(query.name(), "thumbsup");
    assert_eq!(query.literal(), " \u{feff}:thumb:sup:\u{a0}");
    for bad in [
        "",
        ": :",
        "\u{85}thumbsup",
        "THUMBSUP",
        "https://fixture.invalid",
    ] {
        assert!(
            AliasQuery::parse(bad).is_none(),
            "finite alias grammar: {bad:?}"
        );
    }
    for tone in 1..=6 {
        let p = policy(&original, json!(tone));
        let choices = domain.choices(&p, &catalog, p.preferences(), &empty);
        let v17 = domain.choices(&p, &newer, p.preferences(), &empty);
        assert_eq!(choices.len(), 55, "46 tone-free plus9 single-tone aliases");
        assert_eq!(
            v17.len(),
            55,
            "every version must supply the complete cohort"
        );
        assert!(
            choices
                .windows(2)
                .all(|w| w[0].matched_alias() < w[1].matched_alias())
        );
        assert!(
            choices
                .iter()
                .zip(&v17)
                .all(|(a, b)| a.matched_alias() == b.matched_alias()
                    && a.selected_name() == b.selected_name()
                    && a.wire_name() == b.wire_name()
                    && a.glyph() == b.glyph())
        );
        for c in &choices {
            let def = &domain.definitions[c.matched_alias()];
            assert_eq!(c.canonical_group(), def.canonical.name);
            assert_eq!(
                c.wire_name(),
                c.canonical_wire,
                "absent exact alias normalizes canonical"
            );
            let expected = catalog
                .standard
                .insertion(
                    c.matched_alias(),
                    (tone > 1).then_some(tone.to_string()).as_deref(),
                )
                .unwrap();
            assert_eq!(c.selected_name(), expected.name);
            assert_eq!(c.glyph(), expected.glyph);
            let add = empty
                .first_add_alias_selection(&p, &domain, &catalog, p.preferences(), c, ME)
                .unwrap();
            assert!(add.adding());
            assert_eq!(add.name(), c.canonical_wire);
            let canonical_self = full(&original, json!([row(&c.canonical_wire, ME)]));
            assert!(
                !canonical_self
                    .first_add_alias_selection(&p, &domain, &catalog, p.preferences(), c, ME)
                    .unwrap()
                    .adding()
            );
            let peer = full(
                &original,
                json!([row(c.selected_name(), "U2"), row(&c.canonical_wire, ME)]),
            );
            let observed = alias(&domain, &p, &catalog, &peer, c.matched_alias());
            assert_eq!(observed.wire_name(), c.selected_name());
            assert!(
                peer.first_add_alias_selection(
                    &p,
                    &domain,
                    &catalog,
                    p.preferences(),
                    &observed,
                    ME
                )
                .unwrap()
                .adding(),
                "own canonical is not family Remove of peer alias"
            );
            assert!(
                peer.first_add_alias_selection(&p, &domain, &catalog, p.preferences(), c, ME)
                    .is_err(),
                "alias appearance cannot retarget prepared canonical wire"
            );
            assert!(
                empty
                    .first_add_alias_selection(
                        &p,
                        &domain,
                        &catalog,
                        p.preferences(),
                        &observed,
                        ME
                    )
                    .is_err(),
                "alias disappearance cannot retarget prepared alias wire"
            );
            let own = full(
                &original,
                json!([row(c.selected_name(), ME), row(&c.canonical_wire, ME)]),
            );
            let own_choice = alias(&domain, &p, &catalog, &own, c.matched_alias());
            let remove = own
                .first_add_alias_selection(&p, &domain, &catalog, p.preferences(), &own_choice, ME)
                .unwrap();
            assert!(!remove.adding());
            assert_eq!(remove.name(), c.selected_name());
            assert!(remove.current(&p, &domain, &catalog, p.preferences(), &own, ME));
            assert!(!remove.current(&p, &domain, &newer, p.preferences(), &own, ME));
        }
    }
    let p = policy(&original, json!(3));
    let selected = alias(&domain, &p, &catalog, &empty, "thumbsup");
    assert!(query.matches(&selected));
    assert_eq!(selected.selected_name(), "thumbsup::skin-tone-3");
    assert_eq!(selected.wire_name(), "+1::skin-tone-3");
    assert_eq!(selected.glyph(), "👍🏼");
    assert!(
        !AliasQuery::parse("not_a_supported_alias")
            .unwrap()
            .matches(&selected)
    );
    for pref in [
        Value::Null,
        json!("3"),
        json!(true),
        json!(0),
        json!(7),
        json!(3.0),
    ] {
        let bad = policy(&original, pref);
        let choices = domain.choices(&bad, &catalog, bad.preferences(), &empty);
        assert_eq!(choices.len(), 46);
        assert!(!choices.iter().any(|c| c.matched_alias() == "thumbsup"));
        assert!(
            empty
                .first_add_alias_selection(
                    &bad,
                    &domain,
                    &catalog,
                    bad.preferences(),
                    &selected,
                    ME
                )
                .is_err()
        );
    }
    let changed = policy(&original, json!(4));
    assert!(
        empty
            .first_add_alias_selection(
                &changed,
                &domain,
                &catalog,
                changed.preferences(),
                &selected,
                ME
            )
            .is_err()
    );
    let other_domain = AliasDomain::build().unwrap();
    assert!(
        empty
            .first_add_alias_selection(&p, &other_domain, &catalog, p.preferences(), &selected, ME)
            .is_err()
    );
    let add = empty
        .first_add_alias_selection(&p, &domain, &catalog, p.preferences(), &selected, ME)
        .unwrap();
    assert!(!add.current(&p, &domain, &catalog, p.preferences(), &empty, "U3"));
    assert!(!add.current(&p, &other_domain, &catalog, p.preferences(), &empty, ME));
    for name in ["thumbsup", "+1"] {
        for value in ["https://fixture.invalid/custom.png", "alias:heart"] {
            let custom = Catalog::build_records(
                Version::V16,
                vec![CustomRecord {
                    name: name.into(),
                    value: value.into(),
                    is_nonexistent: false,
                }],
            )
            .unwrap();
            assert!(
                !domain
                    .choices(&p, &custom, p.preferences(), &empty)
                    .iter()
                    .any(|c| c.matched_alias() == "thumbsup")
            );
            assert!(
                empty
                    .first_add_alias_selection(&p, &domain, &custom, p.preferences(), &selected, ME)
                    .is_err()
            );
        }
    }
    let own23 = (0..23)
        .map(|n| row(&format!("existing_{n}"), ME))
        .collect::<Vec<_>>();
    assert!(
        full(&original, json!(own23))
            .first_add_alias_selection(&p, &domain, &catalog, p.preferences(), &selected, ME)
            .is_err()
    );
    let mut remove23 = own23;
    remove23[0] = row(selected.selected_name(), ME);
    let own = full(&original, json!(remove23));
    let owned = alias(&domain, &p, &catalog, &own, "thumbsup");
    assert!(
        !own.first_add_alias_selection(&p, &domain, &catalog, p.preferences(), &owned, ME)
            .unwrap()
            .adding()
    );
    let fifty = (0..50)
        .map(|n| row(&format!("existing_{n}"), "U2"))
        .collect::<Vec<_>>();
    assert!(
        full(&original, json!(fifty))
            .first_add_alias_selection(&p, &domain, &catalog, p.preferences(), &selected, ME)
            .is_err()
    );
    let mut existing = fifty;
    existing[0] = row(selected.wire_name(), "U2");
    assert!(
        full(&original, json!(existing))
            .first_add_alias_selection(&p, &domain, &catalog, p.preferences(), &selected, ME)
            .unwrap()
            .adding()
    );
    let old = super::super::first_add::FirstAddDomain::build()
        .unwrap()
        .choices(&catalog);
    assert_eq!(old.len(), 1582);
    let preferred = super::super::first_add_preferred::PreferredDomain::build()
        .unwrap()
        .choices(&catalog, p.preferences());
    assert_eq!(preferred.len(), 1892);
    assert!(!old.iter().any(|c| c.name() == "thumbsup"));
    assert!(!preferred.iter().any(|c| c.base() == "thumbsup"));
}
