use super::*;
use crate::canvas::text::owner_delete_fixture as fixture;
use super_platinum_core::canvas::edit::{structure::StructureIntent, text::TextDraft};
#[test]
fn saved_pair_insertion_uses_exact_worker_readback_and_retains_originals() {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.canvas = Some(crate::state::CanvasTarget {
        id: "Ffixture".into(),
        title: "Test".into(),
        permalink: None,
    });
    shell.overlay = Some(crate::state::Overlay::Canvas);
    open_visit(&mut shell);
    let source = fixture::test_source();
    let r = begin_read(&mut shell).unwrap();
    assert!(finish_read(&mut shell, &r, Ok(source.clone())));
    let v = presentation(&shell).unwrap();
    let (visit, read) = (v.visit, v.read);
    let a = begin_owner_delete(
        &mut shell,
        visit,
        read,
        "paragraph",
        Arc::new(TextDraft::from_rtml("Independent gap").unwrap()),
    )
    .unwrap();
    let worker = PreparedTarget::capture(a, &source, Action::After, 100).unwrap();
    let (a, p, _) = worker.publish(&mut shell).unwrap();
    assert!(mark_dispatched(&mut shell, &a, &p));
    let (accepted, fresh) = fixture::test_delete(p, source.clone());
    assert!(
        source
            .sections
            .iter()
            .all(|s| fresh.section(s.id()) == Some(s))
    );
    assert!(finish_structure(&mut shell, &a, accepted));
    let r = begin_read(&mut shell).unwrap();
    assert!(finish_read(&mut shell, &r, Ok(fresh.clone())));
    assert!(!confirm_readback(&mut shell, &a, &r));
    let stale = Job::capture(&shell, &r).unwrap().run().unwrap();
    let current = Job::capture(&shell, &r).unwrap().run().unwrap();
    shell.canvas_checklist.view.as_mut().unwrap().snapshot = Some(Arc::new((*fresh).clone()));
    assert!(!stale.publish(&mut shell));
    shell.canvas_checklist.view.as_mut().unwrap().snapshot = Some(fresh);
    assert!(current.publish(&mut shell));
    assert!(confirm_readback(&mut shell, &a, &r));
}
fn setup() -> (ShellState, Attempt, Read, Arc<CanvasCollabSnapshot>) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.canvas = Some(crate::state::CanvasTarget {
        id: "Ffixture".into(),
        title: "Test".into(),
        permalink: None,
    });
    shell.overlay = Some(crate::state::Overlay::Canvas);
    open_visit(&mut shell);
    let source = fixture::test_source();
    let read = begin_read(&mut shell).unwrap();
    assert!(finish_read(&mut shell, &read, Ok(source.clone())));
    let view = presentation(&shell).unwrap();
    let a = begin_owner_delete(
        &mut shell,
        view.visit,
        view.read,
        "paragraph",
        Arc::new(TextDraft::from_rtml("Keep this").unwrap()),
    )
    .unwrap();
    assert!(preparation_current(&shell, &a));
    let p = StructureIntent::delete(&source, "paragraph", 100)
        .unwrap()
        .prepare(&source, 100)
        .unwrap();
    assert!(mark_dispatched(&mut shell, &a, &p));
    assert!(fixture_completion_current(
        &shell,
        &a,
        p.session_id(),
        p.sequence()
    ));
    let (accepted, fresh) = fixture::test_delete(p, source);
    assert!(finish_structure(&mut shell, &a, accepted));
    let read = begin_read(&mut shell).unwrap();
    assert!(finish_read(&mut shell, &read, Ok(fresh.clone())));
    (shell, a, read, fresh)
}
#[test]
fn owner_delete_requires_executed_exact_read_proof() {
    let (mut s, a, r, _) = setup();
    assert!(!confirm_readback(&mut s, &a, &r));
    let proof = Job::capture(&s, &r).unwrap().run().unwrap();
    assert!(proof.publish(&mut s));
    assert!(confirm_readback(&mut s, &a, &r));
}
#[test]
fn held_owner_proof_cannot_publish_after_authority_changes() {
    for change in 0..9 {
        let (mut s, a, r, fresh) = setup();
        let proof = Job::capture(&s, &r).unwrap().run().unwrap();
        match change {
            0 => {
                begin_read(&mut s).unwrap();
            }
            1 => {
                s.canvas_checklist.view.as_mut().unwrap().snapshot =
                    Some(Arc::new((*fresh).clone()))
            }
            2 => {
                invalidate(&mut s, "T1", "Ffixture");
            }
            3 => {
                open_visit(&mut s);
            }
            4 => s.core.account_epoch += 1,
            5 => s.core.active_account = Some("other".into()),
            6 => s.canvas_checklist.operations[0].attempt.serial += 1,
            7 => {
                s.canvas_checklist.operations[0]
                    .dispatched
                    .as_mut()
                    .unwrap()
                    .1 += 1
            }
            8 => {
                s.canvas_checklist.operations[0].attempt.mutation =
                    Mutation::Structure(Arc::new(TextDraft::from_rtml("Keep this").unwrap()))
            }
            _ => unreachable!(),
        };
        assert!(!proof.publish(&mut s), "change {change}");
        assert!(!confirm_readback(&mut s, &a, &r));
    }
}
#[test]
fn missing_owner_or_child_never_proves_deletion() {
    for missing_owner in [false, true] {
        let (mut s, a, _, fresh) = setup();
        let mut changed = (*fresh).clone();
        let child = changed
            .section("paragraph")
            .unwrap()
            .crdt()
            .unwrap()
            .unwrap()
            .messages(10)
            .unwrap()[0]
            .string(3)
            .unwrap()
            .unwrap()
            .to_owned();
        changed
            .sections
            .retain(|r| r.id() != if missing_owner { "paragraph" } else { &child });
        let read = begin_read(&mut s).unwrap();
        assert!(finish_read(&mut s, &read, Ok(Arc::new(changed))));
        assert!(Job::capture(&s, &read).unwrap().run().is_none());
        assert!(!confirm_readback(&mut s, &a, &read));
    }
}
