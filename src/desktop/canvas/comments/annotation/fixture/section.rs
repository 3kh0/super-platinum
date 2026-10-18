//! Disconnected section-control fixture and exact normal request completion.
use super::*;

pub(super) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-comment-section" | "canvas-comment-section-narrow")
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
fn message(tag: u32, value: &RawMessage) -> F {
    F::message(tag, value).unwrap()
}
pub(super) fn seed(mut snapshot: CanvasCollabSnapshot) -> Record {
    let mut roots = Vec::new();
    let mut metadata = Vec::new();
    for (index, id, count, archived, positioned) in [
        (0, "old-section-stub", 0, false, false),
        (1, "old-section-archived", 3, true, false),
        (2, "old-section-positioned", 2, false, true),
    ] {
        let mut highlight = vec![
            text(1, "original-creator"),
            text(3, "paragraph"),
            text(3, "not-loaded"),
        ];
        if positioned {
            highlight.push(message(
                11,
                &RawMessage::decode(&[9, 0, 0, 0, 0, 0, 0, 0, 0, 17, 0, 0, 0, 0, 0, 0, 0, 0])
                    .unwrap(),
            ));
        }
        snapshot.sections.push(
            Section::from_raw(raw(&[
                text(1, id),
                num(2, 37),
                text(6, snapshot.thread_id()),
                text(7, snapshot.document_id()),
                num(9, 9),
                num(11, 1),
                message(12, &raw(&[message(7, &raw(&highlight))])),
                text(21, "original-path"),
            ]))
            .unwrap(),
        );
        metadata.push(message(
            1,
            &raw(&[text(1, id), num(2, count), num(8, u64::from(archived))]),
        ));
        roots.push(json!({"type":"message","subtype":"document_comment_root","channel":super_platinum_core::canvas::comments::channel(snapshot.file_id()).unwrap(),
            "ts":format!("1700000000.{:06}",index+1),"text":"Original section comment","user":"U1",
            "reply_count":count,"document_comment":{"thread_id":id}}));
    }
    snapshot.document = snapshot
        .document
        .replace_fields(16, &[message(16, &raw(&metadata))])
        .unwrap();
    Record {
        canonical: Some(snapshot),
        roots,
        ..Default::default()
    }
}

