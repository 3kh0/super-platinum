use super::*;
use std::time::{Duration, Instant};
pub(super) struct Fixture {
    pub queue: Arc<Queue>,
    pub scope: Scope,
    pub stream: p::Stream,
    pub review: p::Review,
}
pub(super) fn until<T>(mut action: impl FnMut() -> Result<T, Reason>) -> T {
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        match action() {
            Ok(v) => return v,
            Err(Reason::Incomplete) => {
                assert!(Instant::now() < end, "actual worker deadline");
                std::thread::yield_now()
            }
            Err(e) => panic!("unexpected refusal {e:?}"),
        }
    }
}
pub(super) fn wait<I, T>(step: &Step<I, T>) {
    until(|| match step.state() {
        StepState::Ready => Ok(()),
        StepState::Waiting => Err(Reason::Incomplete),
        other => panic!("actual issuance {other:?}"),
    });
}
pub(super) fn call<I: Send + Sync + 'static, T: Send + Clone + 'static>(
    queue: &Queue,
    step: Step<I, T>,
) -> T {
    queue.worker.submit_step(&step).unwrap();
    wait(&step);
    step.value().unwrap()
}
pub(super) fn finish(queue: &Queue, c: &Completion, raw: Raw) -> Arc<Finished> {
    queue.worker.bind_raw(c, raw).unwrap();
    queue.worker.submit_completion(c).unwrap();
    finished(c)
}
pub(super) fn finished(c: &Completion) -> Arc<Finished> {
    until(|| c.result()?.ok_or(Reason::Incomplete))
}
impl Fixture {
    pub(super) fn new() -> Self {
        Self::seed(false, false)
    }
    pub(super) fn for_status_check() -> Self {
        Self::seed(true, false)
    }
    pub(super) fn for_slash() -> Self {
        Self::seed(false, true)
    }
    fn seed(known_base: bool, slash: bool) -> Self {
        let queue = Queue::fresh().unwrap();
        let scope = Scope {
            account_id: queue.account().into(),
            key: DraftKey {
                team: "T1".into(),
                channel: "C1".into(),
                thread: None,
            },
        };
        // Canonical input precedes the actual inspect/consent/migrate/open/cold
        // route. No fake State.cold_ready and no unsupported old Client call.
        let canonical = sync::tests::local("retained \u{65e5}\u{672c}\u{8a9e} \u{1f680}");
        let mut prepared_operation = None;
        {
            let mut store = DraftStore::open(queue.owner.path()).unwrap();
            store
                .save([(scope.key.clone(), canonical.clone())])
                .unwrap();
            if slash {
                p::slash_runtime::fixture_seed(&mut store, &scope.account_id);
            }
            if known_base {
                // Recover an actual pre-existing Update after restart. Migrated
                // remote rows cannot use local-origin verification; no new
                // dispatch authority is inferred from this historical operation.
                let remote = crate::drafts::sync::RemoteDraft {
                    scope: scope.clone(),
                    id: "D1".into(),
                    client_msg_id: uuid::Uuid::new_v4().to_string(),
                    last_updated_ts: "10.000001".into(),
                    snapshot: crate::drafts::codec::encode(&scope.key, "acknowledged base")
                        .unwrap(),
                };
                let mut state =
                    crate::drafts::sync::DraftSync::adopt_remote(&scope, remote).unwrap();
                let desired =
                    crate::drafts::codec::encode_composer(&scope.key, &canonical.composer).unwrap();
                assert!(state.edit(&scope, Some(desired)));
                let sync::Commit::Applied(stored) = store
                    .commit_synced(&scope, None, &canonical, &state, None)
                    .unwrap()
                else {
                    panic!("exact ordinary enrollment");
                };
                let sync::Prepare::Prepared { stored, operation } =
                    store.prepare_synced(&scope, stored.revision).unwrap()
                else {
                    panic!("actual durable Update preparation");
                };
                assert_eq!(operation.kind, crate::drafts::sync::OperationKind::Update);
                assert_eq!(operation.base.as_ref().unwrap().id, "D1");
                assert_eq!(stored.state.pending(), Some(&operation));
                assert!(
                    matches!(stored.state.workflow(), crate::drafts::sync::Workflow::InFlight(actual) if actual == &operation)
                );
                prepared_operation = Some(operation);
            }
        }
        fn inspect(queue: &Queue) -> Arc<upgrade::Review> {
            let reply = queue
                .submit_upgrade(upgrade::Action::Inspect)
                .unwrap()
                .wait()
                .unwrap();
            match &*reply {
                upgrade::Reply::Inspected(r) => r.clone(),
                _ => panic!("inspect"),
            }
        }
        {
            let inspected = inspect(&queue);
            if slash {
                // The Slash fixture seeded an existing complete protected store.
                // Protected7 is inspection-only: committing activation again is
                // correctly rejected. Reopen via the actual inspected handoff.
                assert_eq!(inspected.class(), upgrade::Class::Protected7);
            } else {
                queue
                    .submit_upgrade(upgrade::Action::Commit(inspected))
                    .unwrap()
                    .wait()
                    .unwrap();
            }
        }
        queue.open(inspect(&queue)).unwrap();
        let loaded = queue.cold(&Arc::new(())).unwrap().wait().unwrap();
        let review = if known_base {
            let review = loaded.managed()[&scope.key].clone();
            let operation = prepared_operation.as_ref().unwrap();
            // Full immutable operation (ID/base/snapshot included) survives the
            // actual migration and once-only cold normalization unchanged.
            assert_eq!(review.row().state().pending(), Some(operation));
            assert!(
                matches!(review.row().state().workflow(), crate::drafts::sync::Workflow::Uncertain(actual) if actual == operation)
            );
            assert_eq!(
                serde_json::to_value(review.local()).unwrap(),
                serde_json::to_value(&canonical).unwrap()
            );
            assert_eq!(
                review.row().state().desired(),
                Some(
                    &crate::drafts::codec::encode_composer(&scope.key, &canonical.composer)
                        .unwrap()
                )
            );
            assert_eq!(
                review.row().state().acknowledged().unwrap().snapshot,
                crate::drafts::codec::encode(&scope.key, "acknowledged base").unwrap()
            );
            review
        } else {
            let local = loaded.locals()[&scope.key].clone();
            call(&queue, queue.worker.hold_enroll(local).unwrap())
        };
        let state = call(
            &queue,
            queue
                .worker
                .hold_stream_state(TeamInput {
                    scope: scope.clone(),
                })
                .unwrap(),
        );
        let stream = call(
            &queue,
            queue
                .worker
                .hold_open_stream(OpenInput {
                    scope: scope.clone(),
                    expected: state,
                })
                .unwrap(),
        );
        let review = if known_base {
            review
        } else {
            call(
                &queue,
                queue
                    .worker
                    .hold_verify(MutationInput {
                        stream: stream.clone(),
                        expected: review,
                    })
                    .unwrap(),
            )
        };
        Self {
            queue,
            scope,
            stream,
            review,
        }
    }
    pub(super) fn prepared(&self) -> Prepared {
        let step = self
            .queue
            .worker
            .hold_prepare(MutationInput {
                stream: self.stream.clone(),
                expected: self.review.clone(),
            })
            .unwrap();
        self.queue.worker.submit_step(&step).unwrap();
        wait(&step);
        step
    }
    pub(super) fn claimed(&self) -> Claimed {
        let prepared = self.prepared();
        let claimed = until(|| self.queue.worker.claim_mutation(&prepared));
        self.queue.worker.submit_step(&claimed).unwrap();
        wait(&claimed);
        claimed
    }
    pub(super) fn mutation(&self) -> (Completion, crate::drafts::sync::Operation) {
        let claimed = self.claimed();
        let completion = until(|| self.queue.worker.begin_record(&claimed));
        let Dispatch::Mutation(operation) = self.queue.worker.take_dispatch(&completion).unwrap()
        else {
            panic!("mutation")
        };
        let (operation, _) = operation.into_parts();
        (completion, operation)
    }
    pub(super) fn remote(&self) -> (p::Review, crate::drafts::sync::RemoteDraft) {
        let (completion, operation) = self.mutation();
        let remote = crate::drafts::sync::RemoteDraft {
            scope: self.scope.clone(),
            id: "D1".into(),
            client_msg_id: operation.client_msg_id.clone(),
            last_updated_ts: "10.000001".into(),
            snapshot: operation.snapshot.unwrap(),
        };
        let result = finish(
            &self.queue,
            &completion,
            Raw::Mutation {
                outcome: p::Outcome::Active(remote.clone()),
                known_until: 0,
            },
        );
        let Finished::Mutation(Ok(review)) = &*result else {
            panic!("positive mutation")
        };
        (review.clone(), remote)
    }
    pub(super) fn current(&self) -> p::Review {
        call(
            &self.queue,
            self.queue
                .worker
                .hold_managed_review(self.scope.clone())
                .unwrap(),
        )
    }
    pub(super) fn sql(&self, sql: &str) {
        rusqlite::Connection::open(self.queue.owner.path())
            .unwrap()
            .execute_batch(sql)
            .unwrap()
    }
    pub(super) fn baseline(&self) -> Completion {
        let (review, _) = self.remote();
        let reserved = self
            .queue
            .worker
            .hold_baseline(crate::drafts::store::worker::admission::BaselineInput {
                stream: self.stream.clone(),
                expected: review,
                now_ms: 1000,
            })
            .unwrap();
        self.queue.worker.submit_step(&reserved).unwrap();
        wait(&reserved);
        let claimed = until(|| self.queue.worker.claim_baseline(&reserved));
        self.queue.worker.submit_step(&claimed).unwrap();
        wait(&claimed);
        let c = until(|| self.queue.worker.begin_baseline_completion(&claimed));
        assert!(matches!(
            self.queue.worker.take_dispatch(&c),
            Ok(Dispatch::Baseline(_))
        ));
        c
    }
    pub(super) fn check_reserved(&self) -> ReservedCheck {
        let review = self.current();
        let operation = review.row().state().pending().unwrap();
        assert_eq!(operation.kind, crate::drafts::sync::OperationKind::Update);
        assert_eq!(operation.base.as_ref().unwrap().id, "D1");
        assert!(
            matches!(review.row().state().workflow(), crate::drafts::sync::Workflow::Uncertain(actual) if actual == operation)
        );
        let receipt = call(
            &self.queue,
            self.queue
                .worker
                .hold_pending_receipt(review.clone())
                .unwrap(),
        );
        let state = call(
            &self.queue,
            self.queue
                .worker
                .hold_recovery_state(TeamInput {
                    scope: self.scope.clone(),
                })
                .unwrap(),
        );
        let recovery = call(
            &self.queue,
            self.queue
                .worker
                .hold_takeover(TakeoverInput {
                    scope: self.scope.clone(),
                    expected: state,
                })
                .unwrap(),
        );
        let reserved = self
            .queue
            .worker
            .hold_check(CheckInput {
                recovery,
                expected: review.clone(),
                receipt,
                now_ms: 1000,
            })
            .unwrap();
        self.queue.worker.submit_step(&reserved).unwrap();
        wait(&reserved);
        reserved
    }
    pub(super) fn check(&self) -> Completion {
        let reserved = self.check_reserved();
        let claimed = until(|| self.queue.worker.claim_check(&reserved));
        self.queue.worker.submit_step(&claimed).unwrap();
        wait(&claimed);
        let c = until(|| self.queue.worker.begin_check_completion(&claimed));
        assert!(matches!(
            self.queue.worker.take_dispatch(&c),
            Ok(Dispatch::Check(_))
        ));
        c
    }
    pub(super) fn observation(&self) -> (Completion, crate::drafts::sync::DraftObservation) {
        let (review, mut remote) = self.remote();
        remote.last_updated_ts = "11.000001".into();
        let event = crate::drafts::sync::DraftObservation {
            scope: self.scope.clone(),
            draft_id: remote.id.clone(),
            client_msg_id: remote.client_msg_id.clone(),
            event_ts: "11.000002".into(),
            kind: crate::drafts::sync::ObservationKind::Active(remote),
        };
        let reserved = self
            .queue
            .worker
            .hold_observation(crate::drafts::store::worker::admission::ObservationInput {
                stream: self.stream.clone(),
                expected: review,
                event: event.clone(),
            })
            .unwrap();
        self.queue.worker.submit_step(&reserved).unwrap();
        wait(&reserved);
        (
            until(|| self.queue.worker.begin_observation_completion(&reserved)),
            event,
        )
    }
}
