use super::*;
use automerge::marks::Mark;

const ID: &str = "temp:C:ABC0123456789abcdef012345678";

#[test]
fn range_keeps_spanning_format_instead_of_wrapping_fragments() {
    let draft = TextDraft::from_rtml("<b>ABC</b>").unwrap();
    let value = range::capture(&draft, 1, 2, ID).unwrap();
    assert_eq!(
        value.rtml,
        format!("<b>A<annotation id=\"{ID}\">B</annotation>C</b>")
    );
    assert!(value.selected == TextDraft::from_rtml("<b>B</b>").unwrap());
    assert_eq!((value.escaped_start, value.escaped_end), (1, 2));
    assert_eq!(value.text, "ABC");
    assert_eq!(value.marks.last().unwrap().name(), "annotation");
}

#[test]
fn native_offsets_map_entities_breaks_and_astral_scalars_exactly() {
    let draft = TextDraft::from_rtml("A&amp;\u{1f600}<br>&lt;Z").unwrap();
    let value = range::capture(&draft, 1, 6, ID).unwrap();
    assert_eq!(value.text, "A&amp;\u{1f600}<br>&lt;Z");
    assert_eq!((value.escaped_start, value.escaped_end), (1, 16));
    assert_eq!(
        value.rtml,
        format!("A<annotation id=\"{ID}\">&amp;\u{1f600}<br>&lt;</annotation>Z")
    );
    assert!(value.selected == TextDraft::from_rtml("&amp;\u{1f600}<br>&lt;").unwrap());
    for (start, end) in [(0, 0), (3, 4), (2, 3), (0, 99), (6, 1)] {
        assert!(range::capture(&draft, start, end, ID).is_err());
    }
}

#[test]
fn retained_compactor_overlap_golden_is_not_per_run_serialization() {
    // Exact output from retained getCompactRtml/sortBy, not from our renderer.
    let marks = vec![
        Mark::new("i".into(), true, 2, 4),
        Mark::new("a".into(), "{\"href\":\"https://example.com\"}", 0, 3),
        Mark::new("annotation".into(), ID, 1, 4),
    ];
    assert_eq!(
        compact::render("ABCD", &marks).unwrap(),
        format!(
            "<a href=\"https://example.com\">A</a><annotation id=\"{ID}\"><a href=\"https://example.com\">B<i>C</i></a><i>D</i></annotation>"
        )
    );
}

#[test]
fn false_marks_do_not_wrap_and_equal_density_order_is_stable() {
    let marks = vec![
        Mark::new("b".into(), false, 0, 2),
        Mark::new("i".into(), true, 0, 2),
        Mark::new("b".into(), true, 0, 2),
        Mark::new("annotation".into(), ID, 0, 2),
    ];
    assert_eq!(
        compact::render("AB", &marks).unwrap(),
        format!("<annotation id=\"{ID}\"><i><b>AB</b></i></annotation>")
    );
}

#[test]
fn unsupported_attributes_marks_and_half_surrogates_refuse() {
    for mark in [
        Mark::new("unknown".into(), true, 0, 1),
        Mark::new("annotation".into(), "bad\"id", 0, 1),
        Mark::new("a".into(), "{\"href\":\"x\\\"y\"}", 0, 1),
        Mark::new("a".into(), "{\"href\":\"x\",\"nolink\":true}", 0, 1),
        Mark::new("b".into(), true, 1, 2),
        Mark::new("b".into(), true, 0, 3),
    ] {
        assert!(compact::render("\u{1f600}", &[mark]).is_err());
    }
    assert!(
        compact::render("A", &vec![Mark::new("b".into(), true, 0, 1); 4097]).is_err(),
        "corrupt repeated marks refuse before recursive tree construction"
    );
    assert!(TextDraft::from_rtml(&format!("<annotation id=\"{ID}\">A</annotation>")).is_err());
}

