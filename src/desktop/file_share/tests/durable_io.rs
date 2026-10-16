use super::*;
use crate::file_share::draft_io::{
    Cleanup, Command, GatePoint, Outcome, Queue, Reservation, TestGate, Ticket,
};
use super_platinum_core::file_sharing::store::drafts::{
    Claim, Content, Destination, Draft, DraftScope, Mutation,
};

struct Fixture {
    temp: Temp,
    shell: ShellState,
    source: admission::Source,
    storage: recovery::Storage,
    visit: Arc<drafts::Visit>,
    queue: Queue,
}
impl Fixture {
    fn new() -> Self {
        let temp = Temp::new();
        let (shell, target) = sample();
        let source = admission::Source::capture(&shell, &target).unwrap();
        let storage =
            recovery::Storage::at(source.context.key.clone(), temp.0.join("draft.sqlite"));
        let (visit, queue) = {
            let mut custody = shell.file_share.custody.lock().unwrap();
            (custody.visit(&source).unwrap(), custody.draft_io.clone())
        };
        Self {
            temp,
            shell,
            source,
            storage,
            visit,
            queue,
        }
    }
    fn initial(&self) -> Content {
        Content {
            source: self.source.file.clone(),
            title: self.source.title.clone(),
            note: vec![],
            destination: None,
        }
    }
    fn scope(&self) -> DraftScope {
        DraftScope {
            team: self.source.context.key.team.clone(),
            user: self.source.context.key.user.clone(),
            file: self.source.id.clone(),
        }
    }
    fn reserve(&self) -> Reservation {
        self.queue
            .reserve(self.storage.clone(), self.visit.clone())
            .unwrap()
    }
    fn submit(&self, command: Command) -> Ticket {
        submit(self.reserve(), command)
    }
    async fn row(&self) -> Option<Draft> {
        let scope = self.scope();
        self.storage.run(move |j| j.draft(&scope)).await.unwrap()
    }
    async fn claim(&self) -> Draft {
        let ticket = self.submit(Command::LoadClaim {
            initial: self.initial(),
        });
        let draft = match result(&ticket).await {
            Outcome::Claimed {
                draft,
                created: true,
            } => draft,
            _ => panic!("fresh claim"),
        };
        assert!(self.queue.acknowledge(&ticket).unwrap());
        draft
    }
    async fn saved(&self) -> Draft {
        let expected = self.claim().await;
        let mut content = expected.content.clone();
        content.note = note();
        content.destination = Some(destination());
        let ticket = self.submit(Command::Save { expected, content });
        let draft = match result(&ticket).await {
            Outcome::Saved(draft) => draft,
            _ => panic!("save"),
        };
        assert!(self.queue.acknowledge(&ticket).unwrap());
        draft
    }
}
fn destination() -> Destination {
    Destination {
        id: "C00000001".into(),
        name: "ship".into(),
        private: false,
    }
}
fn submit(reservation: Reservation, command: Command) -> Ticket {
    match reservation.submit(command) {
        Ok(ticket) => ticket,
        Err(_) => panic!("bounded submission"),
    }
}
async fn result(ticket: &Ticket) -> Outcome {
    tokio::time::timeout(Duration::from_secs(5), ticket.wait())
        .await
        .expect("worker completes")
}
async fn reached(gate: &TestGate) {
    tokio::time::timeout(Duration::from_secs(5), gate.reached.acquire())
        .await
        .unwrap()
        .unwrap()
        .forget();
}
fn released(gate: &TestGate) {
    gate.release.add_permits(1);
}
fn applied(value: Mutation<Draft>) -> Draft {
    match value {
        Mutation::Applied(d) => d,
        _ => panic!("exact save"),
    }
}

