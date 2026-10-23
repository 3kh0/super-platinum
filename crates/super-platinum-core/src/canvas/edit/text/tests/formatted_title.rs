use super::*;

pub(crate) fn fixture_with_title(source: &str) -> CanvasCollabSnapshot {
    let mut snapshot = fixture();
    replace_title(&mut snapshot, source);
    let title = snapshot
        .sections
        .iter_mut()
        .find(|s| s.id() == "title")
        .unwrap();
    *title = Section::from_raw(
        title
            .raw()
            .replace_fields(99, &[t(99, "opaque title sibling")])
            .unwrap(),
    )
    .unwrap();
    snapshot
}

fn replace_title(snapshot: &mut CanvasCollabSnapshot, source: &str) {
    let title = snapshot
        .sections
        .iter_mut()
        .find(|s| s.id() == "title")
        .unwrap();
    let content = title.raw().message(12).unwrap().unwrap();
    let value = content
        .message(58)
        .unwrap()
        .unwrap()
        .replace_fields(1, &[t(1, source)])
        .unwrap();
    let content = content.replace_fields(58, &[m(58, &value)]).unwrap();
    *title =
        Section::from_raw(title.raw().replace_fields(12, &[m(12, &content)]).unwrap()).unwrap();
}

#[test]
fn formatted_title_allows_paragraph_prepare_without_changing_its_canonical_source() {
    let snapshot = fixture_with_title(" <b>Plan &amp; Review</b> <i>&lt;draft&gt;</i> ");
    let original = snapshot.sections.clone();
    let intent = TextIntent::capture(&snapshot, "item", 100).unwrap();
    assert!(intent.seed() == &canonical_draft(&original[0]).unwrap());
    let mut session = EditSession::new();
    let prepared = intent
        .prepare(&snapshot, &mut session, &changed(), 100)
        .unwrap();
    assert_eq!(prepared.form_title(), "Plan & Review <draft>");
    let data = prepared.document_data();
    assert_eq!(
        data.fields().iter().map(|f| f.tag()).collect::<Vec<_>>(),
        [1, 14]
    );
    let updates = data.messages(1).unwrap();
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].string(1).unwrap(), Some("item"));
    assert_eq!(
        updates[0].message(12).unwrap().unwrap(),
        content(&original[0], &changed()).unwrap()
    );
    let expected = msg(&[
        t(1, "rich_text"),
        m(
            2,
            &msg(&[
                t(1, "rich_text_section"),
                m(2, &msg(&[t(1, "text"), t(2, " Plan & Review <draft> ")])),
            ]),
        ),
    ]);
    assert_eq!(data.message(14).unwrap().unwrap(), expected);
    assert_eq!(snapshot.sections, original);
    prepared.validate_snapshot(&snapshot, 100).unwrap();
}

#[test]
fn equivalent_title_text_does_not_replace_original_byte_and_revision_authority() {
    let original = fixture_with_title("<b>Plan &amp; Review</b>");
    let intent = TextIntent::capture(&original, "item", 100).unwrap();
    let mut equivalent = original.clone();
    replace_title(&mut equivalent, "<i>Plan &#38; Review</i>");
    let mut revision = original.clone();
    let title = revision
        .sections
        .iter_mut()
        .find(|s| s.id() == "title")
        .unwrap();
    let next = title.sequence().unwrap().unwrap() + 1;
    *title = Section::from_raw(title.raw().replace_fields(2, &[n(2, next)]).unwrap()).unwrap();
    for fresh in [&equivalent, &revision] {
        assert_eq!(
            CanonicalTitle::capture(fresh).unwrap().form_title(),
            "Plan & Review"
        );
        let mut session = EditSession::new();
        let id = session.id.clone();
        assert!(matches!(
            intent.prepare(fresh, &mut session, &changed(), 100),
            Err(Error::Changed)
        ));
        assert_eq!(session.id, id);
        assert_eq!(session.sequence, 0);
        assert!(session.local.is_empty());
        assert!(session.text_retained.is_empty());
    }
    let prepared = intent
        .prepare(&original, &mut EditSession::new(), &changed(), 100)
        .unwrap();
    assert_eq!(
        prepared.validate_snapshot(&equivalent, 100),
        Err(Error::Changed)
    );
    assert_eq!(
        prepared.validate_snapshot(&revision, 100),
        Err(Error::Changed)
    );
}
