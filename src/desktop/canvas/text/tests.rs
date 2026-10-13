use super::*;
use super_platinum_core::{
    canvas::{
        CanvasDocument,
        edit::text::{AcceptedText, PreparedTextEdit, TextDraft},
        wire::{EditAck, EncodedField as F, RawMessage},
    },
    slack::api::CanvasCollabSnapshot,
};

fn loaded() -> (ShellState, Arc<CanvasCollabSnapshot>) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.canvas = Some(crate::state::CanvasTarget {
        id: "Ffixture".into(),
        title: "Test".into(),
        permalink: None,
    });
    shell.overlay = Some(crate::state::Overlay::Canvas);
    checklist::open_visit(&mut shell);
    let snapshot = fixture::snapshot("Original <b>paragraph</b>", 1000);
    let read = checklist::begin_read(&mut shell).unwrap();
    assert!(checklist::finish_read(
        &mut shell,
        &read,
        Ok(Arc::clone(&snapshot))
    ));
    (shell, snapshot)
}
fn begin(shell: &mut ShellState) -> checklist::Attempt {
    let view = checklist::presentation(shell).unwrap();
    checklist::text_ops::begin_text(
        shell,
        view.visit,
        view.read,
        "paragraph",
        Arc::new(TextDraft::from_rtml("Changed <i>paragraph</i>").unwrap()),
    )
    .unwrap()
}
fn prepared(shell: &mut ShellState, attempt: &checklist::Attempt) -> PreparedTextEdit {
    let intent = TextIntent::capture(&attempt.snapshot, "paragraph", 1_000_000).unwrap();
    checklist::with_edit_session(shell, attempt, |session| {
        intent.prepare(
            &attempt.snapshot,
            session,
            checklist::text_ops::draft(attempt).unwrap(),
            1_000_000,
        )
    })
    .unwrap()
    .unwrap()
}
fn accepted(
    prepared: &PreparedTextEdit,
    original: &CanvasCollabSnapshot,
) -> (AcceptedText, Arc<CanvasCollabSnapshot>) {
    let sent = prepared.document_data().message(1).unwrap().unwrap();
    let sequence = prepared.base_sequence() + 1000;
    let wire = sent
        .message(38)
        .unwrap()
        .unwrap()
        .remove(12)
        .unwrap()
        .replace_fields(3, &[F::uint(3, sequence).unwrap()])
        .unwrap()
        .replace_fields(4, &[F::uint(4, 60_000_000).unwrap()])
        .unwrap();
    let raw = original
        .section("paragraph")
        .unwrap()
        .raw()
        .replace_fields(2, &[F::uint(2, sequence).unwrap()])
        .unwrap()
        .replace_fields(
            12,
            &[F::message(12, &sent.message(12).unwrap().unwrap()).unwrap()],
        )
        .unwrap()
        .replace_fields(38, &[F::message(38, &wire).unwrap()])
        .unwrap();
    let data = RawMessage::from_fields(&[
        F::bytes(15, b"document").unwrap(),
        F::message(1, &raw).unwrap(),
    ])
    .unwrap();
    let outer =
        RawMessage::from_fields(&[F::uint(1, 1).unwrap(), F::message(2, &data).unwrap()]).unwrap();
    let ack = EditAck::decode(outer.raw()).unwrap();
    let accepted = prepared.validate_ack(&ack).unwrap();
    let mut snapshot = original.clone();
    *snapshot
        .sections
        .iter_mut()
        .find(|s| s.id() == "paragraph")
        .unwrap() = accepted.section().clone();
    (accepted, Arc::new(snapshot))
}

#[test]
fn canonical_seed_never_comes_from_export_and_ambiguous_anchors_are_inert() {
    let (shell, snapshot) = loaded();
    let parse = |body: &str| {
        CanvasDocument::parse(&format!("<div class='quip-canvas-content'>{body}</div>")).unwrap()
    };
    let document = parse(
        "<p id='paragraph'><span id='paragraph'>Lagging export</span></p><p id='unsupported'>Looks plain in export</p>",
    );
    let map = Mapping::project(&shell, &document, Some(&snapshot));
    assert_eq!(map.items.len(), 1);
    assert!(
        *map.items["paragraph"].intent.seed()
            == TextDraft::from_rtml("Original <b>paragraph</b>").unwrap()
    );
    for body in [
        "<p id='paragraph'>One</p><p id='paragraph'>Two</p>",
        "<p id='paragraph'>One</p><span id='paragraph'>Outside</span>",
        "<p id='paragraph'><span id='paragraph'>One</span><span id='paragraph'>Two</span></p>",
    ] {
        assert!(
            Mapping::project(&shell, &parse(body), Some(&snapshot))
                .items
                .is_empty()
        );
    }
}

