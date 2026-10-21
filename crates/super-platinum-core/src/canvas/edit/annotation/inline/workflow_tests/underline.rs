use super::*;
use automerge::ReadDoc;

#[test]
fn underlined_annotation_round_trip_keeps_body_history_and_exact_two_section_evidence() {
    let mut s = fixture();
    let index = s
        .sections
        .iter()
        .position(|section| section.id() == "item")
        .unwrap();
    s.sections[index] = changed(
        &s.sections[index],
        m(
            12,
            &raw(&[m(1, &raw(&[t(1, "<u><b>A&amp;\u{1f642}B</b></u>")]))]),
        ),
    );
    let original = s.section("item").unwrap().raw().clone();
    let p = prepare(&s);
    let (control, parent) = received(&p, &s);
    let original_model = super::super::existing::parse(s.section("item").unwrap()).unwrap();
    let desired =
        super::super::range::capture_existing(&original_model, 1, 4, p.control_id()).unwrap();
    let doc =
        super::super::history::validate(&parent.crdt().unwrap().unwrap(), &parent, &desired, None)
            .unwrap();
    let (_, text) = doc.get(automerge::ROOT, "text").unwrap().unwrap();
    let marks = doc.marks(&text).unwrap();
    for name in ["u", "b", "annotation"] {
        assert!(marks.iter().any(|mark| mark.name() == name));
    }
    let canonical_rtml = parent
        .raw()
        .message(12)
        .unwrap()
        .unwrap()
        .message(1)
        .unwrap()
        .unwrap();
    assert!(canonical_rtml.string(1).unwrap().unwrap().contains("<u>"));
    assert_eq!(s.section("item").unwrap().raw(), &original);
    let accepted = p
        .validate_ack(&ack(&[control.clone(), parent.clone()]))
        .unwrap();
    assert_eq!(
        accepted.evidence().unwrap().anchor.as_deref(),
        Some(parent.raw().raw())
    );
    let current = fresh(&s, control, parent);
    accepted.check_confirmation(&current).unwrap();
    accepted.confirm(&current).unwrap();
}
