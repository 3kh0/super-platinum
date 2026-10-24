use super::*;
use crate::canvas::text::owner_delete_fixture as fixture;
fn setup(source: Arc<CanvasCollabSnapshot>) -> (ShellState, Attempt) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.canvas = Some(crate::state::CanvasTarget {
        id: "Ffixture".into(),
        title: "Test".into(),
        permalink: None,
    });
    shell.overlay = Some(crate::state::Overlay::Canvas);
    open_visit(&mut shell);
    let read = begin_read(&mut shell).unwrap();
    assert!(finish_read(&mut shell, &read, Ok(source)));
    let view = presentation(&shell).unwrap();
    let (visit, read) = (view.visit, view.read);
    let attempt = begin_owner_delete(
        &mut shell,
        visit,
        read,
        "paragraph",
        Arc::new(TextDraft::from_rtml("Independent gap").unwrap()),
    )
    .unwrap();
    (shell, attempt)
}
#[test]
fn generated_target_publication_keeps_dispatch_and_checkpoint_bound() {
    let source = fixture::test_source();
    let (mut shell, a) = setup(source.clone());
    let job = PreparedTarget::capture(a.clone(), &source, Action::After, 100).unwrap();
    assert!(source.section(job.prepared.target_section()).is_none());
    assert!(!mark_dispatched(&mut shell, &a, &job.prepared));
    let (bound, prepared, checkpoint) = job.publish(&mut shell).unwrap();
    assert_ne!(bound.section, a.section);
    assert_eq!(bound.section, prepared.target_section());
    assert_eq!(bound.section, checkpoint.target_section());
    assert!(!preparation_current(&shell, &a));
    assert!(preparation_current(&shell, &bound));
    assert!(
        begin_owner_delete(
            &mut shell,
            bound.visit,
            bound.read,
            "paragraph",
            Arc::new(TextDraft::default())
        )
        .is_err()
    );
    assert!(mark_dispatched(&mut shell, &bound, &prepared));
    assert!(!mark_dispatched(&mut shell, &bound, &prepared));
    assert_eq!(
        retained_request(&shell, &bound),
        Some(prepared.document_data().clone())
    );
}
#[test]
fn held_generated_target_cannot_publish_after_owner_source_or_draft_change() {
    let source = fixture::test_source();
    for change in 0..8 {
        let (mut s, a) = setup(source.clone());
        let job = PreparedTarget::capture(a.clone(), &source, Action::After, 100).unwrap();
        match change {
            0 => {
                begin_read(&mut s).unwrap();
            }
            1 => {
                s.canvas_checklist.view.as_mut().unwrap().snapshot =
                    Some(Arc::new((*source).clone()))
            }
            2 => {
                open_visit(&mut s);
            }
            3 => s.core.account_epoch += 1,
            4 => s.core.active_account = Some("replacement".into()),
            5 => s.canvas_checklist.operations[0].attempt.serial += 1,
            6 => {
                s.canvas_checklist.operations[0].attempt.mutation =
                    Mutation::Structure(Arc::new(TextDraft::from_rtml("Independent gap").unwrap()))
            }
            7 => {
                cancel_preflight(&mut s, &a);
            }
            _ => unreachable!(),
        }
        assert!(job.publish(&mut s).is_none(), "held case {change}");
    }
}
