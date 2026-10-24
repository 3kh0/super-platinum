use super::*;
use crate::canvas::checklist::split_ops::readback::tests as split;
fn setup() -> (ShellState, Attempt, Arc<CanvasCollabSnapshot>, TextIntent) {
    let mut c = split::setup();
    assert!(split::prove(&mut c.shell, &c.read));
    assert!(confirm_readback(&mut c.shell, &c.attempt, &c.read));
    let intent = TextIntent::capture(&c.fresh, "paragraph", 100).unwrap();
    assert_eq!(intent.associated_peer_count(), 2);
    let view = presentation(&c.shell).unwrap();
    let attempt = text_ops::begin_text(
        &mut c.shell,
        view.visit,
        view.read,
        "paragraph",
        Arc::new(TextDraft::from_rtml("Changed").unwrap()),
    )
    .unwrap();
    (c.shell, attempt, c.fresh, intent)
}
fn reopen(shell: &mut ShellState, source: Arc<CanvasCollabSnapshot>) -> Attempt {
    shell.canvas = Some(crate::state::CanvasTarget {
        id: source.file_id().into(),
        title: "Test".into(),
        permalink: None,
    });
    open_visit(shell);
    let read = begin_read(shell).unwrap();
    assert!(finish_read(shell, &read, Ok(source)));
    let view = presentation(shell).unwrap();
    text_ops::begin_text(
        shell,
        view.visit,
        view.read,
        "paragraph",
        Arc::new(TextDraft::from_rtml("Next").unwrap()),
    )
    .unwrap()
}
#[test]
fn busy_lease_refuses_other_file_and_restores_same_actor_without_counter_rollback() {
    let (mut shell, attempt, source, intent) = setup();
    let mut lease = acquire(&mut shell, &attempt).unwrap();
    let first = lease
        .prepare(&intent, &source, text_ops::draft(&attempt).unwrap(), 100)
        .unwrap();
    let actor = first.session_id().to_owned();
    assert_eq!(first.sequence(), 1);
    assert_eq!(reason(&shell), Some(BUSY));
    assert!(with_edit_session(&mut shell, &attempt, |_| ()).is_none());
    assert!(acquire(&mut shell, &attempt).is_err());
    assert_eq!(shell.canvas_checklist.sessions.len(), 1);
    // Construct another file through the real snapshot parser, keeping the
    // current workspace owner. The busy slot survives navigation.
    let mut file = source.file.clone();
    file.id = Some("Fother".into());
    let other = Arc::new(
        CanvasCollabSnapshot::from_responses(file, source.init.clone(), source.load.clone())
            .unwrap(),
    );
    shell.canvas = Some(crate::state::CanvasTarget {
        id: "Fother".into(),
        title: "Other".into(),
        permalink: None,
    });
    open_visit(&mut shell);
    let read = begin_read(&mut shell).unwrap();
    assert!(finish_read(&mut shell, &read, Ok(other)));
    let view = presentation(&shell).unwrap();
    assert!(
        text_ops::begin_text(
            &mut shell,
            view.visit,
            view.read,
            "paragraph",
            Arc::new(TextDraft::from_rtml("Other").unwrap())
        )
        .is_err()
    );
    assert!(!lease.claim().preparing(&shell, &attempt));
    assert!(
        lease.restore(&mut shell),
        "stale view must not discard the mutated session"
    );
    cancel_preflight(&mut shell, &attempt);
    let next = reopen(&mut shell, source.clone());
    let second = with_edit_session(&mut shell, &next, |session| {
        intent.prepare(&source, session, text_ops::draft(&next).unwrap(), 100)
    })
    .unwrap()
    .unwrap();
    assert_eq!(second.session_id(), actor);
    assert_eq!(second.sequence(), 2);
    assert_eq!(
        second
            .document_data()
            .message(1)
            .unwrap()
            .unwrap()
            .uint(20)
            .unwrap(),
        Some(2)
    );
}
#[test]
fn session_claim_refuses_existing_saving_ack_and_poison_never_creates_another_actor() {
    let (mut shell, attempt, _, _) = setup();
    let mut older = attempt.clone();
    older.serial += 100;
    shell.canvas_checklist.operations.push(Operation {
        attempt: older,
        stage: Stage::Saving,
        dispatched_after_read: attempt.read,
        dispatched: Some(("old-session".into(), 1)),
        structural_request: None,
        accepted: None,
        durable: None,
        durable_verified: None,
        durable_finishing: false,
    });
    assert!(acquire(&mut shell, &attempt).is_err());
    assert!(shell.canvas_checklist.sessions.is_empty());
    shell.canvas_checklist.operations.pop();
    let lease = acquire(&mut shell, &attempt).unwrap();
    let claim = lease.claim();
    claim.poison(&mut shell);
    assert!(!lease.restore(&mut shell));
    assert_eq!(reason(&shell), Some(POISONED));
    assert!(acquire(&mut shell, &attempt).is_err());
    assert!(with_edit_session(&mut shell, &attempt, |_| ()).is_none());
    assert_eq!(shell.canvas_checklist.sessions.len(), 1);
    assert!(!POISONED.contains("Refresh"));
}
#[test]
fn stale_claim_cannot_restore_or_poison_replacement_credentials_or_a_new_token() {
    let (mut shell, attempt, source, _) = setup();
    let lease = acquire(&mut shell, &attempt).unwrap();
    let old = lease.claim();
    shell.core.account_epoch += 1;
    assert!(!lease.restore(&mut shell));
    cancel_preflight(&mut shell, &attempt);
    let next = reopen(&mut shell, source);
    let replacement = acquire(&mut shell, &next).unwrap();
    let current = replacement.claim();
    old.poison(&mut shell);
    assert!(current.owns(&shell));
    assert!(replacement.restore(&mut shell));
    assert_eq!(reason(&shell), None);
}
#[test]
fn held_session_preparation_rejects_replaced_source_read_or_draft() {
    for change in 0..4 {
        let (mut shell, attempt, source, _) = setup();
        let lease = acquire(&mut shell, &attempt).unwrap();
        let claim = lease.claim();
        assert!(claim.preparing(&shell, &attempt));
        match change {
            0 => {
                begin_read(&mut shell).unwrap();
            }
            1 => {
                shell.canvas_checklist.view.as_mut().unwrap().snapshot =
                    Some(Arc::new((*source).clone()));
            }
            2 => {
                shell.core.active_account = Some("other".into());
            }
            3 => {
                if let Mutation::Text(draft) =
                    &mut shell.canvas_checklist.operations[0].attempt.mutation
                {
                    *draft = Arc::new((**draft).clone());
                }
            }
            _ => unreachable!(),
        }
        assert!(!claim.preparing(&shell, &attempt));
        assert_eq!(lease.restore(&mut shell), change != 2);
    }
}

