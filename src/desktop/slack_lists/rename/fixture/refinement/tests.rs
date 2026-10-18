use super::*;
use super_platinum_core::slack_lists::{
    BoardLaneOrderDraft, BoardLanePlacement, ListDefaultViewPreference,
    ListDefaultViewPreferenceAttempt, ViewRefinementDraft, ViewRefinementSave,
    ViewRefinementSaveAttempt,
};
#[test]
fn actual_builder_matches_independent_literal_save_and_child_forms_without_changing_raw_population()
{
    for (alias, mode) in [
        ("slack-list-board-refinement-save", Case::Existing),
        ("slack-list-board-refinement-save-new", Case::New),
        ("slack-list-board-refinement-save-lost", Case::Lost),
    ] {
        let s = ShellState::fixture_variant(crate::media::MediaRegistry::default(), alias);
        let l = s
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap();
        let original = l.document.clone();
        let population = l.records.clone();
        let provider = BoardLaneOrderDraft::moved(
            &original,
            "V_LANE",
            None,
            "OPT_DONE",
            "OPT_PROGRESS",
            BoardLanePlacement::Before,
            l.allow_lists.as_deref(),
            l.personal_task_lists.as_ref(),
        )
        .unwrap();
        let draft = ViewRefinementDraft::capture(&original, "V_LANE")
            .unwrap()
            .with_visibility(&original, "C_NUMBER", false)
            .unwrap();
        let destination = if mode.created() {
            ViewRefinementDestination::New {
                name: "Focused delivery".into(),
            }
        } else {
            ViewRefinementDestination::Existing
        };
        let intent = ViewRefinementSave::capture(
            &original,
            &draft,
            Some(&provider),
            destination,
            l.allow_lists.as_deref(),
            l.personal_task_lists.as_ref(),
        )
        .unwrap();
        let mut attempt = ViewRefinementSaveAttempt::new(intent.clone());
        let permit = attempt
            .admit(
                &original,
                l.allow_lists.as_deref(),
                l.personal_task_lists.as_ref(),
            )
            .unwrap();
        let mut request = super_platinum_core::slack::api::list_view_refinement_save(
            &s.core.client,
            &session(),
            permit,
        );
        forms::metadata(&request, mode).unwrap();
        let RequestBody::Form(fields) = &mut request.body else {
            panic!()
        };
        fields.push(("unexpected".into(), "true".into()));
        assert!(forms::metadata(&request, mode).is_err());
        let expected = Document::parse(forms::saved(&original.raw, mode).unwrap()).unwrap();
        if mode.created() {
            assert_eq!(
                intent.observed_view_id(&expected).as_deref(),
                Some(mode.view_id())
            );
        } else {
            assert_eq!(
                intent.observed_view_id(&expected).as_deref(),
                Some(mode.view_id())
            );
        }
        assert_eq!(original, l.document);
        assert_eq!(population, l.records);
        assert_eq!(population.records.len(), 73);
        if mode == Case::Lost {
            assert!(attempt.finish(false));
            assert!(
                attempt
                    .acknowledge(&json!({"ok":true,"list":expected.raw}))
                    .is_none()
            );
        } else {
            let ack = attempt
                .acknowledge(&json!({"ok":true,"list":expected.raw}))
                .unwrap();
            let child = ListDefaultViewPreference::capture_acknowledged(
                &forms::original_preferences(),
                ack,
            )
            .unwrap();
            let mut child = ListDefaultViewPreferenceAttempt::new(child);
            let permit = child
                .admit(&forms::original_preferences(), &original)
                .unwrap();
            let request = super_platinum_core::slack::api::list_default_view_preference(
                &s.core.client,
                &session(),
                permit,
            );
            forms::preference_write(&request, mode).unwrap();
            assert_eq!(child.intent().expected()["F_OTHER"], "V_OTHER");
            assert_eq!(child.intent().expected()["opaque"], "retained");
        }
    }
}
