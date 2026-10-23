//! Fixed existing-final-empty fixture. Original history is never regenerated.
use super::*;
#[path = "reuse_clearing.rs"]
mod clearing;
#[path = "reuse_zero.rs"]
mod zero;

pub(super) fn content() -> &'static str {
    clearing::selector()
}
pub(super) fn source_rtml() -> &'static str {
    clearing::rtml(content())
}

pub(super) fn base() -> &'static str {
    if std::env::var("SUPER_PLATINUM_CANVAS_REUSE_BASE").as_deref() == Ok("zero") {
        "zero"
    } else {
        "positive"
    }
}
pub(super) fn valid() -> bool {
    clearing::valid()
        && match std::env::var("SUPER_PLATINUM_CANVAS_REUSE_BASE") {
            Ok(value) => value == "positive" || (value == "zero" && is_reuse()),
            Err(std::env::VarError::NotPresent) => true,
            Err(_) => false,
        }
}
fn id(zero: bool) -> &'static str {
    if zero { zero::ID } else { "empty-final" }
}
pub(super) fn child_id() -> &'static str {
    id(base() == "zero")
}

pub(super) fn html() -> &'static str {
    if content() != "empty" {
        return clearing::html(base() == "zero", corpus().name == "unicode", content());
    }
    if base() == "zero" {
        return zero::html(corpus().name == "unicode");
    }
    if corpus().name == "unicode" {
        "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Caf\u{e9} &amp; \u{65e5}\u{672c}\u{1f600}</p><p id='empty-final'></p></div>"
    } else {
        "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='neighbor'>Another paragraph</p><p id='paragraph'>Keep this</p><p id='empty-final'></p></div>"
    }
}

fn child_wire(zero: bool) -> RawMessage {
    if zero {
        return zero::wire();
    }
    // Actual original-client clean empty session, section empty-final/base1000.
    // Provenance: tmp/canvas-end-reuse-20261023/reference/empty-final-seed.json.
    // The original serializer omits expiry; only this independent lease is added.
    let seeds: Value = serde_json::from_str(include_str!("reuse-original.json")).unwrap();
    let bytes = STANDARD
        .decode(seeds["empty-final"].as_str().unwrap())
        .unwrap();
    RawMessage::decode(&bytes)
        .unwrap()
        .replace_fields(4, &[n(4, 8_000_000_000_000_000)])
        .unwrap()
}

