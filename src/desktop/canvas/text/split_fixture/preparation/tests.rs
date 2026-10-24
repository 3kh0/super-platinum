use super::*;
use super_platinum_core::canvas::edit::text::TextDraft;
fn setup() -> (ShellState, checklist::Attempt, Arc<CanvasCollabSnapshot>) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.canvas = Some(crate::state::CanvasTarget {
        id: "Ffixture".into(),
        title: "Test".into(),
        permalink: None,
    });
    shell.overlay = Some(crate::state::Overlay::Canvas);
    checklist::open_visit(&mut shell);
    let mut snapshot = (*super::super::super::structure_fixture::snapshot()).clone();
    let row = snapshot
        .sections
        .iter_mut()
        .find(|row| row.id() == "paragraph")
        .unwrap();
    let text = RawMessage::from_fields(&[
        F::bytes(1, b"First paragraph").unwrap(),
        F::bytes(2, b"en").unwrap(),
    ])
    .unwrap();
    let body = RawMessage::from_fields(&[F::message(1, &text).unwrap()]).unwrap();
    *row = Section::from_raw(
        row.raw()
            .replace_fields(12, &[F::message(12, &body).unwrap()])
            .unwrap(),
    )
    .unwrap();
    let source = Arc::new(snapshot);
    let read = checklist::begin_read(&mut shell).unwrap();
    assert!(checklist::finish_read(
        &mut shell,
        &read,
        Ok(source.clone())
    ));
    shell.canvas_checklist.split_fixture = Some(super::super::State {
        canonical: Some(source.clone()),
        ..Default::default()
    });
    let seed = TextDraft::from_rtml("First paragraph").unwrap();
    let witness = SplitWitness::new(
        &seed,
        6,
        &TextDraft::from_rtml("First ").unwrap(),
        &TextDraft::from_rtml("paragraph").unwrap(),
    )
    .unwrap();
    let view = checklist::presentation(&shell).unwrap();
    let (attempt, _) = checklist::split_ops::begin_split(
        &mut shell,
        view.visit,
        view.read,
        "paragraph",
        witness,
        100,
    )
    .unwrap();
    (shell, attempt, source)
}
#[test]
fn prepared_split_rejects_replaced_canonical_even_if_bytes_equal() {
    let (mut shell, attempt, source) = setup();
    assert!(current(&shell, &attempt, &source));
    shell
        .canvas_checklist
        .split_fixture
        .as_mut()
        .unwrap()
        .canonical = Some(Arc::new((*source).clone()));
    assert!(!current(&shell, &attempt, &source));
}
#[test]
fn prepared_split_rechecks_owner_read_visit_target_and_operation() {
    for change in 0..7 {
        let (mut shell, mut attempt, source) = setup();
        assert!(current(&shell, &attempt, &source));
        match change {
            0 => {
                shell.core.account_epoch += 1;
            }
            1 => {
                shell.core.active_account = Some("other".into());
            }
            2 => {
                checklist::begin_read(&mut shell).unwrap();
            }
            3 => {
                checklist::open_visit(&mut shell);
            }
            4 => {
                assert!(checklist::invalidate(&mut shell, "T1", "Ffixture"));
            }
            5 => {
                attempt.section = "neighbor".into();
            }
            6 => {
                checklist::cancel_preflight(&mut shell, &attempt);
            }
            _ => unreachable!(),
        }
        assert!(!current(&shell, &attempt, &source), "change {change}");
    }
}