pub(super) fn responses(
    p: &PreparedAnnotation,
    current: &CanvasCollabSnapshot,
) -> Result<Vec<Section>, CanvasEditFailure> {
    let failed = || CanvasEditFailure::NotSent(failure());
    let updates = p.document_data().messages(1).map_err(|_| failed())?;
    if updates.is_empty() || updates.len() > 4098 {
        return Err(failed());
    }
    let mut sections = Vec::new();
    let mut ids = std::collections::HashSet::new();
    for update in updates {
        let update = Section::from_raw(update).map_err(|_| failed())?;
        if !ids.insert(update.id().to_owned()) {
            return Err(failed());
        }
        let original = current.section(update.id());
        let mut section = match original {
            Some(original) => original.merge_partial(&update).map_err(|_| failed())?,
            None if update.id() == p.control_id() => update.clone(),
            None => return Err(failed()),
        };
        let sequence = original
            .and_then(|s| s.sequence().ok().flatten())
            .unwrap_or(9001)
            + 1000;
        let mut changed = section.raw().clone();
        for (tag, field) in [
            (2, num(2, sequence)),
            (6, text(6, p.thread_id())),
            (7, text(7, p.document_id())),
        ] {
            changed = changed
                .replace_fields(tag, &[field])
                .map_err(|_| failed())?;
        }
        if update.section_type().map_err(|_| failed())? == Some(9) {
            let path = changed
                .string(21)
                .map_err(|_| failed())?
                .ok_or_else(failed)?
                .to_owned();
            changed = changed
                .replace_fields(21, &[text(21, path.strip_suffix(":temp").unwrap_or(&path))])
                .map_err(|_| failed())?;
        } else if update.id() == p.anchor_section() {
            let expiry = crate::canvas::checklist::save::now_usec()
                .and_then(|v| v.checked_add(60_000_000))
                .ok_or_else(failed)?;
            let history = changed
                .message(38)
                .map_err(|_| failed())?
                .ok_or_else(failed)?
                .remove(12)
                .map_err(|_| failed())?
                .replace_fields(3, &[num(3, sequence)])
                .map_err(|_| failed())?
                .replace_fields(4, &[num(4, expiry)])
                .map_err(|_| failed())?;
            changed = changed
                .replace_fields(38, &[message(38, &history)])
                .map_err(|_| failed())?;
        } else {
            return Err(failed());
        }
        section = Section::from_raw(changed).map_err(|_| failed())?;
        sections.push(section);
    }
    if !ids.contains(p.control_id()) {
        return Err(failed());
    }
    Ok(sections)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super_platinum_core::canvas::edit::annotation::{
        AnnotationPresentation, SectionAnnotationChoice,
    };
    use super_platinum_core::slack::client::SlackClient;
    #[test]
    fn actual_multi_control_builder_ack_and_bootstrap_retain_old_roots_and_visible_control() {
        let mut record = seed(initial());
        let snapshot = record.canonical.clone().unwrap();
        let roots = record.roots.clone();
        let visible = snapshot.section("old-section-positioned").unwrap().clone();
        let SectionAnnotationChoice::New(intent) = AnnotationIntent::for_section(
            &snapshot,
            "paragraph",
            &AnnotationPresentation::ordinary(None, None).unwrap(),
        )
        .unwrap() else {
            panic!()
        };
        let p = intent.prepare(&snapshot).unwrap();
        let session = WorkspaceSession {
            team_id: "T1".into(),
            enterprise_id: None,
            user_id: "U1".into(),
            name: "Fixture".into(),
            url: "https://fixture.slack.com".into(),
            token: "fixture-only".into(),
        };
        let request =
            wire(api::canvas_annotation(&SlackClient::default(), &session, &snapshot, &p).unwrap())
                .unwrap();
        let accepted = complete_anchor(&mut record, &snapshot, p, request).unwrap();
        assert_eq!(record.requests.len(), 1);
        assert_eq!(record.roots, roots);
        assert_eq!(
            record.canonical.as_ref().unwrap().section(visible.id()),
            Some(&visible)
        );
        let proof = accepted
            .confirm(record.canonical.as_ref().unwrap())
            .unwrap();
        let intent = BootstrapIntent::capture(proof, 1_700_000_000_001).unwrap();
        assert_eq!(
            serde_json::to_value(intent.record()).unwrap()["canonical_changed_controls"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        // The unchanged existing fixture bootstrap validates the actual second
        // request and consumes the ordinary root proof, never a synthetic permit.
        let request = wire(
            api::canvas_comment_bootstrap(&SlackClient::default(), &session, &intent).unwrap(),
        )
        .unwrap();
        complete_bootstrap(&mut record, intent, request).unwrap();
        assert_eq!(record.requests.len(), 2);
        assert_eq!(&record.roots[..3], roots.as_slice());
    }

    #[test]
    fn multi_control_fixture_distinguishes_unsent_from_lost_reply_without_a_replayed_write() {
        for mode in [1, 2] {
            let mut record = seed(initial());
            record.mode = mode;
            let snapshot = record.canonical.clone().unwrap();
            let SectionAnnotationChoice::New(intent) = AnnotationIntent::for_section(
                &snapshot,
                "paragraph",
                &AnnotationPresentation::ordinary(None, None).unwrap(),
            )
            .unwrap() else {
                panic!()
            };
            let p = intent.prepare(&snapshot).unwrap();
            let new_id = p.control_id().to_owned();
            let session = WorkspaceSession {
                team_id: "T1".into(),
                enterprise_id: None,
                user_id: "U1".into(),
                name: "Fixture".into(),
                url: "https://fixture.slack.com".into(),
                token: "fixture-only".into(),
            };
            let request = wire(
                api::canvas_annotation(&SlackClient::default(), &session, &snapshot, &p).unwrap(),
            )
            .unwrap();
            let result = complete_anchor(&mut record, &snapshot, p, request);
            if mode == 1 {
                assert!(matches!(result, Err(CanvasEditFailure::NotSent(_))));
                assert_eq!(
                    record.canonical.as_ref().unwrap().sections,
                    snapshot.sections
                );
                assert!(record.requests.is_empty());
            } else {
                assert!(matches!(result, Err(CanvasEditFailure::Uncertain(_))));
                assert_eq!(record.requests.len(), 1);
                let now = record.canonical.as_ref().unwrap();
                assert!(now.section(&new_id).is_some());
                for id in ["old-section-stub", "old-section-archived"] {
                    let h = now
                        .section(id)
                        .unwrap()
                        .raw()
                        .message(12)
                        .unwrap()
                        .unwrap()
                        .message(7)
                        .unwrap()
                        .unwrap();
                    assert!(!h.has(3));
                }
                assert_eq!(now.document, snapshot.document);
            }
            assert_eq!(record.roots.len(), 3);
            assert_eq!(record.prepared.len(), 1);
        }
    }
}