pub(super) fn snapshot(previous: &CanvasCollabSnapshot) -> Arc<CanvasCollabSnapshot> {
    snapshot_with_content(previous, base() == "zero", content())
}
#[cfg(test)]
fn snapshot_with_zero(previous: &CanvasCollabSnapshot, zero: bool) -> Arc<CanvasCollabSnapshot> {
    snapshot_with_content(previous, zero, "empty")
}
fn snapshot_with_content(
    previous: &CanvasCollabSnapshot,
    zero: bool,
    shape: &str,
) -> Arc<CanvasCollabSnapshot> {
    assert_eq!(previous.sections.len(), 3);
    let child = id(zero);
    let parent = previous.section("paragraph").unwrap();
    let empty = parent
        .raw()
        .replace_fields(1, &[t(1, child)])
        .unwrap()
        .replace_fields(12, &[m(12, &body(clearing::rtml(shape), false))])
        .unwrap()
        .replace_fields(
            38,
            &[m(
                38,
                &if shape == "empty" {
                    child_wire(zero)
                } else {
                    clearing::wire(zero, shape)
                },
            )],
        )
        .unwrap();
    let rows = [
        ("title", "aaa"),
        ("neighbor", "aac"),
        ("paragraph", "aae"),
        (child, "aag"),
    ]
    .into_iter()
    .map(|(id, position)| {
        let raw = if id == child {
            &empty
        } else {
            previous.section(id).unwrap().raw()
        };
        raw.replace_fields(8, &[t(8, position)]).unwrap()
    })
    .collect();
    rebuild(previous, rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super_platinum_core::canvas::edit::{structure::EndWitness, text::TextDraft};

    #[test]
    fn fixed_reuse_inventory_admits_existing_empty_identity_with_retained_history() {
        for (zero, corpus) in [false, true].into_iter().flat_map(|zero| {
            [&END_ASCII, &END_UNICODE]
                .into_iter()
                .map(move |corpus| (zero, corpus))
        }) {
            let previous = super::super::super::structure_fixture::snapshot();
            let rows = previous
                .sections
                .iter()
                .map(|row| {
                    if row.id() == "paragraph" {
                        row.raw()
                            .replace_fields(12, &[m(12, &body(corpus.seed_rtml, true))])
                            .unwrap()
                            .replace_fields(38, &[m(38, &end_original::wire(corpus.name))])
                            .unwrap()
                    } else {
                        row.raw().clone()
                    }
                })
                .collect();
            let source = snapshot_with_zero(&rebuild(&previous, rows), zero);
            assert_eq!(
                source
                    .sections
                    .iter()
                    .map(|row| row.id())
                    .collect::<Vec<_>>(),
                ["title", "neighbor", "paragraph", id(zero)]
            );
            let intent = EndIntent::capture(&source, "paragraph", 1_000_000).unwrap();
            assert!(intent.reuses_child());
            let witness = EndWitness::new(
                intent.seed(),
                corpus.offset,
                intent.seed(),
                &TextDraft::default(),
            )
            .unwrap();
            let prepared = intent.prepare(&source, &witness, 1_000_000).unwrap();
            assert!(prepared.reuses_child());
            assert_eq!(prepared.child_section(), id(zero));
            assert_eq!(
                source
                    .section(id(zero))
                    .unwrap()
                    .crdt()
                    .unwrap()
                    .unwrap()
                    .uint(2)
                    .unwrap(),
                Some(if zero { 0 } else { 1000 })
            );
            assert!(prepared.checkpoint().unwrap().is_end_reuse());
            let updates = prepared
                .document_data()
                .fields()
                .iter()
                .filter(|field| field.tag() == 1)
                .collect::<Vec<_>>();
            assert_eq!(updates.len(), 2);
            for field in updates {
                let update =
                    RawMessage::decode(prepared.document_data().encoded_field(field).unwrap())
                        .unwrap()
                        .message(1)
                        .unwrap()
                        .unwrap();
                assert!(!update.has(8) && !update.has(12) && !update.has(21));
                let original = source
                    .section(update.string(1).unwrap().unwrap())
                    .unwrap()
                    .raw()
                    .message(38)
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    update.message(38).unwrap().unwrap().raw(),
                    original.remove(4).unwrap().raw()
                );
            }
        }
    }
    #[test]
    fn modern_zero_fixture_accepts_correction_and_erase_on_the_same_child() {
        let previous = super::super::super::structure_fixture::snapshot();
        let rows = previous
            .sections
            .iter()
            .map(|row| {
                if row.id() == "paragraph" {
                    row.raw()
                        .replace_fields(12, &[m(12, &body(END_ASCII.seed_rtml, true))])
                        .unwrap()
                        .replace_fields(38, &[m(38, &end_original::wire("ascii"))])
                        .unwrap()
                } else {
                    row.raw().clone()
                }
            })
            .collect();
        let source = snapshot_with_zero(&rebuild(&previous, rows), true);
        for reports in [
            &["New", "New draftx", "New draft", "New paragraph"][..],
            &["x", ""][..],
        ] {
            let intent = EndIntent::capture(&source, "paragraph", 1_000_000).unwrap();
            let updates = reports
                .iter()
                .map(|value| TextDraft::from_rtml(value).unwrap())
                .collect::<Vec<_>>();
            let witness = EndWitness::new(intent.seed(), 9, intent.seed(), &TextDraft::default())
                .unwrap()
                .with_child_updates(&updates)
                .unwrap();
            let prepared = intent.prepare(&source, &witness, 1_000_000).unwrap();
            assert_eq!(prepared.child_section(), zero::ID);
            assert!(prepared.reuses_child());
            let checkpoint = serde_json::to_value(prepared.checkpoint().unwrap()).unwrap();
            assert_eq!(checkpoint["version"], 6);
            assert_eq!(checkpoint["action"]["EndTyping"]["child"], zero::ID);
            assert_eq!(
                checkpoint["action"]["EndTyping"]["child_updates"],
                json!(reports)
            );
            let children = prepared.document_data().messages(1).unwrap();
            let child = children
                .iter()
                .find(|row| row.string(1).unwrap() == Some(zero::ID))
                .unwrap();
            let wire = child.message(38).unwrap().unwrap();
            assert_eq!(wire.uint(2).unwrap(), Some(0));
            assert_eq!(wire.uint(3).unwrap(), Some(1000));
            assert!(child.has(12) && wire.has(12));
            assert!(!child.has(8) && !child.has(21));
        }
        assert!(zero::html(false).contains(zero::ID));
        assert!(zero::html(true).contains(zero::ID));
    }
    #[test]
    fn nonliteral_fixtures_clear_then_preserve_ordered_typing_on_both_bases() {
        let previous = super::super::super::structure_fixture::snapshot();
        let rows = previous
            .sections
            .iter()
            .map(|row| {
                if row.id() == "paragraph" {
                    row.raw()
                        .replace_fields(12, &[m(12, &body(END_ASCII.seed_rtml, true))])
                        .unwrap()
                        .replace_fields(38, &[m(38, &end_original::wire("ascii"))])
                        .unwrap()
                } else {
                    row.raw().clone()
                }
            })
            .collect();
        let parent = rebuild(&previous, rows);
        for zero in [false, true] {
            for shape in [
                "spaces",
                "nbsp",
                "br",
                "mixed",
                "mixed-br",
                "separators",
                "feff-leading",
                "feff-interior",
                "feff-post-br",
            ] {
                let source = snapshot_with_content(&parent, zero, shape);
                let original = source.section(id(zero)).unwrap();
                for reports in [
                    &[][..],
                    &["New", "New draftx", "New draft", "New paragraph"][..],
                    &["x", ""][..],
                ] {
                    let intent = EndIntent::capture(&source, "paragraph", 1_000_000).unwrap();
                    assert!(intent.reuses_child() && intent.clears_child());
                    let updates = reports
                        .iter()
                        .map(|text| TextDraft::from_rtml(text).unwrap())
                        .collect::<Vec<_>>();
                    let witness =
                        EndWitness::new(intent.seed(), 9, intent.seed(), &TextDraft::default())
                            .unwrap()
                            .with_child_updates(&updates)
                            .unwrap();
                    let prepared = intent.prepare(&source, &witness, 1_000_000).unwrap();
                    assert!(prepared.clears_child());
                    assert_eq!(prepared.child_section(), id(zero));
                    let checkpoint = serde_json::to_value(prepared.checkpoint().unwrap()).unwrap();
                    assert_eq!(checkpoint["version"], 7);
                    assert_eq!(checkpoint["action"]["EndClearing"]["child"], id(zero));
                    assert_eq!(
                        checkpoint["action"]["EndClearing"]["child_updates"],
                        json!(reports)
                    );
                    let packets = prepared.document_data().messages(1).unwrap();
                    assert_eq!(packets.len(), 2);
                    let child = packets
                        .iter()
                        .find(|row| row.string(1).unwrap() == Some(id(zero)))
                        .unwrap();
                    let wire = child.message(38).unwrap().unwrap();
                    assert_eq!(wire.uint(2).unwrap(), Some(if zero { 0 } else { 1000 }));
                    assert_eq!(wire.uint(3).unwrap(), Some(1000));
                    assert!(wire.has(12) && !wire.has(10));
                    assert!(!child.has(8) && !child.has(21));
                    assert_eq!(
                        child
                            .message(12)
                            .unwrap()
                            .unwrap()
                            .message(1)
                            .unwrap()
                            .unwrap()
                            .string(1)
                            .unwrap(),
                        Some(*reports.last().unwrap_or(&""))
                    );
                    let before = original.raw().message(38).unwrap().unwrap();
                    assert_ne!(wire.bytes(5).unwrap(), before.bytes(5).unwrap());
                    assert_eq!(source.sections.len(), 4);
                }
            }
        }
    }
}