#[tokio::test]
async fn reserve_before_capture_orders_old_save_before_later_claim_and_read() {
    let f = Fixture::new();
    let expected = f.claim().await;
    let reserved = f.reserve();
    let later = f.submit(Command::LoadClaim {
        initial: f.initial(),
    });
    let read = f.submit(Command::Read);
    tokio::task::yield_now().await;
    assert!(later.outcome().is_none() && read.outcome().is_none());
    let mut content = expected.content.clone();
    content.note = note();
    content.destination = Some(destination());
    let old = submit(
        reserved,
        Command::Save {
            expected,
            content: content.clone(),
        },
    );
    let saved = match result(&old).await {
        Outcome::Saved(d) => d,
        _ => panic!("save"),
    };
    let claimed = match result(&later).await {
        Outcome::Claimed {
            draft,
            created: false,
        } => draft,
        _ => panic!("later claim"),
    };
    assert!(
        claimed.content == content
            && claimed.token != saved.token
            && claimed.revision > saved.revision
    );
    assert!(matches!(result(&read).await, Outcome::Read(Some(d)) if d == claimed));
    assert!(f.row().await == Some(claimed));
}

#[tokio::test]
async fn dropping_unsubmitted_front_unblocks_next_and_releases_capacity() {
    let f = Fixture::new();
    let front = f.reserve();
    let ready = f.submit(Command::Read);
    tokio::task::yield_now().await;
    assert!(ready.outcome().is_none());
    drop(front);
    assert!(matches!(result(&ready).await, Outcome::Read(None)));
    assert_eq!(f.queue.retained().unwrap().len(), 1);
    assert!(f.queue.acknowledge(&ready).unwrap());
    assert!(f.queue.retained().unwrap().is_empty());
}

#[tokio::test]
async fn capacity_is_explicit_and_terminal_records_require_observation_then_ack() {
    let f = Fixture::new();
    let mut reservations = Vec::new();
    for _ in 0..32 {
        reservations.push(f.reserve());
    }
    assert!(matches!(
        f.queue.reserve(f.storage.clone(), f.visit.clone()),
        Err(Refusal::Capacity)
    ));
    reservations.pop();
    let front = reservations.remove(0);
    let ticket = submit(front, Command::Read);
    // The remaining reservations keep their own slots but not this ready head.
    assert!(!f.queue.acknowledge(&ticket).unwrap());
    assert!(matches!(result(&ticket).await, Outcome::Read(None)));
    let retained = f.queue.retained().unwrap();
    assert_eq!(retained.len(), 31);
    drop(ticket);
    assert_eq!(f.queue.retained().unwrap().len(), 31);
    assert!(f.queue.acknowledge(&retained[0]).unwrap());
    drop(reservations);
    assert!(f.queue.retained().unwrap().is_empty());
}

#[tokio::test]
async fn acknowledgement_can_remove_record_before_worker_dequeues_without_losing_next_job() {
    let f = Fixture::new();
    let gate = TestGate::new();
    f.queue.gate_next(GatePoint::Published, gate.clone());
    let first = f.submit(Command::Read);
    let next = f.submit(Command::Read);
    reached(&gate).await;
    // Published but unobserved must remain retained.
    assert!(!f.queue.acknowledge(&first).unwrap());
    assert!(matches!(result(&first).await, Outcome::Read(None)));
    assert!(!Queue::default().acknowledge(&first).unwrap());
    assert!(f.queue.acknowledge(&first).unwrap());
    assert!(!f.queue.acknowledge(&first).unwrap());
    released(&gate);
    assert!(matches!(result(&next).await, Outcome::Read(None)));
    assert!(f.queue.acknowledge(&next).unwrap());
    assert!(f.queue.retained().unwrap().is_empty());
}

