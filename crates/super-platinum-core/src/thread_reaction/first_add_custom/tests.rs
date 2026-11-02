use super::*;
use serde_json::json;

const C: &str = "C1";
const TS: &str = "1710000001.000001";
const ME: &str = "U1";
const NAME: &str = "custom_party";
const URL: &str = "https://emoji.slack-edge.com/T1/custom_party/ExactCase.png?x=%2F#original";
fn raw() -> Value {
    json!({"type":"message","channel":C,"ts":TS,"thread_ts":TS,"user":"U2","text":"Original custom Root","is_locked":false,"is_beyond_free_limit":false})
}
fn policy(original: &OriginalRoot, preferred: Value) -> RootPolicy {
    RootPolicy::capture(original,
        &json!({"ok":true,"messages":[raw()],"has_more":false}),
        &json!({"ok":true,"channel":{"id":C,"name":"ship","is_channel":true,"is_member":true,"is_private":false,"is_frozen":false}}),
        &json!({"ok":true,"prefs":{"hidden_users":"","preferred_skin_tone":preferred}}),
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
fn complete(map: Value) -> Complete {
    Complete::decode(json!({"ok":true,"emoji":map})).unwrap()
}
fn row(name: &str, user: &str) -> Value {
    json!({"name":name,"count":1,"users":[user]})
}
fn choice(domain: &CustomDomain, p: &RootPolicy, members: &RootMembership) -> CustomChoice {
    domain
        .choices(p, p.preferences(), members)
        .into_iter()
        .find(|c| c.name() == NAME)
        .unwrap()
}

#[test]
fn direct_custom_complete_definition_exact_wire_membership_quotas_and_snapshot_corpus() {
    let original = OriginalRoot::capture(C, TS, &raw()).unwrap();
    let p = policy(&original, json!(3));
    let empty = full(&original, json!([]));
    let value = complete(json!({
        (NAME):URL, "custom_alias":"alias:custom_party", "dangling":"alias:missing",
        "cycle":"alias:cycle", "another":"https://example.invalid/other.png",
        "bad_http":"http://example.invalid/x", "bad_cred":"https://user:secret@example.invalid/x",
        "bad_empty_cred":"https://@example.invalid/x", "bad_repair":"https:///example.invalid/x",
        "bad_space":"https://example.invalid/x y", "bad_slash":"https://example.invalid\\x",
        "bad_empty":"", "bad_scheme":"data:image/png;base64,abc",
        "Upper":"https://example.invalid/x", "bad:name":"https://example.invalid/x",
        "bad+name":"https://example.invalid/x", "bad\u{0000}":"https://example.invalid/x",
        "__proto__":"https://example.invalid/x", "constructor":"https://example.invalid/x",
        "prototype":"https://example.invalid/x", "tostring":"https://example.invalid/x"
    }));
    let domain = CustomDomain::build(&value).unwrap();
    let projected = domain.choices(&p, p.preferences(), &empty);
    assert_eq!(projected.len(), 2);
    assert!(projected.windows(2).all(|v| v[0].name() < v[1].name()));
    let c = choice(&domain, &p, &empty);
    assert_eq!(
        c.url(),
        URL,
        "validate while retaining the exact original literal"
    );
    let add = empty
        .first_add_custom_selection(&p, &domain, p.preferences(), &c, ME)
        .unwrap();
    assert!(add.adding());
    assert_eq!(add.name(), NAME);
    assert!(add.current(&p, &domain, p.preferences(), &empty, ME));

    // Exhaust BOTH complete standard base+alias tables, not a sparse native
    // builtin domain or just the active V16 table. Every collision stays out.
    let a = Catalog::build(Version::V16).unwrap();
    let b = Catalog::build(Version::V17).unwrap();
    let mut collisions = serde_json::Map::new();
    for standard in a.entries.iter().chain(&b.entries) {
        collisions.insert(standard.name.clone(), json!(URL));
    }
    collisions.insert(NAME.into(), json!(URL));
    let union = CustomDomain::build(&complete(Value::Object(collisions))).unwrap();
    let allowed = union.choices(&p, p.preferences(), &empty);
    assert_eq!(allowed.len(), 1);
    assert_eq!(allowed[0].name(), NAME);
    assert!(
        b.entries.iter().any(|s| a.get(&s.name).is_none()),
        "real V17-only coverage"
    );

    // A direct custom name never inherits tone or malformed-tone normalization.
    for tone in [
        Value::Null,
        json!(1),
        json!(6),
        json!("4"),
        json!("04"),
        json!([]),
        json!({}),
    ] {
        let next = policy(&original, tone);
        let pick = choice(&domain, &next, &empty);
        let selected = empty
            .first_add_custom_selection(&next, &domain, next.preferences(), &pick, ME)
            .unwrap();
        assert_eq!(selected.name(), NAME);
        assert!(selected.adding());
    }

    let own = full(&original, json!([row(NAME, ME)]));
    let remove = own
        .first_add_custom_selection(&p, &domain, p.preferences(), &c, ME)
        .unwrap();
    assert!(!remove.adding());
    assert_eq!(remove.name(), NAME);
    assert!(remove.current(&p, &domain, p.preferences(), &own, ME));
    assert!(!add.current(&p, &domain, p.preferences(), &own, ME));
    let peer = full(&original, json!([row(NAME, "U3")]));
    assert!(
        peer.first_add_custom_selection(&p, &domain, p.preferences(), &c, ME)
            .unwrap()
            .adding()
    );
    let alias_own = full(&original, json!([row("custom_alias", ME), row(NAME, "U3")]));
    assert!(
        alias_own
            .first_add_custom_selection(&p, &domain, p.preferences(), &c, ME)
            .unwrap()
            .adding(),
        "own alias is not ownership of direct target"
    );
    let both = full(&original, json!([row("custom_alias", ME), row(NAME, ME)]));
    assert!(
        !both
            .first_add_custom_selection(&p, &domain, p.preferences(), &c, ME)
            .unwrap()
            .adding()
    );
    assert_eq!(
        both.first_add_custom_selection(&p, &domain, p.preferences(), &c, ME)
            .unwrap()
            .name(),
        NAME
    );

    let fresh_domain = CustomDomain::build(&value).unwrap();
    assert!(!add.current(&p, &fresh_domain, p.preferences(), &empty, ME));
    let changed_url = CustomDomain::build(&complete(
        json!({(NAME):"https://example.invalid/replaced.png"}),
    ))
    .unwrap();
    assert!(
        empty
            .first_add_custom_selection(&p, &changed_url, p.preferences(), &c, ME)
            .is_err()
    );
    let changed_pref = policy(&original, json!(4));
    assert!(!add.current(
        &changed_pref,
        &domain,
        changed_pref.preferences(),
        &empty,
        ME
    ));
    assert!(!add.current(&p, &domain, p.preferences(), &empty, "U3"));
    let unknown = Preferences::default();
    assert!(domain.choices(&p, &unknown, &empty).is_empty());
    assert!(
        empty
            .first_add_custom_selection(&p, &domain, &unknown, &c, ME)
            .is_err()
    );
    let mut other_raw = raw();
    other_raw["user"] = json!("U4");
    let other_original = OriginalRoot::capture(C, TS, &other_raw).unwrap();
    let mut other_message = other_raw;
    other_message["reactions"] = json!([]);
    let other_members = RootMembership::decode(
        &other_original,
        &json!({"ok":true,"type":"message","channel":C,"message":other_message}),
    )
    .unwrap();
    assert!(
        domain
            .choices(&p, p.preferences(), &other_members)
            .is_empty()
    );
    assert!(
        other_members
            .first_add_custom_selection(&p, &domain, p.preferences(), &c, ME)
            .is_err()
    );

    // Full membership requires the actual array and exact original same-root
    // branch. Malformed rows cannot become proof of no own reaction.
    for malformed in [
        Value::Null,
        json!({}),
        json!([row(NAME, ME), row(NAME, "U3")]),
        json!([{"name":NAME,"count":2,"users":[ME,ME]}]),
        json!([{"name":NAME,"count":1,"users":[ME,"U3"]}]),
    ] {
        let mut message = raw();
        message["reactions"] = malformed;
        assert!(
            RootMembership::decode(
                &original,
                &json!({"ok":true,"type":"message","channel":C,"message":message})
            )
            .is_err()
        );
    }
    let mut omitted = raw();
    assert!(
        RootMembership::decode(
            &original,
            &json!({"ok":true,"type":"message","channel":C,"message":omitted})
        )
        .is_err()
    );
    omitted["reactions"] = json!([]);
    omitted.as_object_mut().unwrap().remove("thread_ts");
    assert!(
        RootMembership::decode(
            &original,
            &json!({"ok":true,"type":"message","channel":C,"message":omitted})
        )
        .is_err()
    );

    let quota = |total: usize, owned: usize, selected: Option<&str>| {
        let mut rows = (0..total)
            .map(|n| row(&format!("quota_{n}"), if n < owned { ME } else { "U3" }))
            .collect::<Vec<_>>();
        if let Some(who) = selected {
            rows.push(row(NAME, who));
        }
        full(&original, json!(rows))
    };
    assert!(
        quota(22, 22, None)
            .first_add_custom_selection(&p, &domain, p.preferences(), &c, ME)
            .is_ok()
    );
    assert!(
        quota(23, 23, None)
            .first_add_custom_selection(&p, &domain, p.preferences(), &c, ME)
            .is_err()
    );
    assert!(
        quota(49, 0, None)
            .first_add_custom_selection(&p, &domain, p.preferences(), &c, ME)
            .is_ok()
    );
    assert!(
        quota(50, 0, None)
            .first_add_custom_selection(&p, &domain, p.preferences(), &c, ME)
            .is_err()
    );
    assert!(
        quota(50, 0, Some("U3"))
            .first_add_custom_selection(&p, &domain, p.preferences(), &c, ME)
            .is_ok(),
        "existing exact name does not spend a new row"
    );
    assert!(
        !quota(51, 50, Some(ME))
            .first_add_custom_selection(&p, &domain, p.preferences(), &c, ME)
            .unwrap()
            .adding(),
        "Remove is not Add-quota limited"
    );

    // Bounds are explicit native supported-domain choices. Unsupported rows
    // do not poison valid siblings; oversized complete maps refuse as a whole.
    for n in [1, 64] {
        let name = "q".repeat(n);
        assert!(direct_name(&name));
    }
    assert!(!direct_name(&"q".repeat(65)));
    assert!(!direct_url(&format!(
        "https://example.invalid/{}",
        "q".repeat(MAX_URL_BYTES)
    )));
    let mut many = serde_json::Map::new();
    for n in 0..=MAX_ENTRIES {
        many.insert(format!("custom_{n}"), json!(URL));
    }
    assert!(CustomDomain::build(&complete(Value::Object(many))).is_err());
    let big = (0..4097)
        .map(|n| (format!("custom_{n}"), json!("x".repeat(4096))))
        .collect();
    assert!(CustomDomain::build(&complete(Value::Object(big))).is_err());
    assert!(Complete::decode(json!({"ok":true,"emoji":{(NAME):{"url":URL}}})).is_err());
    assert!(
        CustomDomain::build(&complete(json!({})))
            .unwrap()
            .choices(&p, p.preferences(), &empty)
            .is_empty()
    );

    // Existing glyph APIs/catalogue are not widened by a custom data domain.
    let old = super::super::first_add::FirstAddDomain::build().unwrap();
    let plain =
        crate::composer_emoji::catalog::Catalog::build_records(Version::V16, vec![]).unwrap();
    assert_eq!(old.choices(&plain).len(), 1582);
    assert!(
        empty
            .first_add_selection(&p, &old, &plain, p.preferences(), NAME, ME)
            .is_err()
    );
}
