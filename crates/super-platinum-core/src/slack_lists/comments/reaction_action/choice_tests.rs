use super::*;
use crate::composer_emoji::{
    catalog::{Catalog, CustomRecord},
    preferences::Preferences,
    standard::Version,
};
use serde_json::json;
const CHANNEL: &str = "C01234567";
fn page(reactions: Value) -> Page {
    let (doc, row, root) = super::super::tests::seed();
    Page::parse(
        &Target::capture(&doc, &row).unwrap(),
        json!({"ok":true,"messages":[root,{
        "type":"message","ts":"1700000001.000001","thread_ts":"1700000000.000001",
        "channel":CHANNEL,"text":"Keep body","opaque":{"keep":[1,2]},"reactions":reactions
    }],"has_more":true,"response_metadata":{"next_cursor":"retained"}}),
    )
    .unwrap()
}
fn catalog() -> Catalog {
    Catalog::build_records(Version::V17, vec![]).unwrap()
}
fn choice(catalog: &Catalog, page: &Page, name: &str) -> ReactionChoice {
    let mut prefs = Preferences::default();
    prefs.replace(&std::collections::BTreeMap::from([(
        "preferred_skin_tone".into(),
        json!(4),
    )]));
    ReactionChoice::resolve(catalog, &prefs, &page.messages()[1].reactions, name).unwrap()
}
fn capture(page: &Page, choice: ReactionChoice) -> Result<ReactionIntent, ListError> {
    ReactionIntent::capture_choice(
        page.target(),
        page.root().unwrap(),
        &page.messages()[1],
        json!({"id":CHANNEL}),
        "U_SELF",
        choice,
    )
}
fn reaction(name: &str, mine: bool) -> Value {
    json!({"name":name,"count":1,"users":[if mine {"U_SELF"} else {"U_OTHER"}]})
}

#[test]
fn new_resolved_name_can_add_and_recover_using_original_inert_record() {
    let c = catalog();
    let page = page(json!([]));
    let before = page.raw().clone();
    let intent = capture(&page, choice(&c, &page, "thumbsup")).unwrap();
    assert_eq!(intent.record().name(), "+1::skin-tone-4");
    assert!(intent.record().adding());
    assert_eq!(
        intent.record().message_raw()["opaque"],
        json!({"keep":[1,2]})
    );
    assert!(intent.record().current(
        page.target(),
        page.root().unwrap(),
        &page.messages()[1],
        &json!({"id":CHANNEL})
    ));
    let record = intent.record().clone();
    assert_eq!(intent.accept(json!({"ok":true})).unwrap().record(), &record);
    assert!(
        record
            .observed_full(&json!({"ok":true,"type":"message","channel":CHANNEL,
        "message":{"ts":record.ts(),"reactions":[reaction("+1::skin-tone-4",true)]}}))
            .unwrap()
    );
    assert_eq!(page.raw(), &before);
    assert_eq!(page.cursor(), Some("retained"));
}

#[test]
fn picker_owned_tone_removes_exact_name_only_not_family() {
    let c = catalog();
    let page = page(json!([
        reaction("wave::skin-tone-2", true),
        reaction("wave::skin-tone-4", true),
        reaction("wave", true)
    ]));
    let intent = capture(&page, choice(&c, &page, "wave::skin-tone-2")).unwrap();
    assert_eq!(intent.record().name(), "wave::skin-tone-2");
    assert!(!intent.record().adding());
    // Exact picker chooses preferred4 for base wave, then toggles only that row.
    let base = capture(&page, choice(&c, &page, "wave")).unwrap();
    assert_eq!(base.record().name(), "wave::skin-tone-4");
    assert!(!base.record().adding());
    assert!(
        ReactionIntent::capture(
            page.target(),
            page.root().unwrap(),
            &page.messages()[1],
            json!({"id":CHANNEL}),
            "U_SELF",
            "wave"
        )
        .is_err()
    );
}

#[test]
fn distinct_limit_uses_final_exact_name_while_own_limit_counts_raw_rows() {
    let c = catalog();
    for (count, existing, allowed) in [(49, false, true), (50, false, false), (50, true, true)] {
        let mut rows: Vec<_> = (0..count)
            .map(|i| reaction(&format!("fixture_{i}"), false))
            .collect();
        if existing {
            rows[0] = reaction("wave::skin-tone-4", false)
        }
        let p = page(json!(rows));
        assert_eq!(capture(&p, choice(&c, &p, "wave")).is_ok(), allowed);
    }
    for (count, allowed) in [(22, true), (23, false)] {
        let rows: Vec<_> = (0..count)
            .map(|i| reaction(&format!("fixture_{i}"), true))
            .collect();
        let p = page(json!(rows));
        assert_eq!(capture(&p, choice(&c, &p, "wave")).is_ok(), allowed);
    }
    let mut rows: Vec<_> = (0..23)
        .map(|i| reaction(&format!("fixture_{i}"), true))
        .collect();
    rows.push(reaction("wave::skin-tone-4", true));
    let p = page(json!(rows));
    assert!(
        !capture(&p, choice(&c, &p, "wave"))
            .unwrap()
            .record()
            .adding()
    );
}

