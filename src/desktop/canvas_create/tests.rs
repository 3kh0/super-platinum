use super::*;
use domain::{Content, Delivery, Draft, Owner, Store};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
};
use tokio::sync::oneshot;

#[tokio::test(flavor = "current_thread")]
async fn already_saved_create_refusal_and_pre_poll_cancel_keep_exact_base_without_slot_or_conflict_leak()
 {
    tokio::task::LocalSet::new()
        .run_until(async {
            let temp = Temp::new();
            let custody = Custody::default();
            let base = temp.draft();
            let refused = custody.reserve_saved(temp.storage(), base.clone()).unwrap();
            assert!(refused.create(Fake::new(&temp.owner, Some(1))).is_err());
            assert!(matches!(refused.phase(),Phase::Saved(ref saved) if saved==&base));
            assert!(custody.acknowledge(&refused));
            let canceled = custody.reserve_saved(temp.storage(), base.clone()).unwrap();
            let source = Fake::new(&temp.owner, None);
            let calls = source.requests.clone();
            canceled.create(source).unwrap();
            canceled.cancel_before_admission();
            let phase = wait(&canceled, |p| matches!(p, Phase::Saved(_))).await;
            assert!(matches!(phase,Phase::Saved(ref saved) if saved==&base));
            assert!(calls.borrow().is_empty());
            assert!(custody.acknowledge(&canceled));
            assert!(custody.tickets(&temp.owner).is_empty());
            let after = temp
                .store()
                .save(&base, &id(), text("edited after canceled Create"))
                .unwrap();
            assert_eq!(after.revision(), base.revision() + 1);
            assert!(temp.store().conflicts(&temp.owner).unwrap().is_empty());
            assert!(temp.store().active(&temp.owner).unwrap().unwrap() == after);
        })
        .await;
}