#[tokio::test]
async fn canceled_waiter_and_replaced_shell_do_not_cancel_captured_storage_write() {
    let mut f = Fixture::new();
    let expected = f.claim().await;
    let mut content = expected.content.clone();
    content.note = note();
    content.destination = Some(destination());
    let gate = TestGate::new();
    f.queue.gate_next(GatePoint::BeforeIo, gate.clone());
    let ticket = f.submit(Command::Save {
        expected,
        content: content.clone(),
    });
    let waiter = tokio::spawn({
        let ticket = ticket.clone();
        async move { ticket.wait().await }
    });
    reached(&gate).await;
    waiter.abort();
    assert!(matches!(waiter.await,Err(error) if error.is_cancelled()));
    drop(ticket);
    let (mut replacement, _) = sample();
    carry_pending(&mut f.shell, &mut replacement);
    replacement.core.active_account = Some("different-account".into());
    f.shell = replacement;
    let recovered_queue = f.shell.file_share.custody.lock().unwrap().draft_io.clone();
    let retained = recovered_queue.retained().unwrap();
    assert_eq!(retained.len(), 1);
    assert!(Arc::ptr_eq(retained[0].visit(), &f.visit));
    assert_eq!(retained[0].visit().key.0.account, ACCOUNT);
    released(&gate);
    let saved = match result(&retained[0]).await {
        Outcome::Saved(d) => d,
        _ => panic!("root save"),
    };
    assert!(saved.content == content && f.row().await == Some(saved));
    assert!(
        matches!(retained[0].input(),Some(Command::Save{content:actual,..}) if actual==content)
    );
    assert!(recovered_queue.acknowledge(&retained[0]).unwrap());
}

#[tokio::test]
async fn lost_waiter_after_actual_commit_retains_exact_completion_without_resave() {
    let f = Fixture::new();
    let expected = f.claim().await;
    let old_revision = expected.revision;
    let mut content = expected.content.clone();
    content.note = note();
    let gate = TestGate::new();
    f.queue.gate_next(GatePoint::AfterIo, gate.clone());
    let ticket = f.submit(Command::Save { expected, content });
    let waiter = tokio::spawn({
        let ticket = ticket.clone();
        async move { ticket.wait().await }
    });
    reached(&gate).await;
    let committed = f.row().await.unwrap();
    assert_eq!(committed.revision, old_revision + 1);
    assert!(ticket.outcome().is_none());
    waiter.abort();
    assert!(matches!(waiter.await,Err(error) if error.is_cancelled()));
    drop(ticket);
    let ticket = f.queue.retained().unwrap().remove(0);
    released(&gate);
    assert!(matches!(result(&ticket).await,Outcome::Saved(d) if d==committed));
    assert!(f.row().await == Some(committed));
    assert!(f.queue.acknowledge(&ticket).unwrap());
}

#[tokio::test]
async fn canceled_empty_load_discards_only_its_exact_claim() {
    let f = Fixture::new();
    let gate = TestGate::new();
    f.queue.gate_next(GatePoint::AfterIo, gate.clone());
    let ticket = f.submit(Command::LoadClaim {
        initial: f.initial(),
    });
    reached(&gate).await;
    let claimed = f.row().await.unwrap();
    assert!(claimed.content.empty());
    assert!(ticket.cancel_load());
    released(&gate);
    assert!(
        matches!(result(&ticket).await,Outcome::Canceled{claim,cleanup:Cleanup::Removed} if claim==claimed)
    );
    assert!(f.row().await.is_none());
    assert!(!ticket.cancel_load());
}

#[tokio::test]
async fn canceled_load_never_discards_note_or_destination_only_input() {
    for destination_only in [false, true] {
        let f = Fixture::new();
        let expected = f.claim().await;
        let mut content = expected.content.clone();
        if destination_only {
            content.destination = Some(destination());
        } else {
            content.note = note();
        }
        let saved = f.submit(Command::Save {
            expected,
            content: content.clone(),
        });
        assert!(matches!(result(&saved).await, Outcome::Saved(_)));
        assert!(f.queue.acknowledge(&saved).unwrap());
        let gate = TestGate::new();
        f.queue.gate_next(GatePoint::AfterIo, gate.clone());
        let ticket = f.submit(Command::LoadClaim {
            initial: f.initial(),
        });
        reached(&gate).await;
        assert!(ticket.cancel_load());
        released(&gate);
        assert!(
            matches!(result(&ticket).await,Outcome::Canceled{claim,cleanup:Cleanup::KeptNonempty} if claim.content==content)
        );
        assert!(f.row().await.unwrap().content == content);
    }
}