#[test]
fn nonexistent_custom_refuses_add_but_does_not_block_owned_exact_remove() {
    let c = Catalog::build_records(
        Version::V17,
        vec![CustomRecord {
            name: "gone".into(),
            value: "https://fixture.test/gone".into(),
            is_nonexistent: true,
        }],
    )
    .unwrap();
    for mine in [false, true] {
        let p = page(json!([reaction("gone", mine)]));
        let intent = capture(&p, choice(&c, &p, "gone"));
        assert_eq!(intent.is_ok(), mine);
        if let Ok(intent) = intent {
            assert!(!intent.record().adding());
        }
    }
}

#[test]
fn changed_membership_duplicate_members_and_policy_refuse_choice_capture() {
    let c = catalog();
    let initial = page(json!([]));
    let selected = choice(&c, &initial, "wave");
    let changed = page(json!([reaction("wave::skin-tone-4", true)]));
    assert!(capture(&changed, selected).is_err());
    let malformed = page(json!([reaction("wave", true), reaction("wave", true)]));
    assert!(capture(&malformed, choice(&c, &malformed, "wave")).is_err());
    for channel in [
        json!({"id":"C_FOREIGN"}),
        json!({"id":CHANNEL,"is_frozen":true}),
    ] {
        assert!(
            ReactionIntent::capture_choice(
                initial.target(),
                initial.root().unwrap(),
                &initial.messages()[1],
                channel,
                "U_SELF",
                choice(&c, &initial, "wave")
            )
            .is_err()
        );
    }
    let mut locked = initial.root().unwrap().clone();
    locked.extra.insert("is_locked".into(), json!(true));
    assert!(
        ReactionIntent::capture_choice(
            initial.target(),
            &locked,
            &initial.messages()[1],
            json!({"id":CHANNEL}),
            "U_SELF",
            choice(&c, &initial, "wave")
        )
        .is_err()
    );
}

fn pill(c: &Catalog, p: &Page, name: &str, user: &str) -> ReactionChoice {
    let mut prefs = Preferences::default();
    prefs.replace(&std::collections::BTreeMap::from([(
        "preferred_skin_tone".into(),
        json!(4),
    )]));
    ReactionChoice::resolve_pill_addition(c, &prefs, &p.messages()[1].reactions, name, user)
        .unwrap()
}

#[test]
fn pill_addition_captures_original_actor_and_cannot_flip_on_fresh_ownership() {
    let c = catalog();
    let original = page(json!([reaction("wave", false)]));
    let selected = pill(&c, &original, "wave", "U_SELF");
    let intent = capture(&original, selected.clone()).unwrap();
    assert!(intent.record().adding());
    assert_eq!(intent.record().name(), "wave::skin-tone-4");
    let changed = page(json!([
        reaction("wave", false),
        reaction("wave::skin-tone-2", true)
    ]));
    assert!(capture(&changed, selected.clone()).is_err());
    assert!(!intent.record().current(
        changed.target(),
        changed.root().unwrap(),
        &changed.messages()[1],
        &json!({"id":CHANNEL})
    ));
    assert!(
        ReactionIntent::capture_choice(
            original.target(),
            original.root().unwrap(),
            &original.messages()[1],
            json!({"id":CHANNEL}),
            "U_OTHER",
            selected
        )
        .is_err()
    );
    let record = intent.record().clone();
    assert_eq!(intent.accept(json!({"ok":true})).unwrap().record(), &record);
}

#[test]
fn pill_resolved_addition_uses_final_name_for_distinct_and_own_limits() {
    let c = catalog();
    for (count, resolved_exists, allowed) in
        [(49, false, true), (50, false, false), (50, true, true)]
    {
        let mut r: Vec<_> = (0..count)
            .map(|i| reaction(&format!("fixture_{i}"), false))
            .collect();
        r[0] = reaction("wave", false);
        if resolved_exists {
            r[1] = reaction("wave::skin-tone-4", false)
        }
        let p = page(json!(r));
        assert_eq!(capture(&p, pill(&c, &p, "wave", "U_SELF")).is_ok(), allowed);
    }
    for (owned, allowed) in [(22, true), (23, false)] {
        let mut r: Vec<_> = (0..owned)
            .map(|i| reaction(&format!("fixture_{i}"), true))
            .collect();
        r.push(reaction("handshake", false));
        let p = page(json!(r));
        let result = capture(&p, pill(&c, &p, "handshake", "U_SELF"));
        assert_eq!(result.is_ok(), allowed);
        if let Ok(intent) = result {
            assert_eq!(intent.record().name(), "handshake::skin-tone-4");
        }
    }
}

#[test]
fn pill_addition_reuses_policy_and_preserves_original_raw_custody() {
    let c = catalog();
    let p = page(json!([reaction("handshake::skin-tone-2-6", false)]));
    let raw = p.raw().clone();
    let selected = pill(&c, &p, "handshake::skin-tone-2-6", "U_SELF");
    for channel in [
        json!({"id":"C_FOREIGN"}),
        json!({"id":CHANNEL,"is_frozen":true}),
    ] {
        assert!(
            ReactionIntent::capture_choice(
                p.target(),
                p.root().unwrap(),
                &p.messages()[1],
                channel,
                "U_SELF",
                selected.clone()
            )
            .is_err()
        );
    }
    let intent = capture(&p, selected).unwrap();
    assert!(intent.record().adding());
    assert_eq!(intent.record().name(), "handshake::skin-tone-2-6");
    assert_eq!(
        intent.record().message_raw()["opaque"],
        json!({"keep":[1,2]})
    );
    assert_eq!(p.raw(), &raw);
    assert_eq!(p.cursor(), Some("retained"));
}
