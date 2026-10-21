//! Named comment-permitted H1-H3 source. No transport or ambient capability.
use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
pub(in crate::canvas) const IDS: [&str; 4] = [
    "heading-one",
    "heading-two",
    "heading-three",
    "heading-twin",
];
pub(in crate::canvas) const RTML: &str =
    "<b>A&amp;</b><u>\u{1f600}B</u> <i>New</i> <u>&amp;\u{4e2d}\u{1f98a}</u> tail";
pub(in crate::canvas) const TEXT: &str = "A&\u{1f600}B New &\u{4e2d}\u{1f98a} tail";
#[derive(Default)]
pub(super) struct Control {
    acks: Vec<String>,
    changes: usize,
}
pub(in crate::canvas) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-comment-headings"
                | "canvas-comment-headings-narrow"
                | "canvas-comment-headings-content-id"
                | "canvas-comment-headings-content-id-narrow")
        )
}
pub(in crate::canvas) fn content_id_requested() -> bool {
    requested()
        && std::env::var("SUPER_PLATINUM_FIXTURE")
            .is_ok_and(|name| name.starts_with("canvas-comment-headings-content-id"))
}
pub(in crate::canvas) fn receipt_prefix() -> &'static str {
    if content_id_requested() {
        "canvas-comment-headings-content-id"
    } else {
        "canvas-comment-headings"
    }
}
pub(in crate::canvas) fn enabled(shell: &ShellState) -> bool {
    requested() && eligible(shell) && path(shell).is_some()
}
pub(super) fn snapshot(s: CanvasCollabSnapshot) -> CanvasCollabSnapshot {
    snapshot_with_attributes(s, content_id_requested())
}
fn snapshot_with_attributes(mut s: CanvasCollabSnapshot, content_id: bool) -> CanvasCollabSnapshot {
    let original = s.section("paragraph").unwrap().clone();
    s.sections.retain(|v| v.id() != "paragraph");
    let neighbor = s
        .sections
        .iter_mut()
        .find(|v| v.id() == "neighbor")
        .unwrap();
    *neighbor = Section::from_raw(
        neighbor
            .raw()
            .replace_fields(8, &[F::bytes(8, b"aaf").unwrap()])
            .unwrap(),
    )
    .unwrap();
    for ((id, level), position) in IDS
        .into_iter()
        .zip([1, 2, 3, 2])
        .zip(["aab", "aac", "aad", "aae"])
    {
        let content = RawMessage::from_fields(&[F::message(
            1,
            &RawMessage::from_fields(&[
                F::bytes(1, RTML.as_bytes()).unwrap(),
                F::bytes(2, b"ja").unwrap(),
            ])
            .unwrap(),
        )
        .unwrap()])
        .unwrap();
        let mut raw = original
            .raw()
            .remove(38)
            .unwrap()
            .replace_fields(1, &[F::bytes(1, id.as_bytes()).unwrap()])
            .unwrap()
            .replace_fields(8, &[F::bytes(8, position.as_bytes()).unwrap()])
            .unwrap()
            .replace_fields(10, &[F::uint(10, level).unwrap()])
            .unwrap()
            .replace_fields(12, &[F::message(12, &content).unwrap()])
            .unwrap()
            .replace_fields(177, &[F::bytes(177, b"heading-comment-opaque").unwrap()])
            .unwrap();
        if content_id {
            raw = raw
                .replace_fields(16, &[F::message(16, &content_attributes(id)).unwrap()])
                .unwrap();
        }
        s.sections.push(Section::from_raw(raw).unwrap());
    }
    s
}
fn content_attributes(id: &str) -> RawMessage {
    RawMessage::from_fields(&[
        F::uint(4, 0).unwrap(),
        F::uint(13, 0).unwrap(),
        F::bytes(29, format!("retained-{id}").as_bytes()).unwrap(),
        F::bytes(177, b"heading-attributes-opaque").unwrap(),
    ])
    .unwrap()
}
pub(super) fn seed(s: CanvasCollabSnapshot) -> Record {
    // Existing control spans two differently styled leaves, without driving a UI write.
    let mut r = existing_seed_at(s, IDS[0], 1, 4);
    r.heading.acks.clear();
    r
}
pub(super) fn ack(r: &mut Record, ack: &EditAck) {
    r.heading.acks.push(STANDARD.encode(ack.raw().raw()));
}
pub(super) fn document(
    s: &CanvasCollabSnapshot,
) -> Option<super_platinum_core::canvas::CanvasDocument> {
    if !requested() {
        return None;
    }
    let mut html =
        String::from("<div class='quip-canvas-content'><h1>Heading comment verification</h1>");
    for (id, level) in IDS.into_iter().zip([1, 2, 3, 2]) {
        let row = s.section(id)?;
        let content = row.raw().message(12).ok()??.message(1).ok()??;
        html.push_str(&format!(
            "<h{level} id='{id}'>{}</h{level}>",
            content.string(1).ok()??
        ));
    }
    html.push_str("<p id='neighbor'>Another paragraph</p></div>");
    Some(super_platinum_core::canvas::CanvasDocument::parse(&html).unwrap())
}
pub(in crate::canvas) fn advance(shell: &ShellState) -> Result<Value, Error> {
    if !enabled(shell) {
        return Err(failure());
    }
    let cap = capability(shell)?;
    let mut r = cap.record.lock().map_err(|_| failure())?;
    let row = r
        .canonical
        .as_mut()
        .ok_or_else(failure)?
        .sections
        .iter_mut()
        .find(|v| v.id() == IDS[1])
        .ok_or_else(failure)?;
    let before = STANDARD.encode(row.raw().raw());
    let seq = row.sequence().map_err(|_| failure())?.ok_or_else(failure)? + 1;
    *row = Section::from_raw(
        row.raw()
            .replace_fields(2, &[F::uint(2, seq).unwrap()])
            .map_err(|_| failure())?,
    )
    .map_err(|_| failure())?;
    let after = STANDARD.encode(row.raw().raw());
    r.heading.changes += 1;
    Ok(json!({"id":IDS[1],"before":before,"after":after,"sequence":seq}))
}
pub(in crate::canvas) fn proof(shell: &ShellState) -> Value {
    if !enabled(shell) {
        return Value::Null;
    }
    let Ok(cap) = capability(shell) else {
        return Value::Null;
    };
    let Ok(r) = cap.record.lock() else {
        return Value::Null;
    };
    let Some(s) = &r.canonical else {
        return Value::Null;
    };
    json!({"seedRequests":r.seed_requests,"requests":r.requests,"prepared":r.prepared,"roots":r.roots,"replies":r.replies,"acks":r.heading.acks,"sourceChanges":r.heading.changes,
        "document":STANDARD.encode(s.document.raw()),"thread":STANDARD.encode(s.thread.raw()),"file":s.file,"sections":s.sections.iter().map(|v|json!({"id":v.id(),"raw":STANDARD.encode(v.raw().raw())})).collect::<Vec<_>>()})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn content_id_fixture_keeps_complete_attributes_through_existing_and_new_controls() {
        let input = snapshot_with_attributes(initial(), true);
        for id in IDS {
            assert_eq!(
                input.section(id).unwrap().attributes().unwrap(),
                Some(content_attributes(id))
            );
        }
        let mut record = seed(input);
        let twin = record
            .canonical
            .as_ref()
            .unwrap()
            .section(IDS[3])
            .unwrap()
            .clone();
        for id in &IDS[..3] {
            let before = record.canonical.clone().unwrap();
            let attributes = before.section(id).unwrap().attributes().unwrap().unwrap();
            assert_eq!(attributes, content_attributes(id));
            let prepared = AnnotationIntent::capture_range(&before, id, 6, 14)
                .unwrap()
                .prepare_at(&before, crate::canvas::checklist::save::now_usec().unwrap())
                .unwrap();
            let updates = prepared.document_data().messages(1).unwrap();
            let parent = updates
                .iter()
                .find(|row| row.string(1).unwrap() == Some(*id))
                .unwrap();
            assert_eq!(parent.message(16).unwrap(), Some(attributes.clone()));
            for tag in [3, 8, 10, 11, 29] {
                assert!(!parent.has(tag));
            }
            let accepted = complete_anchor(
                &mut record,
                &before,
                prepared,
                json!({"closed":"content-id test"}),
            )
            .unwrap();
            accepted
                .confirm(record.canonical.as_ref().unwrap())
                .unwrap();
            assert_eq!(
                record
                    .canonical
                    .as_ref()
                    .unwrap()
                    .section(id)
                    .unwrap()
                    .attributes()
                    .unwrap(),
                Some(attributes)
            );
        }
        assert_eq!(
            record.canonical.as_ref().unwrap().section(IDS[3]),
            Some(&twin)
        );
    }
    #[test]
    fn heading_fixture_has_actual_comment_grant_and_cross_style_existing_control() {
        let s = snapshot(initial());
        assert_eq!(s.file.extra.get("access"), Some(&json!("comment")));
        assert_eq!(s.thread.uint(23).unwrap(), Some(5));
        let r = seed(s);
        assert_eq!(r.seed_requests.len(), 2);
        assert!(r.requests.is_empty() && r.prepared.is_empty());
        assert_eq!(r.roots.len(), 1);
        let s = r.canonical.as_ref().unwrap();
        let control = r.roots[0]["document_comment"]["thread_id"]
            .as_str()
            .unwrap();
        let source = s
            .section(IDS[0])
            .unwrap()
            .raw()
            .message(12)
            .unwrap()
            .unwrap()
            .message(1)
            .unwrap()
            .unwrap();
        assert!(source.string(1).unwrap().unwrap().contains(control));
        assert_eq!(TEXT.encode_utf16().count(), 19);
    }
    #[test]
    fn actual_heading_anchor_roundtrips_preserve_twin_siblings_and_unknown_bytes() {
        let mut r = seed(snapshot(initial()));
        let original = r.canonical.clone().unwrap();
        for (id, level) in IDS[..3].iter().zip([1, 2, 3]) {
            let before = r.canonical.clone().unwrap();
            let prepared = AnnotationIntent::capture_range(&before, id, 6, 14)
                .unwrap()
                .prepare_at(&before, crate::canvas::checklist::save::now_usec().unwrap())
                .unwrap();
            let accepted = complete_anchor(
                &mut r,
                &before,
                prepared,
                json!({"closed":"original prepared proof"}),
            )
            .unwrap();
            accepted.confirm(r.canonical.as_ref().unwrap()).unwrap();
            let after = r.canonical.as_ref().unwrap();
            for row in &before.sections {
                if row.id() != *id {
                    assert_eq!(
                        after.section(row.id()).unwrap().raw().raw(),
                        row.raw().raw()
                    );
                }
            }
            let heading = after.section(id).unwrap();
            assert_eq!(heading.raw().uint(10).unwrap(), Some(level));
            assert_eq!(
                heading.raw().string(177).unwrap(),
                Some("heading-comment-opaque")
            );
        }
        assert_eq!(
            r.canonical.as_ref().unwrap().section(IDS[3]).unwrap(),
            original.section(IDS[3]).unwrap()
        );
        assert_eq!(r.roots.len(), 1); // Anchor alone does not invent bootstrap/root acceptance.
    }
}