#[tokio::test]
async fn canceled_empty_claim_cannot_remove_another_process_newer_claim_or_saved_body() {
    let f = Fixture::new();
    let gate = TestGate::new();
    f.queue.gate_next(GatePoint::AfterIo, gate.clone());
    let ticket = f.submit(Command::LoadClaim {
        initial: f.initial(),
    });
    reached(&gate).await;
    let old = f.row().await.unwrap();
    let scope = f.scope();
    let initial = f.initial();
    let newer = f
        .storage
        .run(move |j| {
            let claimed = match j.claim_draft(scope, Some(&old), initial)? {
                Claim::Claimed(d) => d,
                _ => panic!("external claim"),
            };
            let mut content = claimed.content.clone();
            content.note = note();
            Ok(applied(j.save_draft(&claimed, content)?))
        })
        .await
        .unwrap();
    assert!(ticket.cancel_load());
    released(&gate);
    assert!(
        matches!(result(&ticket).await,Outcome::Canceled{cleanup:Cleanup::Conflict(Some(d)),..} if d==newer)
    );
    assert!(f.row().await == Some(newer));
}

#[tokio::test]
async fn stale_save_conflict_retains_original_capture_and_never_rebases() {
    let f = Fixture::new();
    let old = f.saved().await;
    let scope = f.scope();
    let initial = f.initial();
    let expected = old.clone();
    let newer = f
        .storage
        .run(move |j| {
            Ok(match j.claim_draft(scope, Some(&expected), initial)? {
                Claim::Claimed(d) => d,
                _ => panic!("external claim"),
            })
        })
        .await
        .unwrap();
    let mut content = old.content.clone();
    content.note[0]["future"] = json!(123);
    let ticket = f.submit(Command::Save {
        expected: old.clone(),
        content: content.clone(),
    });
    assert!(matches!(result(&ticket).await,Outcome::Conflict(Some(d)) if d==newer));
    assert!(
        matches!(ticket.input(),Some(Command::Save{expected,content:actual}) if expected==old&&actual==content)
    );
    assert!(f.row().await == Some(newer));
}

#[tokio::test]
async fn retired_visit_refuses_new_reservation_but_keeps_already_captured_write_order() {
    let f = Fixture::new();
    let expected = f.claim().await;
    let reserved = f.reserve();
    let next_visit = f
        .shell
        .file_share
        .custody
        .lock()
        .unwrap()
        .visit(&f.source)
        .unwrap();
    assert!(!f.visit.active());
    assert!(matches!(
        f.queue.reserve(f.storage.clone(), f.visit.clone()),
        Err(Refusal::Changed)
    ));
    let later = submit(
        f.queue.reserve(f.storage.clone(), next_visit).unwrap(),
        Command::LoadClaim {
            initial: f.initial(),
        },
    );
    let mut content = expected.content.clone();
    content.note = note();
    let old = submit(
        reserved,
        Command::Save {
            expected,
            content: content.clone(),
        },
    );
    assert!(matches!(result(&old).await,Outcome::Saved(d) if d.content==content));
    assert!(matches!(result(&later).await,Outcome::Claimed{draft,..} if draft.content==content));
}

#[tokio::test]
async fn different_source_load_returns_saved_review_without_rotating_or_rebinding() {
    let f = Fixture::new();
    let saved = f.saved().await;
    let mut source = f.source.clone();
    source.file["title"] = json!("Changed title");
    source.title = "Changed title".into();
    let visit = f
        .shell
        .file_share
        .custody
        .lock()
        .unwrap()
        .visit(&source)
        .unwrap();
    let mut initial = f.initial();
    initial.source = source.file;
    initial.title = source.title;
    let ticket = submit(
        f.queue.reserve(f.storage.clone(), visit).unwrap(),
        Command::LoadClaim { initial },
    );
    assert!(matches!(result(&ticket).await,Outcome::Review(d) if d==saved));
    assert!(f.row().await == Some(saved));
}