#[test]
fn text_and_checklist_share_one_file_lock_and_preflight_cancel_releases_only_unsent_work() {
    let (mut shell, _) = loaded();
    let attempt = begin(&mut shell);
    let view = checklist::presentation(&shell).unwrap();
    assert!(checklist::begin(&mut shell, view.visit, view.read, "item", true).is_err());
    assert!(
        checklist::text_ops::begin_text(
            &mut shell,
            view.visit,
            view.read,
            "paragraph",
            Arc::new(TextDraft::default())
        )
        .is_err()
    );
    assert!(checklist::cancel_preflight(&mut shell, &attempt));
    let next = begin(&mut shell);
    let prepared = prepared(&mut shell, &next);
    assert!(checklist::text_ops::mark_dispatched(
        &mut shell, &next, &prepared
    ));
    assert!(!checklist::cancel_preflight(&mut shell, &next));
    assert!(checklist::finish(
        &mut shell,
        &next,
        checklist::Outcome::Uncertain
    ));
    checklist::close_visit(&mut shell);
    shell.shortcut_generation += 1;
    checklist::open_visit(&mut shell);
    assert!(checklist::presentation(&shell).unwrap().operation.is_some());
}

#[test]
fn stale_read_and_account_cannot_prepare_or_release_another_operation() {
    let (mut shell, _) = loaded();
    let attempt = begin(&mut shell);
    assert!(checklist::dispatchable(&shell, &attempt));
    assert!(checklist::invalidate(&mut shell, "T1", "Ffixture"));
    assert!(!checklist::dispatchable(&shell, &attempt));
    assert!(checklist::with_edit_session(&mut shell, &attempt, |_| ()).is_none());
    shell.core.account_epoch += 1;
    assert!(!checklist::dispatchable(&shell, &attempt));
}

#[test]
fn accepted_text_after_reopen_waits_for_new_visits_canonical_read() {
    let (mut shell, snapshot) = loaded();
    let attempt = begin(&mut shell);
    let prepared = prepared(&mut shell, &attempt);
    assert!(checklist::text_ops::mark_dispatched(
        &mut shell, &attempt, &prepared
    ));
    let (accepted, updated) = accepted(&prepared, &snapshot);
    shell.dismiss_overlay();
    shell.shortcut_generation += 1;
    shell.overlay = Some(crate::state::Overlay::Canvas);
    checklist::open_visit(&mut shell);
    assert!(checklist::text_ops::finish_text(
        &mut shell, &attempt, accepted
    ));
    let read = checklist::begin_read(&mut shell).unwrap();
    checklist::finish_read(&mut shell, &read, Ok(snapshot));
    assert!(!checklist::confirm_readback(&mut shell, &attempt, &read));
    let read = checklist::begin_read(&mut shell).unwrap();
    checklist::finish_read(&mut shell, &read, Ok(updated));
    assert!(checklist::confirm_readback(&mut shell, &attempt, &read));
    assert!(checklist::presentation(&shell).unwrap().operation.is_none());
}

fn pending_draft(attempt: &checklist::Attempt) -> Draft {
    let intent = Arc::new(TextIntent::capture(&attempt.snapshot, "paragraph", 1_000_000).unwrap());
    Draft {
        generation: 1,
        visit: attempt.visit(),
        read: 0,
        section: "paragraph".into(),
        blocks: serde_json::to_string(&intent.seed().rich_blocks()).unwrap(),
        intent,
        reading: false,
        pending: Some(attempt.clone()),
        error: None,
    }
}

