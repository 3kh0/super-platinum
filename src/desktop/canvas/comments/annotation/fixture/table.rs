//! Named modern-table child annotation through the existing disconnected worker.
use super::*;

pub(super) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-comment-table" | "canvas-comment-table-narrow")
        )
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
fn msg(tag: u32, value: &RawMessage) -> F {
    F::message(tag, value).unwrap()
}
fn field(raw: &RawMessage, f: F) -> RawMessage {
    raw.merge_partial(&RawMessage::decode(f.raw()).unwrap())
        .unwrap()
}

pub(super) fn seed(mut snapshot: CanvasCollabSnapshot) -> Record {
    let at = snapshot
        .sections
        .iter()
        .position(|s| s.id() == "paragraph")
        .unwrap();
    let original = snapshot.sections[at].clone();
    snapshot.sections[at] = Section::from_raw(field(original.raw(), num(29, 1))).unwrap();
    for (id, order, body, deleted) in [
        (
            "table-second",
            "zza",
            "Second cell with <b>bold text</b>",
            false,
        ),
        ("table-deleted", "zzb", "Deleted original child", true),
    ] {
        let mut child = original.raw().remove(38).unwrap();
        for f in [
            text(1, id),
            text(8, order),
            text(21, order),
            num(29, 1),
            num(3, u64::from(deleted)),
            msg(12, &raw(&[msg(1, &raw(&[text(1, body)]))])),
        ] {
            child = field(&child, f);
        }
        snapshot.sections.push(Section::from_raw(child).unwrap());
    }
    let layout = raw(&[
        msg(1, &raw(&[text(1, "table-row"), text(2, "aaa")])),
        msg(2, &raw(&[text(1, "table-col1"), text(2, "aaa")])),
        msg(2, &raw(&[text(1, "table-col2"), text(2, "aab")])),
        msg(
            3,
            &raw(&[
                text(1, "table-cell1"),
                text(2, "table-row"),
                text(3, "table-col1"),
                text(4, "paragraph"),
            ]),
        ),
        msg(
            3,
            &raw(&[
                text(1, "table-cell2"),
                text(2, "table-row"),
                text(3, "table-col2"),
                text(4, "table-second"),
                text(4, "table-deleted"),
            ]),
        ),
        text(99, "retained layout metadata"),
    ]);
    snapshot.sections.push(
        Section::from_raw(raw(&[
            text(1, "modern-table"),
            num(2, 71),
            text(6, snapshot.thread_id()),
            text(7, snapshot.document_id()),
            text(8, "zyz"),
            text(21, "zyz"),
            num(9, 33),
            num(10, 28),
            msg(12, &raw(&[msg(30, &layout)])),
        ]))
        .unwrap(),
    );
    section::seed(snapshot)
}

pub(super) fn document(
    snapshot: &CanvasCollabSnapshot,
) -> Option<super_platinum_core::canvas::CanvasDocument> {
    snapshot.section("modern-table")?;
    let mut html = "<div class='quip-canvas-content'><h1>Table comments</h1><table id='modern-table'><tbody><tr>".to_owned();
    for id in ["paragraph", "table-second"] {
        let child = snapshot.section(id)?;
        let content = child.raw().message(12).ok()??.message(1).ok()??;
        // Fixed synthetic IDs and original fixture RTML, parsed to typed nodes.
        html.push_str(&format!(
            "<td><p id='{id}'>{}</p></td>",
            content.string(1).ok()??
        ));
    }
    html.push_str("</tr></tbody></table>");
    for other in &snapshot.sections {
        if other.deleted().ok() == Some(Some(true))
            || other.raw().boolean(29).ok() == Some(Some(true))
            || other.section_type().ok().flatten().unwrap_or(0) != 0
        {
            continue;
        }
        let content = other.raw().message(12).ok()??.message(1).ok()??;
        html.push_str(&format!(
            "<p id='{}'>{}</p>",
            other.id(),
            content.string(1).ok()??
        ));
    }
    html.push_str("</div>");
    Some(super_platinum_core::canvas::CanvasDocument::parse(&html).unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use super_platinum_core::canvas::edit::annotation::{
        AnnotationPresentation, SectionAnnotationChoice, section_previews,
    };
    use super_platinum_core::slack::client::SlackClient;
    fn session() -> WorkspaceSession {
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
            panic!("new")
        };
        intent.prepare(snapshot).unwrap()
    }
    #[test]
    fn actual_table_fixture_uses_real_anchor_and_separate_bootstrap_without_rewriting_layout() {
        let mut record = seed(initial());
        let s = record.canonical.clone().unwrap();
        let roots = record.roots.clone();
        assert!(document(&s).is_some());
        let previews =
            section_previews(&s, &AnnotationPresentation::ordinary(None, None).unwrap()).unwrap();
        assert!(previews.iter().any(|p| p.id == "paragraph"));
        assert!(previews.iter().any(|p| p.id == "table-second"));
        assert!(
            !previews
                .iter()
                .any(|p| matches!(p.id.as_str(), "modern-table" | "table-deleted"))
        );
        let p = prepared(&s);
        assert_eq!(
            serde_json::to_value(p.record().unwrap()).unwrap()["version"],
            10
        );
        let request =
            wire(api::canvas_annotation(&SlackClient::default(), &session(), &s, &p).unwrap())
                .unwrap();
        let accepted = complete_anchor(&mut record, &s, p, request).unwrap();
        let now = record.canonical.as_ref().unwrap();
        for id in ["modern-table", "paragraph", "table-second", "table-deleted"] {
            assert_eq!(s.section(id), now.section(id));
        }
        let intent =
            BootstrapIntent::capture(accepted.confirm(now).unwrap(), 1_720_000_000_001).unwrap();
        let request = wire(
            api::canvas_comment_bootstrap(&SlackClient::default(), &session(), &intent).unwrap(),
        )
        .unwrap();
        complete_bootstrap(&mut record, intent, request).unwrap();
        assert_eq!(record.requests.len(), 2);
        assert_eq!(&record.roots[..3], roots.as_slice());
    }
    #[test]
    fn actual_table_fixture_keeps_unsent_and_lost_ack_distinct_with_unchanged_child_custody() {
        for mode in [1, 2] {
            let mut record = seed(initial());
            record.mode = mode;
            let s = record.canonical.clone().unwrap();
            let p = prepared(&s);
            let id = p.control_id().to_owned();
            let request =
                wire(api::canvas_annotation(&SlackClient::default(), &session(), &s, &p).unwrap())
                    .unwrap();
            let result = complete_anchor(&mut record, &s, p, request);
            if mode == 1 {
                assert!(matches!(result, Err(CanvasEditFailure::NotSent(_))));
                assert!(record.requests.is_empty());
                assert!(record.canonical.as_ref().unwrap().section(&id).is_none());
            } else {
                assert!(matches!(result, Err(CanvasEditFailure::Uncertain(_))));
                assert_eq!(record.requests.len(), 1);
                assert!(record.canonical.as_ref().unwrap().section(&id).is_some());
            }
            for id in ["modern-table", "paragraph", "table-second", "table-deleted"] {
                assert_eq!(
                    s.section(id),
                    record.canonical.as_ref().unwrap().section(id)
                );
            }
            assert_eq!(record.roots.len(), 3);
        }
    }
}
