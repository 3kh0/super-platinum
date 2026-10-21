use super::*;
use serde_json::json;

const CHANNEL: &str = "C01234567";
const ROOT: &str = "1700000000.000001";
const TS: &str = "1700000001.000001";

fn page(reactions: Value) -> Page {
    let (doc, row, root) = super::super::super::tests::seed();
    Page::parse(
        &Target::capture(&doc, &row).unwrap(),
        json!({
            "ok":true,"messages":[root,{
                "type":"message","channel":CHANNEL,"ts":TS,"thread_ts":ROOT,
                "text":"Original rich reply","opaque":{"keep":[1,2]},
                "reactions":reactions
            }],"has_more":true,"response_metadata":{"next_cursor":"original+/cursor"}
        }),
    )
    .unwrap()
}
fn reaction(name: &str, mine: bool) -> Value {
    json!({"name":name,"count":9,"users":if mine {vec!["U_SELF"]} else {vec!["U_OTHER"]}})
}
fn capture(page: &Page, selected: &str) -> Result<ReactionFamilyIntent, ListError> {
    ReactionFamilyIntent::capture(
        page.target(),
        page.root().unwrap(),
        &page.messages()[1],
        json!({"id":CHANNEL}),
        "U_SELF",
        selected,
    )
}

#[test]
fn names_agree_exhaustively_with_retained_v16_and_v17_metadata() {
    use crate::composer_emoji::standard::{Catalog, Version};
    for version in [Version::V16, Version::V17] {
        let catalog = Catalog::build(version).unwrap();
        for entry in &catalog.entries {
            assert_eq!(reaction_family_base(&entry.name), Some(entry.name.as_str()));
            for variant in entry.skin_variations.values() {
                assert_eq!(
                    reaction_family_base(&variant.name),
                    Some(entry.name.as_str()),
                    "{}",
                    variant.name
                );
            }
        }
    }
    assert!(names::BASES.windows(2).all(|w| w[0] < w[1]));
    assert!(names::TONES.windows(2).all(|w| w[0].0 < w[1].0));
}

#[test]
fn permissive_parser_edges_are_not_sendable_names() {
    for name in [
        ":wave:",
        "WAVE",
        " wave ",
        "wave::anything-4",
        "wave::skin-tone-2-3tail",
        "wave::skin-tone-2-3-4",
        "wave::skin-tone-7-2",
        "wave::skin-tone-04",
        "wave::skin-tone-1",
        "wave::skin-tone-7",
        "wave::skin-tone-2::skin-tone-3",
        "wave::skin-tone-2-3",
        "fixture_custom",
    ] {
        assert_eq!(reaction_family_base(name), None, "{name}");
    }
    assert_eq!(
        reaction_family_base("handshake::skin-tone-2-3"),
        Some("handshake")
    );
}

#[test]
fn unowned_selection_captures_own_base_and_siblings_in_original_order() {
    let page = page(json!([
        reaction("wave::skin-tone-5", true),
        reaction("wave", true),
        reaction("wave::skin-tone-2", false),
        reaction("wave::skin-tone-3", true),
        reaction("smile", true)
    ]));
    let raw_page = page.raw().clone();
    let mut intent = capture(&page, "wave::skin-tone-2").unwrap();
    let record = intent.record().clone();
    assert_eq!(record.base(), "wave");
    assert_eq!(record.selected(), "wave::skin-tone-2");
    assert_eq!(
        record
            .children()
            .iter()
            .map(ReactionRecord::name)
            .collect::<Vec<_>>(),
        ["wave::skin-tone-5", "wave", "wave::skin-tone-3"]
    );
    assert!(record.current(
        page.target(),
        page.root().unwrap(),
        &page.messages()[1],
        &json!({"id":CHANNEL})
    ));
    for expected in record.children() {
        let child = intent.take_next().unwrap();
        assert_eq!(child.record(), expected);
        assert!(!child.record().adding());
        assert!(child.record().current(
            page.target(),
            page.root().unwrap(),
            &page.messages()[1],
            &json!({"id":CHANNEL})
        ));
        assert_eq!(
            child.record().message_raw()["opaque"],
            json!({"keep":[1,2]})
        );
        assert_eq!(child.accept(json!({"ok":true})).unwrap().record(), expected);
    }
    assert!(intent.take_next().is_none());
    assert!(intent.take_next().is_none());
    assert_eq!(page.raw(), &raw_page);
    assert_eq!(page.cursor(), Some("original+/cursor"));
}

#[test]
fn aliases_are_distinct_raw_families_and_paired_variants_are_exact() {
    let page = page(json!([
        reaction("+1", true),
        reaction("thumbsup::skin-tone-4", true),
        reaction("thumbsup", false),
        reaction("+1::skin-tone-5", true),
        reaction("handshake::skin-tone-2-3", true),
        reaction("handshake::skin-tone-3-2", true)
    ]));
    for (selected, expected) in [
        ("thumbsup", vec!["thumbsup::skin-tone-4"]),
        ("+1", vec!["+1", "+1::skin-tone-5"]),
        (
            "handshake::skin-tone-2-3",
            vec!["handshake::skin-tone-2-3", "handshake::skin-tone-3-2"],
        ),
    ] {
        assert_eq!(
            capture(&page, selected)
                .unwrap()
                .record()
                .children()
                .iter()
                .map(ReactionRecord::name)
                .collect::<Vec<_>>(),
            expected
        );
    }
}

