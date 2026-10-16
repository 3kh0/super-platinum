//! Actual private SQLite and production load/capture/review transitions.
use super::*;
use crate::file_share::{
    draft_io::{Command, Outcome, Queue, Ticket},
    durable::capture::Capture,
};
use super_platinum_core::file_sharing::store::drafts::{Content, Draft, Mutation};

struct Fixture {
    temp: Temp,
    shell: ShellState,
    target: crate::file_actions::Target,
    id: String,
}
impl Fixture {
    async fn new() -> Self {
        let temp = Temp::new();
        let (mut shell, target) = sample();
        let id = open(&mut shell, target.clone(), "source".into(), &temp)
            .await
            .unwrap();
        assert_eq!(visible(&shell).unwrap().phase, Phase::Editing);
        Self {
            temp,
            shell,
            target,
            id,
        }
    }
    fn queue(&self) -> Queue {
        self.shell
            .file_share
            .custody
            .lock()
            .unwrap()
            .draft_io
            .clone()
    }
    fn draft(&self) -> Draft {
        visible(&self.shell).unwrap().durable.clone().unwrap()
    }
    fn storage(&self) -> recovery::Storage {
        visible(&self.shell).unwrap().draft_storage.clone().unwrap()
    }
    async fn row(&self) -> Option<Draft> {
        let scope = self.draft().scope;
        self.storage().run(move |j| j.draft(&scope)).await.unwrap()
    }
    fn capture(&mut self) -> Capture {
        Capture::prepare(&mut self.shell, &self.id).unwrap()
    }
    fn recovery(&mut self, rows: Vec<Draft>, local: Vec<Ticket>) {
        close(&mut self.shell, &self.id);
        let r = &mut self.shell.file_share.recovery;
        r.open = true;
        r.loading = false;
        r.serial += 1;
        r.drafts = rows;
        r.local = local;
        self.shell.overlay = Some(Overlay::FileShare);
    }
}
fn destination() -> Destination {
    Destination {
        id: "C00000001".into(),
        name: "ship".into(),
        private: false,
    }
}
fn submit(
    queue: &Queue,
    storage: recovery::Storage,
    visit: Arc<drafts::Visit>,
    command: Command,
) -> Ticket {
    match queue.reserve(storage, visit).unwrap().submit(command) {
        Ok(t) => t,
        Err(_) => panic!("bounded command"),
    }
}
async fn result(ticket: &Ticket) -> Outcome {
    tokio::time::timeout(Duration::from_secs(5), ticket.wait())
        .await
        .expect("bounded actual completion")
}
fn saved(outcome: Outcome) -> Draft {
    match outcome {
        Outcome::Saved(d) => d,
        _ => panic!("committed exact draft"),
    }
}

