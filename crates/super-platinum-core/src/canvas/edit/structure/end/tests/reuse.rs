use super::*;
mod recovery;

pub(super) fn reused_fixture(retained: bool) -> CanvasCollabSnapshot {
    let mut s = fixture("Café &amp; 日本😀", true);
    let i = s.sections.iter().position(|r| r.id() == "other").unwrap();
    let before = if retained {
        draft("Earlier child")
    } else {
        TextDraft::default()
    };
    s.sections[i] = change(&s.sections[i], m(12, &body(&before.rtml().unwrap(), true)));
    let seed = text::crdt::bootstrap(&s.sections[i], &before, "0123456789ab", 100).unwrap();
    let seed = if retained {
        text::crdt::update(
            &seed,
            &s.sections[i],
            &before,
            &TextDraft::default(),
            "abcdef012345",
            100,
        )
        .unwrap()
        .remove(12)
        .unwrap()
    } else {
        seed
    };
    let doc = Automerge::load(seed.bytes(5).unwrap().unwrap()).unwrap();
    let seq = s.sections[i].sequence().unwrap().unwrap();
    let wire = seed
        .replace_fields(4, &[n(4, 1_000_000)])
        .unwrap()
        .replace_fields(
            11,
            &[m(
                11,
                &msg(&[t(1, &doc.get_heads()[0].to_string()), n(2, seq)]),
            )],
        )
        .unwrap();
    s.sections[i] = change(
        &change(&s.sections[i], m(12, &body("", true))),
        m(38, &wire),
    );
    s
}
fn replace(s: &CanvasCollabSnapshot, id: &str, f: F) -> CanvasCollabSnapshot {
    let mut s = s.clone();
    let i = s.sections.iter().position(|r| r.id() == id).unwrap();
    s.sections[i] = change(&s.sections[i], f);
    s
}