#[tokio::test]
async fn invalid_bounded_binding_and_storage_failure_retain_inputs_without_success() {
    let f = Fixture::new();
    let mut initial = f.initial();
    initial.source["id"] = json!("FOTHER");
    let ticket = f.submit(Command::LoadClaim {
        initial: initial.clone(),
    });
    assert!(matches!(
        result(&ticket).await,
        Outcome::Failed(Refusal::Changed)
    ));
    assert!(matches!(ticket.input(),Some(Command::LoadClaim{initial:actual}) if actual==initial));
    assert!(!f.temp.0.join("draft.sqlite").exists());
    let bad = recovery::Storage::at(f.source.context.key.clone(), f.temp.0.clone());
    let initial = f.initial();
    let failed = submit(
        f.queue.reserve(bad, f.visit.clone()).unwrap(),
        Command::LoadClaim {
            initial: initial.clone(),
        },
    );
    assert!(matches!(
        result(&failed).await,
        Outcome::Failed(Refusal::Storage)
    ));
    assert!(matches!(failed.input(),Some(Command::LoadClaim{initial:actual}) if actual==initial));
    let mut wrong = f.source.context.key.clone();
    wrong.account = "wrong-account".into();
    let storage = recovery::Storage::at(wrong, f.temp.0.join("wrong.sqlite"));
    assert!(matches!(
        f.queue.reserve(storage, f.visit.clone()),
        Err(Refusal::Changed)
    ));
    assert!(!f.temp.0.join("wrong.sqlite").exists());
}

#[tokio::test]
async fn oversized_or_invalid_input_returns_ownership_and_unblocks_following_job() {
    for kind in 0..6 {
        let f = Fixture::new();
        let reservation = f.reserve();
        let next = f.submit(Command::Read);
        let mut initial = f.initial();
        match kind {
            0 => initial.source["oversized"] = json!("s".repeat(256 * 1024)),
            1 => initial.title = "x".repeat(16385),
            2 => {
                initial.note = note();
                initial.note[0]["elements"][0]["elements"][0]["text"] =
                    json!("n".repeat(256 * 1024));
            }
            3 => initial.note = vec![json!({"type":""})],
            4 => {
                initial.destination = Some(Destination {
                    id: "C00000001".into(),
                    name: "x".repeat(16385),
                    private: false,
                })
            }
            _ => {
                let mut deep = json!(null);
                for _ in 0..40 {
                    deep = json!([deep]);
                }
                initial.source["deep"] = deep;
            }
        }
        let rejected = match reservation.submit(Command::LoadClaim {
            initial: initial.clone(),
        }) {
            Err(r) => r,
            Ok(_) => panic!("reject oversized or invalid input"),
        };
        assert!(matches!(
            rejected.reason,
            Refusal::Source | Refusal::Note | Refusal::Destination
        ));
        assert!(matches!(rejected.command,Command::LoadClaim{initial:actual} if actual==initial));
        assert!(matches!(result(&next).await, Outcome::Read(None)));
        assert_eq!(f.queue.retained().unwrap().len(), 1);
        assert!(f.queue.acknowledge(&next).unwrap());
    }
}

#[tokio::test]
async fn actual_linked_unresolved_receipt_blocks_claim_save_and_discard() {
    let f = Fixture::new();
    let saved = f.saved().await;
    let expected = saved.clone();
    let receipt = f
        .storage
        .run(move |j| {
            Ok(match j.begin_from_draft(&expected, 10)? {
                Begin::Started(r) => r,
                _ => panic!("begin"),
            })
        })
        .await
        .unwrap();
    for command in [
        Command::LoadClaim {
            initial: f.initial(),
        },
        Command::Save {
            expected: saved.clone(),
            content: saved.content.clone(),
        },
        Command::Discard {
            expected: saved.clone(),
        },
    ] {
        let ticket = f.submit(command);
        assert!(matches!(result(&ticket).await,Outcome::Blocked(r) if r==receipt));
        assert!(ticket.input().is_some());
        assert!(f.row().await == Some(saved.clone()));
        assert!(f.queue.acknowledge(&ticket).unwrap());
    }
}