#[test]
fn unknown_self_membership_never_creates_removals_or_additions() {
    let page = page(json!([
        reaction("wave", false),
        reaction("wave::skin-tone-2", false)
    ]));
    assert!(
        own_reaction_family_names(&page.messages()[1], "U_SELF", "wave")
            .unwrap()
            .is_empty()
    );
    assert!(capture(&page, "wave").is_err());
    assert!(capture(&page, "smile").is_err());
    assert!(own_reaction_family_names(&page.messages()[1], "", "wave").is_err());
}

#[test]
fn malformed_membership_and_unsupported_selected_family_refuse_atomically() {
    for reactions in [
        json!([reaction("wave", true), reaction("wave", true)]),
        json!([{"name":"wave","count":1,"users":["U_SELF","U_SELF"]}]),
        json!([{"name":"wave","count":0,"users":["U_SELF"]}]),
        json!([reaction("wave", true), reaction("wave::anything-4", true)]),
        json!([
            reaction("wave", true),
            reaction("wave::skin-tone-7-2", true)
        ]),
    ] {
        assert!(capture(&page(reactions), "wave").is_err());
    }
    // Unrelated valid custom rows remain outside this standard family.
    assert!(
        capture(
            &page(json!([
                reaction("wave", true),
                reaction("fixture_custom", true)
            ])),
            "wave"
        )
        .is_ok()
    );
}

#[test]
fn same_policy_and_target_guards_apply_before_any_child_is_available() {
    let page = page(json!([
        reaction("wave", true),
        reaction("wave::skin-tone-2", true)
    ]));
    let root = page.root().unwrap();
    let message = &page.messages()[1];
    for channel in [
        json!({"id":"C_FOREIGN"}),
        json!({"id":CHANNEL,"is_frozen":true}),
        json!({"id":CHANNEL,"is_im":true,"is_pending_ext_shared":true}),
    ] {
        assert!(
            ReactionFamilyIntent::capture(page.target(), root, message, channel, "U_SELF", "wave")
                .is_err()
        );
    }
    let mut locked = root.clone();
    locked.extra.insert("is_locked".into(), json!(true));
    assert!(
        ReactionFamilyIntent::capture(
            page.target(),
            &locked,
            message,
            json!({"id":CHANNEL}),
            "U_SELF",
            "wave"
        )
        .is_err()
    );
    for patch in [
        json!({"channel":"C_FOREIGN"}),
        json!({"thread_ts":"1700000099.000001"}),
        json!({"ts":ROOT}),
        json!({"no_display":true}),
        json!({"subtype":"tombstone"}),
    ] {
        let mut raw = raw(message).unwrap();
        raw.as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        let changed = serde_json::from_value(raw).unwrap();
        assert!(
            ReactionFamilyIntent::capture(
                page.target(),
                root,
                &changed,
                json!({"id":CHANNEL}),
                "U_SELF",
                "wave"
            )
            .is_err()
        );
    }
    // No posting or archive policy is added to the established reaction gate.
    assert!(
        ReactionFamilyIntent::capture(
            page.target(),
            root,
            message,
            json!({"id":CHANNEL,"is_archived":true,"is_read_only":true}),
            "U_SELF",
            "wave"
        )
        .is_ok()
    );
}

#[test]
fn family_records_reuse_response_and_full_readback_without_exact_capture() {
    let page = page(json!([reaction("wave::skin-tone-2", true)]));
    let family = capture(&page, "wave::skin-tone-2").unwrap();
    let child = &family.record().children()[0];
    assert!(
        ReactionIntent::capture(
            page.target(),
            page.root().unwrap(),
            &page.messages()[1],
            json!({"id":CHANNEL}),
            "U_SELF",
            "wave::skin-tone-2"
        )
        .is_err()
    );
    assert_eq!(
        child.classify(&json!({"ok":false,"error":"no_reaction"})),
        ReactionResponse::DesiredState
    );
    assert_eq!(
        child.classify(&json!({"ok":false,"error":"fatal_error"})),
        ReactionResponse::Uncertain
    );
    assert_eq!(
        child.classify(&json!({"ok":false,"error":"ratelimited"})),
        ReactionResponse::Rejected
    );
    let full = json!({"ok":true,"type":"message","channel":CHANNEL,"message":{
        "ts":TS,"thread_ts":ROOT,"reactions":[]}});
    assert!(child.observed_full(&full).unwrap());
    assert!(child.full_reactions(&full).unwrap().unwrap().is_empty());
    assert!(!child.observed(&page).unwrap());
}

#[test]
fn exact_currentness_does_not_redefine_fixed_original_children_after_a_removal() {
    let page = page(json!([
        reaction("wave", true),
        reaction("wave::skin-tone-2", true)
    ]));
    let mut family = capture(&page, "wave").unwrap();
    let original = family.record().clone();
    let first = family.take_next().unwrap();
    let mut changed = page.messages()[1].clone();
    changed.reactions.remove(0);
    assert!(!original.current(
        page.target(),
        page.root().unwrap(),
        &changed,
        &json!({"id":CHANNEL})
    ));
    // Host can account for its issued child's socket removal without recapture:
    // remaining original intent is still owned, has original full row, and is
    // consumed once. These facts alone do not authorize dispatch after change.
    first.accept(json!({"ok":true})).unwrap();
    let second = family.take_next().unwrap();
    assert_eq!(second.record().name(), "wave::skin-tone-2");
    assert_eq!(
        second.record().message_raw(),
        original.children()[0].message_raw()
    );
    assert!(family.take_next().is_none());
}