struct Temp {
    dir: PathBuf,
    owner: Owner,
}
impl Temp {
    fn new() -> Self {
        let dir =
            std::env::temp_dir().join(format!("sp-canvas-adapter-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        Self {
            dir,
            owner: Owner::new(&id(), "T00000001", "U00000001").unwrap(),
        }
    }
    fn store(&self) -> Store {
        Store::open(self.dir.join("canvas.sqlite"), self.owner.account()).unwrap()
    }
    fn storage(&self) -> Storage {
        Storage::isolated(self.dir.join("canvas.sqlite"), self.owner.clone()).unwrap()
    }
    fn draft(&self) -> Draft {
        self.store()
            .create(&self.owner, &id(), text("Before"))
            .unwrap()
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        // This constructor exclusively creates a random child of the test temp
        // root. No caller-provided/config/default path can reach this cleanup.
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn text(title: &str) -> Content {
    Content::new(
        format!(" {title} "),
        "\u{4e2d}\u{1f642}e\u{301}\n  unchanged markdown  ".into(),
    )
    .unwrap()
}
struct Fake {
    owner: Owner,
    client: SlackClient,
    workspace: WorkspaceSession,
    checks: Cell<usize>,
    deny_at: Option<usize>,
    fault_on_check: Option<(usize, Storage, bool)>,
    requests: Rc<RefCell<Vec<PreparedRequest>>>,
    response: RefCell<Option<oneshot::Receiver<Result<serde_json::Value, Error>>>>,
}
impl Fake {
    fn new(owner: &Owner, deny_at: Option<usize>) -> Self {
        Self {
            owner: owner.clone(),
            client: SlackClient::default(),
            workspace: WorkspaceSession {
                team_id: owner.team().into(),
                user_id: owner.user().into(),
                enterprise_id: None,
                name: "Isolated fixture".into(),
                url: "https://fixture.slack.com".into(),
                token: "synthetic-only".into(),
            },
            checks: Cell::new(0),
            deny_at,
            fault_on_check: None,
            requests: Rc::default(),
            response: RefCell::new(None),
        }
    }
}
impl Source for Fake {
    fn owner(&self) -> &Owner {
        &self.owner
    }
    fn current(&self) -> bool {
        let checks = self.checks.get() + 1;
        self.checks.set(checks);
        if let Some((at, storage, after)) = &self.fault_on_check {
            if checks == *at {
                storage.fail_next(*after);
            }
        }
        self.deny_at.is_none_or(|deny| checks < deny)
    }
    fn client(&self) -> &SlackClient {
        &self.client
    }
    fn workspace(&self) -> &WorkspaceSession {
        &self.workspace
    }
    fn execute(
        &self,
        request: PreparedRequest,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, Error>> + '_>> {
        self.requests.borrow_mut().push(request);
        let response = self.response.borrow_mut().take();
        Box::pin(async move {
            match response {
                Some(receiver) => receiver
                    .await
                    .unwrap_or_else(|_| Err(Error::Offline("synthetic closed reply".into()))),
                None => Ok(serde_json::json!({"ok":true,"file_id":"FCREATED"})),
            }
        })
    }
}
async fn wait(ticket: &Ticket, matches: impl Fn(&Phase) -> bool) -> Phase {
    let mut changes = ticket.changes();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let phase = ticket.phase();
            if matches(&phase) {
                return phase;
            }
            changes.changed().await.unwrap();
        }
    })
    .await
    .expect("bounded synthetic worker completion")
}
async fn wait_request(ticket: &Ticket, calls: &Rc<RefCell<Vec<PreparedRequest>>>) {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while calls.borrow().is_empty() {
            assert!(
                !matches!(ticket.phase(), Phase::Held(_)),
                "worker refused before executor"
            );
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("bounded executor admission");
}
async fn saved(custody: &Custody, temp: &Temp) -> Ticket {
    let ticket = custody.reserve(temp.storage(), temp.draft()).unwrap();
    assert!(ticket.save(text("Exact title")).is_ok());
    wait(&ticket, |p| matches!(p, Phase::Saved(_))).await;
    ticket
}

#[tokio::test(flavor = "current_thread")]
async fn actual_builder_runs_once_only_after_durable_started_and_retains_exact_result() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let temp = Temp::new();
            let custody = Custody::default();
            let ticket = saved(&custody, &temp).await;
            let fake = Fake::new(&temp.owner, None);
            let requests = fake.requests.clone();
            let (send, receive) = oneshot::channel();
            *fake.response.borrow_mut() = Some(receive);
            ticket.create(fake).unwrap();
            wait_request(&ticket, &requests).await;
            let request = requests.borrow()[0].clone();
            let active = temp.store().active(&temp.owner).unwrap().unwrap();
            let attempt = temp
                .store()
                .attempt(&temp.owner, active.attempt_id().unwrap())
                .unwrap()
                .unwrap();
            assert!(matches!(attempt.delivery(), Delivery::Started));
            let super_platinum_core::slack::client::RequestBody::Form(form) = request.body else {
                panic!("form");
            };
            assert_eq!(request.method, "POST");
            assert_eq!(
                request.url.split('?').next(),
                Some("https://fixture.slack.com/api/files.createCanvas")
            );
            assert_eq!(
                form.iter().find(|(k, _)| k == "title").unwrap().1,
                "Exact title"
            );
            assert_eq!(
                form.iter().find(|(k, _)| k == "markdown").unwrap().1,
                text("Exact title").markdown()
            );
            assert!(
                !form
                    .iter()
                    .any(|(k, _)| ["channel_id", "idempotency_key"].contains(&k.as_str()))
            );
            assert!(ticket.create(Fake::new(&temp.owner, None)).is_err());
            send.send(Ok(serde_json::json!({"ok":true,"file_id":"FCREATED"})))
                .unwrap();
            let Phase::Settled(done) = wait(&ticket, |p| matches!(p, Phase::Settled(_))).await
            else {
                unreachable!()
            };
            assert!(matches!(done.delivery(), Delivery::Created{file_id} if file_id == "FCREATED"));
            assert_eq!(requests.borrow().len(), 1);
            assert!(custody.acknowledge(&ticket));
            assert!(ticket.create(Fake::new(&temp.owner, None)).is_err());
            assert!(
                temp.store()
                    .attempt(&temp.owner, done.id())
                    .unwrap()
                    .as_ref()
                    .is_some_and(|a| a == &done)
            );
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn stale_before_admission_is_canceled_but_stale_after_started_is_uncertain() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for deny_at in [3, 4] {
                let temp = Temp::new();
                let custody = Custody::default();
                let ticket = saved(&custody, &temp).await;
                let source = Fake::new(&temp.owner, Some(deny_at));
                let calls = source.requests.clone();
                ticket.create(source).unwrap();
                let phase = wait(&ticket, |p| {
                    matches!(p, Phase::Canceled(_) | Phase::Settled(_))
                })
                .await;
                match (deny_at, phase) {
                    (3, Phase::Canceled(attempt)) => assert!(matches!(
                        attempt.delivery(),
                        Delivery::CanceledBeforeAdmission
                    )),
                    (4, Phase::Settled(attempt)) => {
                        assert!(matches!(attempt.delivery(), Delivery::Uncertain))
                    }
                    _ => panic!("wrong dispatch boundary classification"),
                }
                assert!(calls.borrow().is_empty());
            }
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn deleted_base_late_capture_is_durable_conflict_not_new_sendable_draft() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let temp = Temp::new();
            let custody = Custody::default();
            let base = temp.draft();
            let ticket = custody.reserve(temp.storage(), base.clone()).unwrap();
            temp.store().discard(&base).unwrap();
            assert!(ticket.save(text("Captured after leave")).is_ok());
            let Phase::Conflict(conflict) =
                wait(&ticket, |p| matches!(p, Phase::Conflict(_))).await
            else {
                unreachable!()
            };
            assert!(conflict.content() == &text("Captured after leave"));
            assert!(temp.store().active(&temp.owner).unwrap().is_none());
            assert!(ticket.create(Fake::new(&temp.owner, None)).is_err());
            assert!(custody.acknowledge(&ticket));
            assert_eq!(temp.store().conflicts(&temp.owner).unwrap().len(), 1);
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn late_created_after_archive_settles_original_without_changing_new_draft() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let temp = Temp::new();
            let custody = Custody::default();
            let ticket = saved(&custody, &temp).await;
            let fake = Fake::new(&temp.owner, None);
            let calls = fake.requests.clone();
            let (send, receive) = oneshot::channel();
            *fake.response.borrow_mut() = Some(receive);
            ticket.create(fake).unwrap();
            wait_request(&ticket, &calls).await;
            let mut store = temp.store();
            let old = store.active(&temp.owner).unwrap().unwrap();
            let started = store
                .attempt(&temp.owner, old.attempt_id().unwrap())
                .unwrap()
                .unwrap();
            let unknown = store.settle(&started, domain::Outcome::Uncertain).unwrap();
            store
                .archive_and_new(
                    &old,
                    &unknown,
                    domain::ArchiveReason::AbandonedOutcomeUnknown,
                    &id(),
                    &id(),
                    now().unwrap(),
                )
                .unwrap();
            let new = store.active(&temp.owner).unwrap().unwrap();
            send.send(Ok(serde_json::json!({"ok":true,"file_id":"FLATE"})))
                .unwrap();
            let Phase::Settled(done) = wait(&ticket, |p| matches!(p, Phase::Settled(_))).await
            else {
                unreachable!()
            };
            assert_eq!(done.id(), started.id());
            assert!(matches!(done.delivery(), Delivery::Created{file_id} if file_id == "FLATE"));
            assert!(
                store
                    .active(&temp.owner)
                    .unwrap()
                    .as_ref()
                    .is_some_and(|a| a == &new)
            );
            assert_eq!(calls.borrow().len(), 1);
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn observer_cancellation_does_not_cancel_save_or_release_root_custody() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let temp = Temp::new();
            let custody = Custody::default();
            let carried = custody.clone();
            let ticket = custody.reserve(temp.storage(), temp.draft()).unwrap();
            assert!(ticket.save(text("Keep after observer drops")).is_ok());
            let observed = ticket.clone();
            let waiter = tokio::task::spawn_local(async move {
                wait(&observed, |p| matches!(p, Phase::Saved(_))).await;
            });
            waiter.abort();
            drop(custody);
            wait(&ticket, |p| matches!(p, Phase::Saved(_))).await;
            assert!(
                temp.store().active(&temp.owner).unwrap().unwrap().content()
                    == &text("Keep after observer drops")
            );
            assert!(carried.acknowledge(&ticket));
        })
        .await;
}