#[test]
fn late_file_event_cannot_strand_a_canonically_settled_text_draft() {
    let (mut shell, snapshot) = loaded();
    let attempt = begin(&mut shell);
    let draft = pending_draft(&attempt);
    let prepared = prepared(&mut shell, &attempt);
    assert!(checklist::text_ops::mark_dispatched(
        &mut shell, &attempt, &prepared
    ));
    let (accepted, updated) = accepted(&prepared, &snapshot);
    assert!(checklist::text_ops::finish_text(
        &mut shell, &attempt, accepted
    ));
    assert!(!should_close(&shell, Some(&draft)));
    let read = checklist::begin_read(&mut shell).unwrap();
    assert!(checklist::finish_read(
        &mut shell,
        &read,
        Ok(updated.clone())
    ));
    assert!(checklist::confirm_readback(&mut shell, &attempt, &read));

    // The socket event wins the race with Dioxus's next effect, after proof has
    // removed the receipt. A stale cache must not erase that positive proof.
    assert!(checklist::invalidate(&mut shell, "T1", "Ffixture"));
    assert!(checklist::presentation(&shell).unwrap().stale);
    assert!(should_close(&shell, Some(&draft)));
    let visit = checklist::event_reads::needed(&shell).unwrap();
    let read = checklist::event_reads::claim(&mut shell, visit).unwrap();
    assert!(should_close(&shell, Some(&draft)));
    assert!(checklist::event_reads::claim(&mut shell, visit).is_none());
    assert!(checklist::finish_read(&mut shell, &read, Ok(updated)));
    assert!(!checklist::presentation(&shell).unwrap().stale);
    assert!(checklist::event_reads::needed(&shell).is_none());

    shell.dismiss_overlay();
    assert!(!should_close(&shell, Some(&draft)));
}

#[test]
fn absent_or_uncertain_receipt_is_not_successful_text_settlement() {
    let (mut shell, _) = loaded();
    let attempt = begin(&mut shell);
    let draft = pending_draft(&attempt);
    assert!(checklist::cancel_preflight(&mut shell, &attempt));
    assert!(!should_close(&shell, Some(&draft)));
    let attempt = begin(&mut shell);
    let draft = pending_draft(&attempt);
    let prepared = prepared(&mut shell, &attempt);
    assert!(checklist::text_ops::mark_dispatched(
        &mut shell, &attempt, &prepared
    ));
    assert!(checklist::invalidate(&mut shell, "T1", "Ffixture"));
    assert!(
        checklist::event_reads::needed(&shell).is_none(),
        "Saving blocks event reads"
    );
    assert!(checklist::finish(
        &mut shell,
        &attempt,
        checklist::Outcome::Uncertain
    ));
    assert!(checklist::invalidate(&mut shell, "T1", "Ffixture"));
    assert!(!should_close(&shell, Some(&draft)));
}

#[test]
fn event_reads_coalesce_revisions_and_stop_after_errors_or_owner_change() {
    let (mut shell, snapshot) = loaded();
    let visit = checklist::presentation(&shell).unwrap().visit;
    assert!(checklist::event_reads::needed(&shell).is_none());
    let attempt = begin(&mut shell);
    assert!(checklist::invalidate(&mut shell, "T1", "Ffixture"));
    assert!(
        checklist::event_reads::needed(&shell).is_none(),
        "Preparing blocks reads"
    );
    assert!(checklist::cancel_preflight(&mut shell, &attempt));
    assert!(checklist::invalidate(&mut shell, "T1", "Ffixture"));
    let read = checklist::event_reads::claim(&mut shell, visit).unwrap();
    assert!(checklist::event_reads::needed(&shell).is_none());
    assert!(checklist::invalidate(&mut shell, "T1", "Ffixture"));
    // An invalidated 429/error still suppresses automatic event retries.
    checklist::event_reads::block_after_error(&mut shell, &read);
    assert!(!checklist::finish_read(
        &mut shell,
        &read,
        Err("Rate limited")
    ));
    assert!(checklist::event_reads::needed(&shell).is_none());
    assert!(checklist::invalidate(&mut shell, "T1", "Ffixture"));
    assert!(checklist::event_reads::needed(&shell).is_none());
    // Explicit recovery can restart reads; an old failure cannot block it.
    let manual = checklist::begin_read(&mut shell).unwrap();
    checklist::event_reads::block_after_error(&mut shell, &read);
    assert!(checklist::finish_read(&mut shell, &manual, Ok(snapshot)));
    assert!(checklist::invalidate(&mut shell, "T1", "Ffixture"));
    assert_eq!(checklist::event_reads::needed(&shell), Some(visit));
    shell.core.account_epoch += 1;
    assert!(checklist::event_reads::claim(&mut shell, visit).is_none());
}
