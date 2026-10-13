use super::*;

fn message(fields: Vec<F>) -> RawMessage {
    RawMessage::from_fields(&fields).unwrap()
}
fn string(tag: u32, value: &str) -> F {
    F::bytes(tag, value.as_bytes()).unwrap()
}
fn number(tag: u32, value: u64) -> F {
    F::uint(tag, value).unwrap()
}
fn nested(tag: u32, value: RawMessage) -> F {
    F::message(tag, &value).unwrap()
}
fn title_section(text: &str) -> Section {
    Section::from_raw(message(vec![
        string(1, "title-section"),
        number(2, 10_000),
        string(6, "ThTest"),
        string(7, "Dtest"),
        number(9, 64),
        number(10, 48),
        nested(
            12,
            message(vec![
                nested(
                    1,
                    message(vec![string(1, "legacy sibling is not authoritative")]),
                ),
                nested(58, message(vec![string(1, text)])),
            ]),
        ),
    ]))
    .unwrap()
}
fn snapshot(text: &str) -> CanvasCollabSnapshot {
    crate::slack::api::canvas_collab_test_snapshot(
        vec![title_section(text)],
        message(vec![
            string(1, "Dtest"),
            number(2, 10_000),
            string(6, "ThTest"),
        ]),
    )
}
fn leaf_text(title: &CanonicalTitle) -> String {
    let root = title.rich_text();
    assert_eq!(root.string(1).unwrap(), Some("rich_text"));
    let group = root.message(2).unwrap().unwrap();
    assert_eq!(group.string(1).unwrap(), Some("rich_text_section"));
    let leaf = group.message(2).unwrap().unwrap();
    assert_eq!(leaf.string(1).unwrap(), Some("text"));
    assert_eq!(root.fields().len(), 2);
    assert_eq!(group.fields().len(), 2);
    assert_eq!(leaf.fields().len(), 2);
    leaf.string(2).unwrap().unwrap().to_owned()
}

#[test]
fn missing_document_metadata_synthesizes_exact_three_level_proto_from_static_title() {
    let mut source = snapshot("A native canvas");
    assert!(!source.document.has(69));
    let original = source.sections[0].clone();
    let title = CanonicalTitle::capture(&source).unwrap();
    assert_eq!(title.form_title(), "A native canvas");
    assert_eq!(leaf_text(&title), "A native canvas");
    assert_eq!(
        source.sections[0], original,
        "synthesis must not edit title content"
    );
    // Even when present, a cached Document.69 is not the editor's save source.
    source.document = source
        .document
        .replace_fields(
            69,
            &[nested(
                69,
                message(vec![string(1, "stale cached metadata")]),
            )],
        )
        .unwrap();
    assert_eq!(
        leaf_text(&CanonicalTitle::capture(&source).unwrap()),
        "A native canvas"
    );
    title.revalidate(&source).unwrap();
}

#[test]
fn export_normalizes_spaces_and_controls_then_only_form_title_trims() {
    let cases = [
        ("A\0B", "AB", "AB"),
        ("  A\u{a0}B\u{200b}\u{feff} C  ", "  A B C  ", "A B C"),
        ("A\tB\r\nC\rD\nE", "A B C D E", "A B C D E"),
        ("A\u{1}B\u{7f}C\u{85}D\u{9f}E", "ABCDE", "ABCDE"),
        (
            "\u{2003}雪 👩🏽‍💻 e\u{301}\u{202f}",
            "\u{2003}雪 👩🏽‍💻 e\u{301}\u{202f}",
            "雪 👩🏽‍💻 e\u{301}",
        ),
    ];
    for (source, rich, form) in cases {
        let title = CanonicalTitle::capture(&snapshot(source)).unwrap();
        assert_eq!(leaf_text(&title), rich);
        assert_eq!(title.form_title(), form);
    }
}

#[test]
fn utf16_limits_are_independent_and_never_split_a_rust_character_silently() {
    let input = format!("{}😀{}", "a".repeat(253), "b".repeat(2000));
    let title = CanonicalTitle::capture(&snapshot(&input)).unwrap();
    assert_eq!(title.form_title(), format!("{}😀", "a".repeat(253)));
    assert_eq!(leaf_text(&title).encode_utf16().count(), 2000);
    assert_eq!(
        leaf_text(&title),
        format!("{}😀{}", "a".repeat(253), "b".repeat(1745))
    );
    // Captured JS protobuf emits invalid UTF-8 for a slice ending in a lone high
    // surrogate. This bounded native slice explicitly declines those titles.
    for input in [
        format!("{}😀", "a".repeat(254)),
        format!("{}😀", "a".repeat(1999)),
    ] {
        assert!(CanonicalTitle::capture(&snapshot(&input)).is_err());
    }
    let exactly = "雪".repeat(2000);
    let title = CanonicalTitle::capture(&snapshot(&exactly)).unwrap();
    assert_eq!(title.form_title(), "雪".repeat(255));
    assert_eq!(leaf_text(&title), exactly);
}

#[test]
fn unsupported_rich_empty_ambiguous_and_misidentified_titles_do_not_fall_back() {
    for text in [
        "",
        " \u{a0}\u{200b}\u{feff}\t",
        "<b>rich</b>",
        "A &amp; B",
        "A > B",
    ] {
        assert!(CanonicalTitle::capture(&snapshot(text)).is_err());
    }
    assert!(CanonicalTitle::capture(&snapshot(&"x".repeat(MAX_TITLE_SOURCE_BYTES + 1))).is_err());
    let original = snapshot("title");
    for replacement in [
        number(9, 0),
        number(10, 0),
        string(6, "OtherThread"),
        string(7, "OtherDoc"),
        number(2, 0),
    ] {
        let tag = RawMessage::decode(replacement.raw()).unwrap().fields()[0].tag();
        let mut source = snapshot("title");
        source.sections[0] = Section::from_raw(
            source.sections[0]
                .raw()
                .replace_fields(tag, &[replacement])
                .unwrap(),
        )
        .unwrap();
        assert!(CanonicalTitle::capture(&source).is_err());
    }
    let mut source = original;
    let duplicate = Section::from_raw(
        source.sections[0]
            .raw()
            .replace_fields(1, &[string(1, "another-title")])
            .unwrap(),
    )
    .unwrap();
    source.sections.push(duplicate.clone());
    assert!(CanonicalTitle::capture(&source).is_err());
    source.sections[1] =
        Section::from_raw(duplicate.raw().replace_fields(3, &[number(3, 1)]).unwrap()).unwrap();
    assert!(
        CanonicalTitle::capture(&source).is_ok(),
        "deleted title is not a candidate"
    );
    source.sections.clear();
    assert!(CanonicalTitle::capture(&source).is_err());
}

#[test]
fn preflight_binds_identity_revision_and_all_canonical_title_fields() {
    let source = snapshot("title");
    let title = CanonicalTitle::capture(&source).unwrap();
    title.revalidate(&source).unwrap();
    for fields in [
        vec![number(2, 11_000)],
        vec![string(1, "replacement-title")],
        vec![number(99, 1)],
        vec![nested(
            12,
            message(vec![nested(58, message(vec![string(1, "changed")]))]),
        )],
    ] {
        let mut changed = snapshot("title");
        for field in fields {
            let tag = RawMessage::decode(field.raw()).unwrap().fields()[0].tag();
            changed.sections[0] = Section::from_raw(
                changed.sections[0]
                    .raw()
                    .replace_fields(tag, &[field])
                    .unwrap(),
            )
            .unwrap();
        }
        assert!(title.revalidate(&changed).is_err());
    }
}