#[test]
fn root_capacity_is_reserved_before_capture_and_retired_ticket_cannot_accept_input() {
    fn send_type<T: Send>() {}
    send_type::<Custody>();
    send_type::<Ticket>();
    let temp = Temp::new();
    let custody = Custody::default();
    let base = temp.draft();
    let tickets: Vec<_> = (0..custody::LIMIT)
        .map(|_| custody.reserve(temp.storage(), base.clone()).unwrap())
        .collect();
    assert!(matches!(
        custody.reserve(temp.storage(), base),
        Err(domain::Fault::Capacity)
    ));
    assert!(custody.abandon_empty(&tickets[0]));
    let captured = text("Returned to caller");
    let Err((_, returned)) = tickets[0].save(captured.clone()) else {
        panic!("retired ticket");
    };
    assert!(returned == captured);
}

#[tokio::test(flavor = "current_thread")]
async fn cancellation_before_first_poll_allows_only_a_new_explicit_create() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let temp = Temp::new();
            let custody = Custody::default();
            let ticket = saved(&custody, &temp).await;
            let first = Fake::new(&temp.owner, None);
            let first_calls = first.requests.clone();
            ticket.create(first).unwrap();
            ticket.cancel_before_admission();
            wait(&ticket, |p| matches!(p, Phase::Saved(_))).await;
            assert!(first_calls.borrow().is_empty());
            assert!(
                temp.store()
                    .active(&temp.owner)
                    .unwrap()
                    .unwrap()
                    .attempt_id()
                    .is_none()
            );
            let second = Fake::new(&temp.owner, None);
            let second_calls = second.requests.clone();
            ticket.create(second).unwrap();
            wait(&ticket, |p| matches!(p, Phase::Settled(_))).await;
            assert_eq!(second_calls.borrow().len(), 1);
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn save_storage_retry_preserves_exact_operation_and_recovers_a_lost_reply() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for after_commit in [false, true] {
                let temp = Temp::new();
                let custody = Custody::default();
                let base = temp.draft();
                let ticket = custody.reserve(temp.storage(), base.clone()).unwrap();
                ticket.job.storage.fail_next(after_commit);
                assert!(ticket.save(text("Storage retry")).is_ok());
                wait(&ticket, |p| matches!(p, Phase::Held(_))).await;
                assert!(!custody.acknowledge(&ticket));
                let wrong = Owner::new(&id(), "T00000001", "U00000001").unwrap();
                assert!(custody.tickets(&wrong).is_empty());
                let recovered = custody.tickets(&temp.owner).pop().unwrap();
                recovered.retry_storage().unwrap();
                let Phase::Saved(saved) = wait(&recovered, |p| matches!(p, Phase::Saved(_))).await
                else {
                    unreachable!()
                };
                assert_eq!(saved.revision(), base.revision() + 1);
                assert!(saved.content() == &text("Storage retry"));
                assert!(temp.store().conflicts(&temp.owner).unwrap().is_empty());
                assert!(custody.acknowledge(&recovered));
            }
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn reserve_admit_cancel_and_settle_fault_recovery_never_replays_executor() {
    tokio::task::LocalSet::new()
        .run_until(async {
            // call2 is before reserve, call3 before admit/cancel, call4 before execute.
            for (at, after, deny, expected_calls) in [
                (2, true, None, 0),
                (3, true, None, 0),
                (3, false, Some(3), 0),
                (4, true, None, 1),
            ] {
                let temp = Temp::new();
                let custody = Custody::default();
                let ticket = saved(&custody, &temp).await;
                let mut source = Fake::new(&temp.owner, deny);
                source.fault_on_check = Some((at, ticket.job.storage.clone(), after));
                let calls = source.requests.clone();
                ticket.create(source).unwrap();
                wait(&ticket, |p| matches!(p, Phase::Held(_))).await;
                assert!(!custody.acknowledge(&ticket));
                ticket.retry_storage().unwrap();
                let phase = wait(&ticket, |p| {
                    matches!(
                        p,
                        Phase::Review { .. } | Phase::Settled(_) | Phase::Canceled(_)
                    )
                })
                .await;
                match (at, deny, phase) {
                    (
                        2,
                        _,
                        Phase::Review {
                            attempt: Some(attempt),
                            ..
                        },
                    ) => assert!(matches!(attempt.delivery(), Delivery::Reserved)),
                    (3, None, Phase::Settled(attempt)) => {
                        assert!(matches!(attempt.delivery(), Delivery::Uncertain))
                    }
                    (3, Some(3), Phase::Canceled(attempt)) => assert!(matches!(
                        attempt.delivery(),
                        Delivery::CanceledBeforeAdmission
                    )),
                    (4, _, Phase::Settled(attempt)) => {
                        assert!(matches!(attempt.delivery(), Delivery::Created { .. }))
                    }
                    _ => panic!("wrong exact recovery"),
                }
                assert_eq!(calls.borrow().len(), expected_calls);
                assert!(ticket.create(Fake::new(&temp.owner, None)).is_err());
                assert!(custody.acknowledge(&ticket));
            }
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn failed_reservation_then_competing_edit_preserves_original_capture_for_review() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let temp = Temp::new();
            let custody = Custody::default();
            let ticket = saved(&custody, &temp).await;
            let source = Fake::new(&temp.owner, None);
            let calls = source.requests.clone();
            ticket.job.storage.fail_next(false);
            ticket.create(source).unwrap();
            wait(&ticket, |p| matches!(p, Phase::Held(_))).await;
            let mut store = temp.store();
            let old = store.active(&temp.owner).unwrap().unwrap();
            let newer = store.save(&old, &id(), text("Different writer")).unwrap();
            ticket.retry_storage().unwrap();
            let Phase::Conflict(conflict) =
                wait(&ticket, |p| matches!(p, Phase::Conflict(_))).await
            else {
                unreachable!()
            };
            assert!(conflict.content() == &text("Exact title"));
            assert!(
                store
                    .active(&temp.owner)
                    .unwrap()
                    .as_ref()
                    .is_some_and(|d| d == &newer)
            );
            assert!(calls.borrow().is_empty());
            assert!(custody.acknowledge(&ticket));
        })
        .await;
}