#[tokio::test]
async fn close_content_keeps_destination_only_and_new_open_rotates_exact_saved_claim() {
    let mut f = Fixture::new().await;
    let old = f.draft();
    let ticket = f
        .capture()
        .persist(vec![], Some(destination()), true)
        .await
        .unwrap();
    let stored = saved(result(&ticket).await);
    assert!(stored.content.note.is_empty() && stored.content.destination.is_some());
    assert!(stored.revision > old.revision && stored.token == old.token);
    close(&mut f.shell, &f.id);
    let reopened = open(&mut f.shell, f.target.clone(), "source".into(), &f.temp)
        .await
        .unwrap();
    let d = visible(&f.shell).unwrap();
    assert_eq!(d.id, reopened);
    assert!(d.destination.as_ref() == Some(&destination()));
    assert!(d.durable.as_ref().unwrap().content == stored.content);
    assert_ne!(d.durable.as_ref().unwrap().token, stored.token);
}
#[tokio::test]
async fn a_storage_failed_open_never_mounts_an_empty_editable_replacement() {
    let temp = Temp::new();
    let (mut shell, target) = sample();
    let source = admission::Source::capture(&shell, &target).unwrap();
    let load =
        durable::open::prepare_captured(&mut shell, source, "source".into(), Err(Refusal::Storage))
            .unwrap();
    assert!(!visible(&shell).unwrap().editor_ready);
    load.publish(&mut shell, load.outcome().await);
    let d = visible(&shell).unwrap();
    assert_eq!(d.phase, Phase::LoadFailed);
    assert!(!d.editor_ready && d.durable.is_none() && d.error.is_some());
    assert!(std::fs::read_dir(&temp.0).unwrap().next().is_none());
}
#[tokio::test]
async fn dropped_and_rejected_share_reservations_wake_close_without_rebasing() {
    for reject in [false, true] {
        let mut f = Fixture::new().await;
        let original = f.draft();
        let share = f.capture();
        let observer = share.ticket.clone();
        let close = f.capture();
        if reject {
            assert!(
                share
                    .retain(vec![json!({"oversized":"x".repeat(300*1024)})], None)
                    .is_err()
            );
        } else {
            drop(share);
        }
        assert!(matches!(result(&observer).await, Outcome::Failed(_)));
        let ticket = close
            .persist(note(), Some(destination()), true)
            .await
            .unwrap();
        let committed = saved(result(&ticket).await);
        assert_eq!(committed.token, original.token);
        assert_eq!(committed.content.note, note());
        assert!(
            f.queue()
                .retained()
                .unwrap()
                .iter()
                .all(|t| !t.same(&observer))
        );
    }
}
#[tokio::test]
async fn reversed_dom_read_completions_chain_only_same_visit_saved_version() {
    let mut f = Fixture::new().await;
    let share = f.capture();
    let close = f.capture();
    let later = note();
    let mut earlier = note();
    earlier[0]["future"] = json!(99);
    let close_task = tokio::spawn(close.persist(later.clone(), Some(destination()), true));
    tokio::task::yield_now().await;
    assert!(!close_task.is_finished());
    let first = share
        .persist(earlier.clone(), Some(destination()), false)
        .await
        .unwrap();
    let first_saved = saved(result(&first).await);
    let second = close_task.await.unwrap().unwrap();
    let last = saved(result(&second).await);
    assert_eq!(first_saved.content.note, earlier);
    assert_eq!(last.content.note, later);
    assert_eq!(last.revision, first_saved.revision + 1);
    assert!(f.row().await == Some(last));
}
#[tokio::test]
async fn predecessor_conflict_retains_later_capture_without_claim_or_rebase() {
    let mut f = Fixture::new().await;
    let original = f.draft();
    let storage = f.storage();
    let share = f.capture();
    let close = f.capture();
    let expected = original.clone();
    let mut external = expected.content.clone();
    external.note = note();
    let external = storage
        .run(move |j| j.save_draft(&expected, external))
        .await
        .unwrap();
    let Mutation::Applied(external) = external else {
        panic!("external exact write")
    };
    let first = share.persist(note(), None, false).await.unwrap();
    assert!(matches!(result(&first).await, Outcome::Conflict(_)));
    let last = close
        .persist(vec![], Some(destination()), true)
        .await
        .unwrap();
    assert!(matches!(
        result(&last).await,
        Outcome::Failed(Refusal::Changed)
    ));
    assert!(durable::unsaved(&last));
    assert!(durable::captured(&last).unwrap().destination.is_some());
    assert!(f.row().await == Some(external));
}
#[tokio::test]
async fn readonly_source_missing_review_allows_only_exact_discard_at_worker_boundary() {
    let mut f = Fixture::new().await;
    let expected = f.draft();
    let queue = f.queue();
    let storage = f.storage();
    let visit = drafts::Visit::review(
        &admission::Context::capture(&f.shell).unwrap().key,
        &expected,
    )
    .unwrap();
    for command in [
        Command::LoadClaim {
            initial: expected.content.clone(),
        },
        Command::Claim {
            expected: Some(expected.clone()),
            initial: expected.content.clone(),
        },
        Command::Save {
            expected: expected.clone(),
            content: expected.content.clone(),
        },
    ] {
        let ticket = submit(&queue, storage.clone(), visit.clone(), command);
        assert!(matches!(
            result(&ticket).await,
            Outcome::Failed(Refusal::Changed)
        ));
        queue.acknowledge(&ticket).unwrap();
    }
    f.recovery(vec![expected.clone()], vec![]);
    crate::file_actions::attachment::tests::message(&mut f.shell)
        .files
        .clear();
    let serial = f.shell.file_share.recovery.serial;
    let id = durable::saved::review(&mut f.shell, serial, &expected).unwrap();
    // Keep the actual review/identity path, but route its later I/O back to
    // this test's owned database rather than ordinary account-default storage.
    f.shell.file_share.dialog.as_mut().unwrap().draft_storage = Some(storage.clone());
    assert!(visible(&f.shell).unwrap().admission.is_none());
    assert!(
        !visible(&f.shell)
            .unwrap()
            .draft_visit
            .as_ref()
            .unwrap()
            .writable()
    );
    let (_, _, Some((queue, ticket, _))) =
        durable::saved::prepare_discard(&mut f.shell, &id).unwrap()
    else {
        panic!("exact readonly discard")
    };
    assert!(matches!(result(&ticket).await, Outcome::Discarded));
    queue.acknowledge(&ticket).unwrap();
    let scope = expected.scope;
    assert!(
        storage
            .run(move |j| j.draft(&scope))
            .await
            .unwrap()
            .is_none()
    );
}
#[tokio::test]
async fn unresolved_receipt_precedes_changed_source_review_without_rotating_draft() {
    let mut f = Fixture::new().await;
    let t = f
        .capture()
        .persist(note(), Some(destination()), false)
        .await
        .unwrap();
    let expected = saved(result(&t).await);
    let copy = expected.clone();
    let begun = f
        .storage()
        .run(move |j| j.begin_from_draft(&copy, 10))
        .await
        .unwrap();
    let Begin::Started(receipt) = begun else {
        panic!("admission")
    };
    close(&mut f.shell, &f.id);
    crate::file_actions::attachment::tests::message(&mut f.shell).files[0]
        .extra
        .insert("changed".into(), json!(true));
    open(&mut f.shell, f.target.clone(), "source".into(), &f.temp)
        .await
        .unwrap();
    let d = visible(&f.shell).unwrap();
    assert_eq!(d.phase, Phase::Outcome);
    assert_eq!(d.receipt.as_ref().unwrap().attempt_id, receipt.attempt_id);
    let scope = expected.scope.clone();
    let storage = recovery::Storage::at(
        admission::Context::capture(&f.shell).unwrap().key,
        f.temp.0.join("draft.sqlite"),
    );
    assert!(storage.run(move |j| j.draft(&scope)).await.unwrap() == Some(expected));
}
#[tokio::test]
async fn close_save_wins_before_begin_and_begin_wins_before_close_have_distinct_exact_results() {
    for begin_first in [false, true] {
        let mut f = Fixture::new().await;
        let t = f
            .capture()
            .persist(note(), Some(destination()), false)
            .await
            .unwrap();
        let prepared = saved(result(&t).await);
        let q = f.queue();
        let visit = visible(&f.shell).unwrap().draft_visit.clone().unwrap();
        let storage = f.storage();
        let mut next = prepared.content.clone();
        next.note[0]["future"] = json!(11);
        if begin_first {
            let copy = prepared.clone();
            let begin = storage
                .run(move |j| j.begin_from_draft(&copy, 10))
                .await
                .unwrap();
            let Begin::Started(receipt) = begin else {
                panic!("started")
            };
            let t = submit(
                &q,
                storage.clone(),
                visit,
                Command::Save {
                    expected: prepared.clone(),
                    content: next,
                },
            );
            assert!(
                matches!(result(&t).await,Outcome::Blocked(r) if r.attempt_id==receipt.attempt_id)
            );
            assert!(durable::unsaved(&t));
            assert!(f.row().await == Some(prepared));
        } else {
            let t = submit(
                &q,
                storage.clone(),
                visit,
                Command::Save {
                    expected: prepared.clone(),
                    content: next.clone(),
                },
            );
            let committed = saved(result(&t).await);
            assert!(
                storage
                    .run(move |j| j.begin_from_draft(&prepared, 10))
                    .await
                    .is_err()
            );
            assert!(f.row().await == Some(committed));
        }
    }
}
#[tokio::test]
async fn captured_conflict_review_keep_and_discard_preserve_exact_ticket_identity() {
    let mut f = Fixture::new().await;
    let capture = f.capture();
    let ticket = capture.retain(note(), Some(destination())).unwrap();
    assert!(matches!(
        result(&ticket).await,
        Outcome::Failed(Refusal::Storage)
    ));
    f.recovery(vec![], vec![ticket.clone()]);
    let serial = f.shell.file_share.recovery.serial;
    let review = durable::saved::review_local(&mut f.shell, serial, ticket.id()).unwrap();
    let d = visible(&f.shell).unwrap();
    assert!(d.draft_local && d.note == note() && d.admission.is_none());
    close(&mut f.shell, &review); // Keep for this session uses this same logical close after focus handoff.
    assert!(
        f.queue()
            .retained()
            .unwrap()
            .iter()
            .any(|t| t.same(&ticket))
    );
    f.shell.file_share.recovery.open = true;
    f.shell.overlay = Some(Overlay::FileShare);
    assert!(durable::saved::review_local(&mut f.shell, serial + 1, ticket.id()).is_err());
    let id = durable::saved::review_local(&mut f.shell, serial, ticket.id()).unwrap();
    let (_, _, work) = durable::saved::prepare_discard(&mut f.shell, &id).unwrap();
    assert!(work.is_none());
    assert!(
        !f.queue()
            .retained()
            .unwrap()
            .iter()
            .any(|t| t.same(&ticket))
    );
}

