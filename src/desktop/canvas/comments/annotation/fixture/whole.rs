//! A real existing inline seed plus the separately named section-control seed.
//! Setup requests are retained separately from the new user-driven operation.
use super::*;
pub(super) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-comment-section-inline" | "canvas-comment-section-inline-narrow")
        )
}
pub(super) fn seed(snapshot: CanvasCollabSnapshot) -> Record {
    let inline = hidden_seed(inline_snapshot(snapshot));
    let canonical = inline.canonical.as_ref().unwrap();
    let old = canonical
        .document
        .message(16)
        .unwrap()
        .unwrap()
        .messages(1)
        .unwrap();
    let mut combined = section::seed(canonical.clone());
    let canonical = combined.canonical.as_mut().unwrap();
    let mut fields = old
        .iter()
        .map(|x| F::message(1, x).unwrap())
        .collect::<Vec<_>>();
    fields.extend(
        canonical
            .document
            .message(16)
            .unwrap()
            .unwrap()
            .messages(1)
            .unwrap()
            .iter()
            .map(|x| F::message(1, x).unwrap()),
    );
    canonical.document = canonical
        .document
        .replace_fields(
            16,
            &[F::message(16, &RawMessage::from_fields(&fields).unwrap()).unwrap()],
        )
        .unwrap();
    combined.roots.splice(0..0, inline.roots);
    combined.replies = inline.replies;
    combined.seed_requests = inline.seed_requests;
    combined
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
    fn prepared(s: &CanvasCollabSnapshot) -> PreparedAnnotation {
        let SectionAnnotationChoice::New(intent) = AnnotationIntent::for_section(
            s,
            "paragraph",
            &AnnotationPresentation::ordinary(None, None).unwrap(),
        )
        .unwrap() else {
            panic!("new")
        };
        intent
            .prepare_at(s, crate::canvas::checklist::save::now_usec().unwrap())
            .unwrap()
    }
    #[test]
    fn actual_section_inline_fixture_runs_parent_old_controls_and_qpc_without_losing_old_roots() {
        let mut record = seed(initial());
        let snapshot = record.canonical.clone().unwrap();
        let roots = record.roots.clone();
        assert_eq!(roots.len(), 4);
        assert_eq!(record.seed_requests.len(), 2);
        assert!(record.requests.is_empty());
        let p = prepared(&snapshot);
        assert_eq!(p.document_data().messages(1).unwrap().len(), 4);
        let request = wire(
            api::canvas_annotation(&SlackClient::default(), &workspace(), &snapshot, &p).unwrap(),
        )
        .unwrap();
        let accepted = complete_anchor(&mut record, &snapshot, p, request).unwrap();
        assert_eq!(record.roots, roots);
        assert_eq!(record.requests.len(), 1);
        let proof = accepted
            .confirm(record.canonical.as_ref().unwrap())
            .unwrap();
        let intent = BootstrapIntent::capture(proof, 1_720_000_000_003).unwrap();
        let evidence = serde_json::to_value(intent.record()).unwrap();
        assert!(evidence["canonical_anchor"].is_array());
        assert_eq!(
            evidence["canonical_changed_controls"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        let request = wire(
            api::canvas_comment_bootstrap(&SlackClient::default(), &workspace(), &intent).unwrap(),
        )
        .unwrap();
        complete_bootstrap(&mut record, intent, request).unwrap();
        assert_eq!(record.requests.len(), 2);
        assert_eq!(&record.roots[..4], roots.as_slice());
    }
    #[test]
    fn section_inline_fixture_lost_ack_retains_changed_body_and_controls_without_retry() {
        for mode in [1, 2] {
            let mut record = seed(initial());
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
                assert_eq!(record.requests.len(), 0);
                assert_eq!(
                    record.canonical.as_ref().unwrap().sections,
                    snapshot.sections
                );
            } else {
                assert!(matches!(result, Err(CanvasEditFailure::Uncertain(_))));
                assert_eq!(record.requests.len(), 1);
                let current = record.canonical.as_ref().unwrap();
                assert!(current.section(&id).is_some());
                assert_ne!(current.section("paragraph"), snapshot.section("paragraph"));
                assert_eq!(current.document, snapshot.document);
            }
            assert_eq!(record.roots.len(), 4);
        }
    }
}