fn section(id: &str, rtml: &str) -> crate::canvas::wire::Section {
    use crate::canvas::wire::{EncodedField as F, RawMessage, Section};
    let text = RawMessage::from_fields(&[F::bytes(1, rtml.as_bytes()).unwrap()]).unwrap();
    let content = RawMessage::from_fields(&[F::message(1, &text).unwrap()]).unwrap();
    Section::from_raw(
        RawMessage::from_fields(&[
            F::bytes(1, id.as_bytes()).unwrap(),
            F::uint(2, 1001).unwrap(),
            F::bytes(6, b"thread").unwrap(),
            F::bytes(7, b"document").unwrap(),
            F::message(12, &content).unwrap(),
        ])
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn actual_automerge_mark_keeps_original_text_and_format_history() {
    use automerge::ReadDoc;
    let draft = TextDraft::from_rtml("<b>A&amp;\u{1f600}B</b>").unwrap();
    let parent = section("parent", "<b>A&amp;\u{1f600}B</b>");
    let model = range::capture(&draft, 1, 4, ID).unwrap();
    let prepared = history::prepare(
        &parent,
        &existing::from_rtml(&draft.rtml().unwrap()).unwrap(),
        &model,
        "012345abcdef",
        1_000_000,
    )
    .unwrap();
    let doc = history::validate(&prepared.wire, &parent, &model, None).unwrap();
    let (_, object) = doc.get(automerge::ROOT, "text").unwrap().unwrap();
    assert_eq!(doc.text(&object).unwrap(), "A&amp;\u{1f600}B\u{feff}");
    assert_eq!(
        prepared.rtml,
        format!("<b>A<annotation id=\"{ID}\">&amp;\u{1f600}</annotation>B</b>")
    );
    assert_eq!(prepared.expires, None);
    assert!(
        super::super::super::text::crdt::validate(&prepared.wire, &parent, &draft, None).is_err()
    );
    let wrong = range::capture(&draft, 0, 1, ID).unwrap();
    assert!(history::validate(&prepared.wire, &parent, &wrong, None).is_err());
}

#[test]
fn modern_zero_base_ack_needs_complete_processed_history_and_exact_mark() {
    use crate::canvas::wire::{EncodedField as F, Section};
    let draft = TextDraft::from_rtml("ABCD").unwrap();
    let parent = section("temp:C:ABCabcdef0123456789abcdef012", "ABCD");
    let seed =
        super::super::super::text::crdt::bootstrap(&parent, &draft, "111111111111", 1_000_000)
            .unwrap()
            .replace_fields(2, &[F::uint(2, 0).unwrap()])
            .unwrap()
            .replace_fields(3, &[F::uint(3, 1001).unwrap()])
            .unwrap()
            .replace_fields(4, &[F::uint(4, 20_000_000).unwrap()])
            .unwrap();
    let parent = Section::from_raw(
        parent
            .raw()
            .replace_fields(38, &[F::message(38, &seed).unwrap()])
            .unwrap(),
    )
    .unwrap();
    let model = range::capture(&draft, 1, 3, ID).unwrap();
    let prepared = history::prepare(
        &parent,
        &existing::from_rtml(&draft.rtml().unwrap()).unwrap(),
        &model,
        "012345abcdef",
        2_000_000,
    )
    .unwrap();
    assert_eq!(prepared.wire.uint(2).unwrap(), Some(0));
    assert_eq!(prepared.expires, Some(20_000_000));
    let ack = prepared
        .wire
        .replace_fields(3, &[F::uint(3, 2001).unwrap()])
        .unwrap()
        .replace_fields(4, &[F::uint(4, 30_000_000).unwrap()])
        .unwrap();
    let parent = Section::from_raw(
        parent
            .raw()
            .replace_fields(2, &[F::uint(2, 2001).unwrap()])
            .unwrap(),
    )
    .unwrap();
    assert!(history::validate(&ack, &parent, &model, Some(3_000_000)).is_ok());
    assert!(history::validate(&ack.remove(6).unwrap(), &parent, &model, Some(3_000_000)).is_err());
    assert!(history::validate(&ack, &parent, &model, Some(30_000_000)).is_err());
    let wrong = range::capture(&draft, 0, 1, ID).unwrap();
    assert!(history::validate(&ack, &parent, &wrong, Some(3_000_000)).is_err());
}