fn accepted(prepared: &PreparedTextEdit, source: &CanvasCollabSnapshot) -> AcceptedText {
    use super_platinum_core::canvas::wire::{EditAck, EncodedField as F, RawMessage, Section};
    let update = Section::from_raw(prepared.document_data().message(1).unwrap().unwrap()).unwrap();
    let original = source.section(prepared.section_id()).unwrap();
    let sequence = original.sequence().unwrap().unwrap() + 1000;
    let wire = update
        .crdt()
        .unwrap()
        .unwrap()
        .remove(12)
        .unwrap()
        .replace_fields(3, &[F::uint(3, sequence).unwrap()])
        .unwrap()
        .replace_fields(4, &[F::uint(4, 1_000_000).unwrap()])
        .unwrap();
    let row = original
        .merge_partial(&update)
        .unwrap()
        .raw()
        .replace_fields(2, &[F::uint(2, sequence).unwrap()])
        .unwrap()
        .replace_fields(38, &[F::message(38, &wire).unwrap()])
        .unwrap();
    let data = RawMessage::from_fields(&[F::message(1, &row).unwrap()]).unwrap();
    let response =
        RawMessage::from_fields(&[F::uint(1, 1).unwrap(), F::message(2, &data).unwrap()]).unwrap();
    prepared
        .validate_ack(&EditAck::decode(response.raw()).unwrap())
        .unwrap()
}
#[test]
fn worker_memo_returns_with_same_session_and_is_bound_to_the_exact_draft_attempt() {
    // Each session adopts its ACK exactly once. A second adoption would consume
    // a canonical wire as though it were still the outgoing request.
    for replace_attempt in [true, false] {
        let (mut shell, attempt, source, intent) = setup();
        let mut lease = acquire(&mut shell, &attempt).unwrap();
        let prepared = lease
            .prepare(&intent, &source, text_ops::draft(&attempt).unwrap(), 100)
            .unwrap();
        assert!(text_ops::mark_dispatched(&mut shell, &attempt, &prepared));
        let accepted = accepted(&prepared, &source);
        let mut canonical = (*source).clone();
        *canonical
            .sections
            .iter_mut()
            .find(|s| s.id() == accepted.section().id())
            .unwrap() = accepted.section().clone();
        let memo = lease.remember(accepted);
        assert!(lease.restore(&mut shell));
        if replace_attempt {
            let mut changed = attempt.clone();
            if let Mutation::Text(draft) = &mut changed.mutation {
                *draft = Arc::new((**draft).clone());
            }
            assert!(!text_ops::finish_text_with_memo(&mut shell, &changed, memo));
            assert_eq!(shell.canvas_checklist.operations[0].stage, Stage::Saving);
            continue;
        }
        assert!(text_ops::finish_text_with_memo(&mut shell, &attempt, memo));
        assert!(matches!(
            shell.canvas_checklist.operations[0].accepted,
            Some(Accepted::TextSequence(_))
        ));
        // A subsequent edit consumes the canonical ACK with the same actor and
        // advances counters while retaining the acknowledged authored head.
        let next_intent = TextIntent::capture(&canonical, "paragraph", 100).unwrap();
        let next = next_intent
            .prepare(
                &canonical,
                shell.canvas_checklist.sessions[0].session.as_mut().unwrap(),
                &TextDraft::from_rtml("Next").unwrap(),
                100,
            )
            .unwrap();
        assert_eq!(next.session_id(), prepared.session_id());
        assert_eq!(next.sequence(), prepared.sequence() + 1);
        let old_wire = prepared
            .document_data()
            .message(1)
            .unwrap()
            .unwrap()
            .message(38)
            .unwrap()
            .unwrap();
        let new_row = next.document_data().message(1).unwrap().unwrap();
        assert_eq!(new_row.uint(20).unwrap(), Some(2));
        let new_wire = new_row.message(38).unwrap().unwrap();
        let previous_head = old_wire.string(12).unwrap().unwrap();
        assert_ne!(new_wire.string(12).unwrap(), Some(previous_head));
        assert!(new_wire.fields().iter().filter(|f| f.tag() == 6).any(|f| {
            super_platinum_core::canvas::wire::RawMessage::decode(
                new_wire.encoded_field(f).unwrap(),
            )
            .unwrap()
            .string(6)
            .unwrap()
                == Some(previous_head)
        }));
    }
}