#[tokio::test]
async fn failed_save_keeps_editor_retryable_against_same_version_and_retains_old_input() {
    let mut f = Fixture::new().await;
    let expected = f.draft();
    let storage = f.storage();
    let bad = recovery::Storage::at(storage.key.clone(), f.temp.0.clone());
    f.shell.file_share.dialog.as_mut().unwrap().draft_storage = Some(bad);
    let capture = f.capture();
    let visit = capture.visit.clone();
    let failed = capture
        .persist(note(), Some(destination()), true)
        .await
        .unwrap();
    let outcome = result(&failed).await;
    assert!(matches!(outcome, Outcome::Failed(Refusal::Storage)));
    assert!(durable::apply_capture(
        &mut f.shell,
        &f.id,
        &visit,
        &failed,
        &outcome
    ));
    let d = visible(&f.shell).unwrap();
    assert_eq!(d.phase, Phase::Editing);
    assert!(d.durable.as_ref() == Some(&expected) && d.error.is_some());
    f.shell.file_share.dialog.as_mut().unwrap().draft_storage = Some(storage);
    let retry = f.capture();
    assert!(retry.predecessor.is_none());
    let done = retry
        .persist(note(), Some(destination()), true)
        .await
        .unwrap();
    let stored = saved(result(&done).await);
    assert_eq!(stored.token, expected.token);
    assert_eq!(stored.revision, expected.revision + 1);
    assert!(
        f.queue()
            .retained()
            .unwrap()
            .iter()
            .any(|t| t.same(&failed))
    );
}
#[tokio::test]
async fn source_and_account_retirement_do_not_drop_old_captured_save_or_relabel_its_visit() {
    let mut f = Fixture::new().await;
    let capture = f.capture();
    let visit = capture.visit.clone();
    let storage = f.storage();
    let scope = f.draft().scope;
    let (mut away, _) = sample();
    away.core.active_account = Some("other".into());
    carry_pending(&mut f.shell, &mut away);
    let ticket = capture
        .persist(note(), Some(destination()), true)
        .await
        .unwrap();
    let stored = saved(result(&ticket).await);
    assert!(Arc::ptr_eq(ticket.visit(), &visit));
    assert!(storage.run(move |j| j.draft(&scope)).await.unwrap() == Some(stored));
    assert!(Arc::ptr_eq(
        &away.file_share.custody,
        &f.shell.file_share.custody
    ));
}
#[tokio::test]
async fn published_empty_claim_transfers_only_its_own_slot_before_exact_cleanup_at_capacity() {
    let temp = Temp::new();
    let (mut shell, target) = sample();
    let source = admission::Source::capture(&shell, &target).unwrap();
    let storage = recovery::Storage::at(source.context.key.clone(), temp.0.join("draft.sqlite"));
    let load =
        durable::open::prepare_captured(&mut shell, source, "source".into(), Ok(storage.clone()))
            .unwrap();
    let Outcome::Claimed { draft, .. } = load.outcome().await else {
        panic!("published claim")
    };
    let ticket = visible(&shell).unwrap().draft_task.clone().unwrap();
    assert!(!ticket.cancel_load());
    assert!(durable::pending_claim(&ticket) == Some(&draft));
    let queue = shell.file_share.custody.lock().unwrap().draft_io.clone();
    let visit = ticket.visit().clone();
    let mut others = vec![];
    for _ in 0..31 {
        let t = submit(
            &queue,
            storage.clone(),
            visit.clone(),
            Command::Retain {
                content: Content {
                    note: note(),
                    ..draft.content.clone()
                },
                reason: Refusal::Storage,
            },
        );
        assert!(matches!(
            result(&t).await,
            Outcome::Failed(Refusal::Storage)
        ));
        others.push(t);
    }
    assert!(matches!(
        queue.reserve(storage.clone(), visit),
        Err(Refusal::Capacity)
    ));
    close(&mut shell, &load.id);
    shell.file_share.recovery.open = true;
    shell.file_share.recovery.serial = 1;
    shell.file_share.recovery.drafts = vec![draft.clone()];
    shell.overlay = Some(Overlay::FileShare);
    let review = durable::saved::review(&mut shell, 1, &draft).unwrap();
    assert!(!visible(&shell).unwrap().draft_local);
    assert!(
        visible(&shell)
            .unwrap()
            .draft_task
            .as_ref()
            .unwrap()
            .same(&ticket)
    );
    let (_, _, Some((q, cleanup, _))) =
        durable::saved::prepare_discard(&mut shell, &review).unwrap()
    else {
        panic!("cleanup")
    };
    assert!(matches!(result(&cleanup).await, Outcome::Discarded));
    assert!(q.retained().unwrap().iter().all(|t| !t.same(&ticket)));
    for t in others {
        assert!(q.retained().unwrap().iter().any(|r| r.same(&t)));
    }
    let scope = draft.scope;
    assert!(
        storage
            .run(move |j| j.draft(&scope))
            .await
            .unwrap()
            .is_none()
    );
}
#[tokio::test]
async fn readonly_discard_capacity_refusal_keeps_exact_version_for_explicit_retry() {
    let mut f = Fixture::new().await;
    let expected = f.draft();
    let storage = f.storage();
    let queue = f.queue();
    let visit = visible(&f.shell).unwrap().draft_visit.clone().unwrap();
    let mut other = vec![];
    for _ in 0..32 {
        let t = submit(
            &queue,
            storage.clone(),
            visit.clone(),
            Command::Retain {
                content: Content {
                    note: note(),
                    ..expected.content.clone()
                },
                reason: Refusal::Storage,
            },
        );
        result(&t).await;
        other.push(t);
    }
    f.recovery(vec![expected.clone()], vec![]);
    let serial = f.shell.file_share.recovery.serial;
    let id = durable::saved::review(&mut f.shell, serial, &expected).unwrap();
    // Keep the actual review/identity path, but route its later I/O back to
    // this test's owned database rather than ordinary account-default storage.
    f.shell.file_share.dialog.as_mut().unwrap().draft_storage = Some(storage.clone());
    assert!(matches!(
        durable::saved::prepare_discard(&mut f.shell, &id),
        Err(Refusal::Capacity)
    ));
    assert!(visible(&f.shell).unwrap().durable.as_ref() == Some(&expected));
    assert!(!visible(&f.shell).unwrap().closing);
    assert!(queue.acknowledge(&other[0]).unwrap()); // explicit acknowledgement of this one observed copy
    let (_, _, Some((_, ticket, _))) = durable::saved::prepare_discard(&mut f.shell, &id).unwrap()
    else {
        panic!("retry")
    };
    assert!(matches!(result(&ticket).await, Outcome::Discarded));
    assert_eq!(queue.retained().unwrap().len(), 32);
    let scope = expected.scope;
    assert!(
        storage
            .run(move |j| j.draft(&scope))
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn cancel_before_publication_removes_only_empty_claim_and_retains_destination_only_body() {
    use crate::file_share::draft_io::{Cleanup, GatePoint, TestGate};
    for nonempty in [false, true] {
        let mut f = Fixture::new().await;
        if nonempty {
            let t = f
                .capture()
                .persist(vec![], Some(destination()), true)
                .await
                .unwrap();
            saved(result(&t).await);
        }
        close(&mut f.shell, &f.id);
        let source = admission::Source::capture(&f.shell, &f.target).unwrap();
        let storage =
            recovery::Storage::at(source.context.key.clone(), f.temp.0.join("draft.sqlite"));
        let queue = f.queue();
        let gate = TestGate::new();
        queue.gate_next(GatePoint::AfterIo, gate.clone());
        let load = durable::open::prepare_captured(
            &mut f.shell,
            source,
            "source".into(),
            Ok(storage.clone()),
        )
        .unwrap();
        tokio::time::timeout(Duration::from_secs(5), gate.reached.acquire())
            .await
            .unwrap()
            .unwrap()
            .forget();
        let ticket = visible(&f.shell).unwrap().draft_task.clone().unwrap();
        assert!(ticket.cancel_load());
        gate.release.add_permits(1);
        let outcome = load.outcome().await;
        if nonempty {
            assert!(matches!(
                outcome,
                Outcome::Canceled {
                    cleanup: Cleanup::KeptNonempty,
                    ..
                }
            ));
        } else {
            assert!(matches!(
                outcome,
                Outcome::Canceled {
                    cleanup: Cleanup::Removed,
                    ..
                }
            ));
        }
        let key = storage.key.clone();
        let rows = storage
            .run(move |j| j.drafts(&key.team, &key.user))
            .await
            .unwrap();
        assert_eq!(rows.len(), usize::from(nonempty));
        if nonempty {
            assert!(rows[0].content.destination.is_some());
        }
    }
}

mod clear_capture;
mod reader_failure;

#[tokio::test]
async fn downstream_shell_tests_refuse_implicit_default_storage_before_io() {
    let (shell, _) = sample();
    let context = admission::Context::capture(&shell).unwrap();
    let storage = recovery::Storage::capture(&context).unwrap();
    assert!(matches!(
        storage.run(|_| Ok(())).await,
        Err(Refusal::Storage)
    ));
}

mod pressure;