#[test]
fn reuse_selects_exact_existing_final_child_without_creation_or_history_changes() {
    for retained in [false, true] {
        let s = reused_fixture(retained);
        let intent = EndIntent::capture(&s, "item", 100).unwrap();
        assert!(intent.reuses_child());
        let p = intent
            .prepare(&s, &witness("Café &amp; 日本😀"), 100)
            .unwrap();
        assert!(p.reuses_child());
        assert_eq!(p.child_section(), "other");
        let updates = p.data.messages(1).unwrap();
        assert_eq!(updates.len(), 2);
        for (update, id) in updates.iter().zip(["item", "other"]) {
            assert_eq!(
                update.fields().iter().map(|f| f.tag()).collect::<Vec<_>>(),
                [1, 2, 13, 19, 20, 33, 38]
            );
            assert_eq!(update.string(1).unwrap(), Some(id));
            assert_eq!(
                update.uint(2).unwrap(),
                s.section(id).unwrap().sequence().unwrap()
            );
            assert_eq!(
                update.message(38).unwrap().unwrap(),
                validation::project_parent_session(
                    &s.section(id).unwrap().crdt().unwrap().unwrap()
                )
                .unwrap()
            );
            assert!(!update.has(8) && !update.has(12) && !update.has(21));
        }
        assert!(p.checkpoint().unwrap().is_end_reuse());
        let rows = pair(&p, &s);
        let accepted = p.validate_ack(&ack(&rows)).unwrap();
        assert!(accepted.reuses_child());
        let fresh = readback(&s, &rows);
        assert!(accepted.confirms_readback(&fresh).unwrap());
        assert_eq!(fresh.sections.len(), s.sections.len());
        assert_eq!(
            fresh.section("other").unwrap().raw().string(8).unwrap(),
            s.section("other").unwrap().raw().string(8).unwrap()
        );
        let edited = save_text(&fresh, "other", "Existing child now has 👩🏽\u{200d}💻 text");
        assert_eq!(edited.section("item"), fresh.section("item"));
        assert_eq!(edited.sections.len(), s.sections.len());
        let final_state = save_text(&edited, "item", "Café &amp; 日本😀 again");
        assert_eq!(final_state.section("other"), edited.section("other"));
        let old = Automerge::load(p.child_wire.bytes(5).unwrap().unwrap()).unwrap();
        let current = Automerge::load(
            final_state
                .section("other")
                .unwrap()
                .crdt()
                .unwrap()
                .unwrap()
                .bytes(5)
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        assert!(
            old.get_changes(&[])
                .iter()
                .all(|c| current.get_change_by_hash(&c.hash()).is_some())
        );
    }
}

#[test]
fn reuse_refuses_nonliteral_source_empty_default_or_unsupported_child() {
    let s = reused_fixture(true);
    for value in [
        " ",
        "&nbsp;",
        "\u{200b}",
        "\u{feff}",
        "<br>",
        "<br><br>",
        "<b> </b>",
        "<i></i>",
        "<a href=\"https://example.invalid\"> </a>",
    ] {
        let bad = replace(&s, "other", m(12, &body(value, true)));
        assert!(
            EndIntent::capture(&bad, "item", 100).is_err(),
            "source-empty {value:?}"
        );
    }
    // This pinned source predicate is not Unicode is_whitespace: ZWJ/ZWNJ
    // contain content. Default-content metadata may hide a nonempty label.
    for value in ["Another paragraph", "\u{200c}", "\u{200d}"] {
        let changed = replace(&s, "other", m(12, &body(value, true)));
        assert!(
            !EndIntent::capture(&changed, "item", 100)
                .unwrap()
                .reuses_child()
        );
    }
    let visible = replace(&s, "other", m(12, &body("Visible placeholder", true)));
    let bad = replace(&visible, "other", m(16, &msg(&[n(13, 1)])));
    assert!(EndIntent::capture(&bad, "item", 100).is_err());
    for field in [
        n(35, 1),
        n(10, 1),
        n(29, 1),
        m(16, &msg(&[t(29, "content-id")])),
    ] {
        assert!(EndIntent::capture(&replace(&s, "other", field), "item", 100).is_err());
    }
    let wire = s.section("other").unwrap().crdt().unwrap().unwrap();
    for bad_wire in [
        wire.replace_fields(2, &[n(2, 0)]).unwrap(),
        wire.replace_fields(10, &[m(10, &msg(&[n(1, 0)]))]).unwrap(),
    ] {
        assert!(EndIntent::capture(&replace(&s, "other", m(38, &bad_wire)), "item", 100).is_err());
    }
    let mut missing = s.clone();
    let i = missing
        .sections
        .iter()
        .position(|r| r.id() == "other")
        .unwrap();
    missing.sections[i] = Section::from_raw(missing.sections[i].raw().remove(38).unwrap()).unwrap();
    assert!(EndIntent::capture(&missing, "item", 100).is_err());
}

#[test]
fn reuse_is_immediate_and_final_and_binds_child_freshness_including_expiry() {
    let s = reused_fixture(true);
    let mut nonadjacent = s.clone();
    let between = change(
        &change(s.section("item").unwrap(), t(1, "between")),
        t(8, "aad"),
    );
    nonadjacent.sections.push(between);
    assert!(EndIntent::capture(&nonadjacent, "item", 100).is_err());
    let mut not_final = s.clone();
    let later = change(
        &change(s.section("item").unwrap(), t(1, "later")),
        t(8, "aag"),
    );
    not_final.sections.push(later);
    assert!(
        !EndIntent::capture(&not_final, "item", 100)
            .unwrap()
            .reuses_child()
    );
    let new = prepare(&not_final);
    assert!(!new.reuses_child());
    assert!(!new.checkpoint().unwrap().is_end_reuse());
    for f in [
        n(2, 20000),
        m(12, &body("changed", true)),
        n(35, 1),
        t(8, "aaz"),
    ] {
        let intent = EndIntent::capture(&s, "item", 100).unwrap();
        assert!(
            intent
                .prepare(&replace(&s, "other", f), &witness("Café &amp; 日本😀"), 100)
                .is_err()
        );
    }
    let wire = s
        .section("other")
        .unwrap()
        .crdt()
        .unwrap()
        .unwrap()
        .replace_fields(4, &[n(4, 150)])
        .unwrap();
    let expires = replace(&s, "other", m(38, &wire));
    let p = prepare(&expires);
    assert!(p.validate_snapshot(&expires, 200).is_err());
    assert!(
        EndIntent::capture(&expires, "item", 100)
            .unwrap()
            .prepare(&expires, &witness("Café &amp; 日本😀"), 200)
            .is_err()
    );
}

#[test]
fn reuse_ack_and_readback_require_original_ids_histories_contents_and_count() {
    let s = reused_fixture(true);
    let p = prepare(&s);
    let rows = pair(&p, &s);
    let good = readback(&s, &rows);
    for one in [&rows[..1], &rows[1..]] {
        assert!(p.validate_ack(&ack(one)).is_err());
    }
    for (i, f) in [
        (1, t(1, "duplicate-child")),
        (
            1,
            n(2, s.section("other").unwrap().sequence().unwrap().unwrap()),
        ),
        (1, t(8, "aaz")),
        (1, m(12, &body("", false))),
        (1, m(12, &body("new", true))),
        (1, n(19, 0)),
        (1, n(20, 2)),
    ] {
        let mut bad = rows.clone();
        bad[i] = change(&bad[i], f);
        assert!(p.validate_ack(&ack(&bad)).is_err());
    }
    let mut rebuilt = rows.clone();
    let wire = text::crdt::bootstrap(&rebuilt[1], &TextDraft::default(), "111111111111", 100)
        .unwrap()
        .replace_fields(4, &[n(4, 1_000_000)])
        .unwrap();
    rebuilt[1] = change(&rebuilt[1], m(38, &wire));
    assert!(p.validate_ack(&ack(&rebuilt)).is_err());
    let metadata = rows
        .iter()
        .map(|r| Section::from_raw(r.raw().remove(12).unwrap()).unwrap())
        .collect::<Vec<_>>();
    let accepted = p.validate_ack(&ack(&metadata)).unwrap();
    assert!(accepted.confirms_readback(&good).unwrap());
    assert!(
        !accepted
            .confirms_readback(&readback(&s, &metadata))
            .unwrap_or(false)
    );
    let mut duplicate = good.clone();
    duplicate
        .sections
        .push(change(good.section("other").unwrap(), t(1, "unexpected")));
    assert!(!accepted.confirms_readback(&duplicate).unwrap_or(false));
    let changed = replace(&good, "other", n(2, 90000));
    assert!(!accepted.confirms_readback(&changed).unwrap_or(false));
    let mut missing = good.clone();
    missing.sections.retain(|r| r.id() != "other");
    assert!(!accepted.confirms_readback(&missing).unwrap_or(false));
}
