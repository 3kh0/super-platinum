//! Named disconnected ordinary list annotation. Uses the existing real request,
//! response, separate bootstrap and lost-result fixture paths without Transport.
use super::*;
pub(super) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-comment-list" | "canvas-comment-list-narrow")
        )
}
pub(super) fn document(
    snapshot: &CanvasCollabSnapshot,
) -> Option<super_platinum_core::canvas::CanvasDocument> {
    let list = snapshot.section("paragraph")?;
    if list.section_type().ok().flatten() != Some(1) || snapshot.section("list-first").is_none() {
        return None;
    }
    let style = list.style().unwrap().unwrap();
    let tag = if style == 6 { "ol" } else { "ul" };
    let mut html =
        "<div class='quip-canvas-content'><h1>List annotation verification</h1>".to_owned();
    if style == 7 {
        html.push_str("<div data-section-style='7'>");
    }
    html.push_str(&format!("<{tag} id='paragraph'>"));
    let mut depth = 0;
    for id in ["list-first", "list-child", "list-last"] {
        let child = snapshot.section(id).unwrap();
        let attrs = child.raw().message(16).unwrap().unwrap();
        let indent = attrs.uint(1).unwrap().unwrap() as usize;
        while depth < indent {
            html.push_str(&format!("<{tag}>"));
            depth += 1;
        }
        while depth > indent {
            html.push_str(&format!("</{tag}>"));
            depth -= 1;
        }
        let class = if attrs.boolean(2).unwrap() == Some(true) {
            " class='checked'"
        } else {
            ""
        };
        let text = child
            .raw()
            .message(12)
            .unwrap()
            .unwrap()
            .message(1)
            .unwrap()
            .unwrap();
        // Exact synthetic fixture IDs/canonical RTML only, parsed to typed nodes.
        html.push_str(&format!(
            "<li id='{id}'{class}>{}</li>",
            text.string(1).unwrap().unwrap()
        ));
    }
    for _ in 0..=depth {
        html.push_str(&format!("</{tag}>"));
    }
    if style == 7 {
        html.push_str("</div>");
    }
    for other in &snapshot.sections {
        if other.id() == "paragraph"
            || other.deleted().unwrap() == Some(true)
            || other.section_type().unwrap().unwrap_or(0) != 0
            || other
                .parents()
                .unwrap()
                .is_some_and(|p| !p.fields().is_empty())
        {
            continue;
        }
        let text = other
            .raw()
            .message(12)
            .unwrap()
            .unwrap()
            .message(1)
            .unwrap()
            .unwrap();
        html.push_str(&format!(
            "<p id='{}'>{}</p>",
            other.id(),
            text.string(1).unwrap().unwrap()
        ));
    }
    html.push_str("</div>");
    Some(super_platinum_core::canvas::CanvasDocument::parse(&html).unwrap())
}
fn field(raw: &RawMessage, f: F) -> RawMessage {
    raw.merge_partial(&RawMessage::decode(f.raw()).unwrap())
        .unwrap()
}
fn raw(fields: &[F]) -> RawMessage {
    RawMessage::from_fields(fields).unwrap()
}
fn text(tag: u32, value: &str) -> F {
    F::bytes(tag, value.as_bytes()).unwrap()
}
fn num(tag: u32, value: u64) -> F {
    F::uint(tag, value).unwrap()
}
fn message(tag: u32, value: &RawMessage) -> F {
    F::message(tag, value).unwrap()
}
pub(super) fn seed(mut snapshot: CanvasCollabSnapshot, style: u64) -> Record {
    let i = snapshot
        .sections
        .iter()
        .position(|s| s.id() == "paragraph")
        .unwrap();
    let original = snapshot.sections[i].clone();
    let parent_position = original.raw().string(8).unwrap().unwrap();
    let mut list = original.raw().remove(38).unwrap();
    for f in [num(9, 1), num(10, style), message(12, &raw(&[]))] {
        list = field(&list, f);
    }
    snapshot.sections[i] = Section::from_raw(list).unwrap();
    for (id, position, html, indent, checked) in [
        (
            "list-first",
            "zaa",
            "First item with <b>bold text</b>",
            0,
            0,
        ),
        ("list-child", "zab", "Nested completed item", 1, 1),
        ("list-last", "zac", "Last item with a second<br>line", 0, 0),
    ] {
        let mut child = original.raw().remove(38).unwrap();
        for f in [
            text(1, id),
            text(8, position),
            text(21, &format!("{parent_position}-{position}")),
            num(10, 0),
            message(13, &raw(&[text(1, "paragraph"), num(4, style)])),
            message(12, &raw(&[message(1, &raw(&[text(1, html)]))])),
            message(16, &raw(&[num(1, indent), num(2, checked)])),
        ] {
            child = field(&child, f);
        }
        snapshot.sections.push(Section::from_raw(child).unwrap());
    }
    section::seed(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super_platinum_core::{
        canvas::edit::annotation::{AnnotationPresentation, SectionAnnotationChoice},
        slack::client::SlackClient,
    };
    fn workspace() -> WorkspaceSession {
        WorkspaceSession {
            team_id: "T1".into(),
            enterprise_id: None,
            user_id: "U1".into(),
            name: "Fixture".into(),
            url: "https://fixture.slack.com".into(),
            token: "fixture-only".into(),
        }
    }
    fn prepared(snapshot: &CanvasCollabSnapshot) -> PreparedAnnotation {
        let SectionAnnotationChoice::New(intent) = AnnotationIntent::for_section(
            snapshot,
            "paragraph",
            &AnnotationPresentation::ordinary(None, None).unwrap(),
        )
        .unwrap() else {
            panic!()
        };
        intent.prepare(snapshot).unwrap()
    }
    #[test]
    fn actual_list_fixtures_preserve_children_and_old_roots_through_edit_and_separate_qpc() {
        for style in 5..=7 {
            let mut record = seed(initial(), style);
            let snapshot = record.canonical.clone().unwrap();
            assert!(document(&snapshot).is_some());
            let roots = record.roots.clone();
            let p = prepared(&snapshot);
            assert_eq!(p.document_data().messages(1).unwrap().len(), 3);
            let request = wire(
                api::canvas_annotation(&SlackClient::default(), &workspace(), &snapshot, &p)
                    .unwrap(),
            )
            .unwrap();
            let accepted = complete_anchor(&mut record, &snapshot, p, request).unwrap();
            let canonical = record.canonical.as_ref().unwrap();
            for id in ["paragraph", "list-first", "list-child", "list-last"] {
                assert_eq!(snapshot.section(id), canonical.section(id));
            }
            let proof = accepted.confirm(canonical).unwrap();
            let intent = BootstrapIntent::capture(proof, 1_720_000_000_003).unwrap();
            let request = wire(
                api::canvas_comment_bootstrap(&SlackClient::default(), &workspace(), &intent)
                    .unwrap(),
            )
            .unwrap();
            complete_bootstrap(&mut record, intent, request).unwrap();
            assert_eq!(record.requests.len(), 2);
            assert_eq!(&record.roots[..3], roots.as_slice());
        }
    }
    #[test]
    fn actual_list_fixture_distinguishes_not_sent_from_lost_ack_without_child_rewrite_or_replay() {
        for mode in [1, 2] {
            let mut record = seed(initial(), 7);
            record.mode = mode;
            let snapshot = record.canonical.clone().unwrap();
            let p = prepared(&snapshot);
            let id = p.control_id().to_owned();
            let request = wire(
                api::canvas_annotation(&SlackClient::default(), &workspace(), &snapshot, &p)
                    .unwrap(),
            )
            .unwrap();
            let result = complete_anchor(&mut record, &snapshot, p, request);
            if mode == 1 {
                assert!(matches!(result, Err(CanvasEditFailure::NotSent(_))));
                assert!(record.requests.is_empty());
                assert!(record.canonical.as_ref().unwrap().section(&id).is_none());
            } else {
                assert!(matches!(result, Err(CanvasEditFailure::Uncertain(_))));
                assert_eq!(record.requests.len(), 1);
                assert!(record.canonical.as_ref().unwrap().section(&id).is_some());
            }
            for id in ["paragraph", "list-first", "list-child", "list-last"] {
                assert_eq!(
                    snapshot.section(id),
                    record.canonical.as_ref().unwrap().section(id)
                );
            }
            assert_eq!(record.roots.len(), 3);
        }
    }
}
