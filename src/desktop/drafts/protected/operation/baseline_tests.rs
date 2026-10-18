//! Actual owned worker/read/choice and original operation retention. Socket
//! acceptance and the final opaque projection observation are explicit unit
//! setup: these tests do not claim browser capture or RendererApplied evidence.
use super::Action;
use super::*;
use crate::drafts::protected::{
    editor::Editor,
    owned_route::{Held, o},
    source::Source,
};
use crate::{
    drafts::{network::Binding, observation},
    state::ShellState,
};
use dioxus::prelude::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use super_platinum_core::{
    config::{Session, WorkspaceSession},
    slack::{
        Transport,
        realtime::{ConnectParams, flannel_url},
    },
};

async fn setup() -> (
    VirtualDom,
    Signal<ShellState>,
    observation::WorkerSource,
    Arc<Editor>,
    Arc<crate::drafts::protected::baseline::Evidence>,
    durable::Stream,
) {
    let (initialized, fixture) = tokio::task::spawn_blocking(|| {
        let (route, fixture) = o::OwnedRoute::fresh_baseline_fixture(
            o::BaselineFixtureTarget::Channel,
            o::BaselineFixtureOutcome::ChangedWithAttachment,
        )
        .unwrap();
        let before = route.inspect().unwrap().wait().unwrap();
        route.commit(&before).unwrap().wait().unwrap();
        let initialized = route
            .inspect()
            .unwrap()
            .wait()
            .unwrap()
            .open()
            .unwrap()
            .initialize_cold(&o::FactoryIdentity::default())
            .unwrap()
            .wait()
            .unwrap();
        (initialized, fixture)
    })
    .await
    .unwrap();
    let ops = initialized.operations();
    let account = Account::from_retained(initialized);
    let owner = account.owner(fixture.scope().key.clone()).unwrap();
    let editor = Editor::open(owner.clone()).await.unwrap();
    let expected = owner.review().await.unwrap();
    assert_eq!(expected.local().attachments.len(), 1);
    let stamp = Held::new(ops.stream_state(o::TeamInput {
        scope: owner.scope().clone(),
    }))
    .observed_success()
    .await
    .unwrap();
    let stream = Held::new(ops.open_stream(o::OpenInput {
        scope: owner.scope().clone(),
        expected: stamp,
    }))
    .observed_success()
    .await
    .unwrap();
    let read_ops = ops.clone();
    let read_stream = stream.clone();
    let evidence = tokio::task::spawn_blocking(move || {
        let reserve = read_ops
            .baseline(o::BaselineInput::new(read_stream, expected, 1000))
            .unwrap();
        reserve.submit().unwrap();
        assert_eq!(reserve.wait().unwrap(), o::JobState::Ready);
        let claim = reserve.claim().unwrap();
        claim.submit().unwrap();
        assert_eq!(claim.wait().unwrap(), o::JobState::Ready);
        let completion = claim.completion().unwrap();
        completion
            .bind(fixture.read(&completion, 1000).unwrap())
            .unwrap();
        completion.submit().unwrap();
        completion.wait().unwrap().unwrap();
        let (result, ack) = completion.observe_success().unwrap();
        let evidence = result.evidence().unwrap();
        assert!(ack.acknowledge().is_ok());
        evidence
    })
    .await
    .unwrap();
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.core.active_account = Some(account.id().into());
    shell.core.account_epoch = 7;
    shell.core.transport = Some(Arc::new(Transport::disconnected_fixture(
        "fixture-only-cookie",
    )));
    shell.core.session = Some(Session {
        unified_user_client_enabled: Default::default(),
        d_cookie: "fixture-only-cookie".into(),
        workspaces: [(
            "T1".into(),
            WorkspaceSession {
                team_id: "T1".into(),
                enterprise_id: None,
                user_id: shell.core.workspaces["T1"].self_user_id.clone(),
                name: "Fixture".into(),
                url: "https://fixture.slack.com".into(),
                token: "fixture-only-token".into(),
            },
        )]
        .into(),
    });
    let mut worker = observation::WorkerSource::capture(
        &mut shell,
        ConnectParams {
            team: "T1".into(),
            ws_url: flannel_url("fixture-only-token", "T1"),
            d_cookie: "fixture-only-cookie".into(),
            user_agent: "fixture-only-agent".into(),
        },
    )
    .unwrap();
    worker.take_connect_params(&shell).unwrap();
    shell.core.workspaces.get_mut("T1").unwrap().rt_generation = 9;
    let socket = Arc::new(worker.accept(&mut shell, "T1", 9).unwrap());
    let source = Source::captured(account, socket, 9, stream.clone(), &shell).unwrap();
    let evidence = Arc::new(crate::drafts::protected::baseline::Evidence {
        evidence,
        binding: Binding::capture(&shell, "T1").unwrap(),
        source,
    });
    type Props = (
        Rc<RefCell<Option<ShellState>>>,
        Rc<Cell<Option<Signal<ShellState>>>>,
    );
    let slot = Rc::new(Cell::new(None));
    let mut dom = VirtualDom::new_with_props(
        |(shell, slot): Props| {
            let state = use_signal(|| shell.borrow_mut().take().unwrap());
            slot.set(Some(state));
            VNode::empty()
        },
        (Rc::new(RefCell::new(Some(shell))), slot.clone()),
    );
    dom.rebuild_in_place();
    (dom, slot.get().unwrap(), worker, editor, evidence, stream)
}

async fn applied() -> (
    VirtualDom,
    observation::WorkerSource,
    Arc<Editor>,
    Operation,
    Box<Applied>,
) {
    let (mut dom, state, worker, editor, evidence, stream) = setup().await;
    let local = evidence.review().local().clone();
    let lease = editor
        .prepare_settlement(Visit::new(), &local)
        .await
        .unwrap();
    let operation = Operation::reserve(
        &editor,
        Seed {
            request: Request::Baseline {
                evidence: evidence.clone(),
                choice: super_platinum_core::drafts::store::sync::RemoteChoice::Remote,
            },
            local: Arc::new(local),
            book: None,
            serial: editor.snapshot().serial,
            epoch: 7,
        },
    )
    .unwrap_or_else(|_| panic!("exact baseline reservation"));
    let run_editor = editor.clone();
    let run_operation = operation.clone();
    let (send, mut recv) = tokio::sync::oneshot::channel();
    dom.in_runtime(|| {
        dioxus::core::spawn_forever(async move {
            let result = run_editor
                .apply_review_owned(
                    lease,
                    Action::Baseline {
                        evidence,
                        choice: super_platinum_core::drafts::store::sync::RemoteChoice::Remote,
                        state,
                    },
                    run_operation,
                    move || Ok(Some(stream)),
                )
                .await;
            let _ = send.send(result);
        })
    });
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop { tokio::select! { result = &mut recv => break result.unwrap(), _ = dom.wait_for_work() => { dom.render_immediate_to_vec(); } } }
    }).await.unwrap().unwrap();
    (dom, worker, editor, operation, result)
}

#[tokio::test]
async fn actual_attachment_remote_choice_records_barrier_without_loosening_consume() {
    let (_dom, _worker, editor, operation, result) = applied().await;
    assert_eq!(result.before.local().attachments.len(), 1);
    assert!(result.after.local().attachments.is_empty());
    assert_ne!(
        result.before.local().composer.text,
        result.after.local().composer.text
    );
    assert!(editor.owner.review().await.unwrap().same(&result.after));
    assert!(matches!(
        operation.consumed(&result),
        Err(durable::Error::Stale)
    ));
    let held = operation.0.evidence.lock().unwrap();
    assert!(held.consumed.is_none());
    assert!(Arc::ptr_eq(
        held.baseline_applied.as_ref().unwrap(),
        result.barrier.as_ref().unwrap()
    ));
    assert!(held.baseline_finish.is_some());
    drop(held);
    operation.finish(Err(durable::Error::ReviewRequired));
    assert_eq!(
        operation.ready().await.unwrap(),
        Status::Failed(durable::Error::ReviewRequired)
    );
    assert!(matches!(
        operation.acknowledge_baseline_projection(),
        Err(durable::Error::Pending)
    ));
    assert_eq!(editor.owner.account.retained_operations().unwrap().len(), 1);
}

#[tokio::test]
async fn exact_observed_projection_cleans_original_failure_and_reuses_finish_ack() {
    let (_dom, _worker, editor, operation, result) = applied().await;
    operation.finish(Err(durable::Error::ReviewRequired));
    let barrier = result.barrier.as_ref().unwrap().clone();
    // Representative observation only: the separate mounted fixture must prove
    // the renderer creates this receipt after actual native paired installation.
    let proof = Arc::new(Acknowledged {
        barrier: barrier.clone(),
        review: result.after.clone(),
        editor: Arc::downgrade(&editor),
    });
    let finish = barrier.owned_finish.as_ref().unwrap();
    assert!(matches!(
        finish.acknowledge(&result.before),
        Err(durable::Error::Stale)
    ));
    finish.acknowledge(&result.after).unwrap();
    finish.acknowledge(&result.after).unwrap();
    operation.record_baseline_projection(proof).unwrap();
    assert_eq!(
        operation.acknowledge_baseline_projection().unwrap(),
        Status::Failed(durable::Error::ReviewRequired)
    );
    assert!(
        editor
            .owner
            .account
            .retained_operations()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        operation.ready().await.unwrap(),
        Status::Failed(durable::Error::ReviewRequired)
    );
    assert!(matches!(
        operation.acknowledge_baseline_projection(),
        Err(durable::Error::Stale)
    ));
}

#[tokio::test]
async fn wrong_editor_review_and_unobserved_running_operation_cannot_retire() {
    let (_dom, _worker, editor, operation, result) = applied().await;
    let barrier = result.barrier.as_ref().unwrap().clone();
    let wrong = Arc::new(Acknowledged {
        barrier: barrier.clone(),
        review: result.after.clone(),
        editor: Weak::new(),
    });
    assert!(matches!(
        operation.record_baseline_projection(wrong),
        Err(durable::Error::Stale)
    ));
    let wrong = Arc::new(Acknowledged {
        barrier: barrier.clone(),
        review: result.before.clone(),
        editor: Arc::downgrade(&editor),
    });
    assert!(matches!(
        operation.record_baseline_projection(wrong),
        Err(durable::Error::Stale)
    ));
    assert!(matches!(
        operation.acknowledge_baseline_projection(),
        Err(durable::Error::Pending)
    ));
    assert_eq!(editor.owner.account.retained_operations().unwrap().len(), 1);
}

/// Test-only actual-owned-worker sequence on one Account. Every later read uses
/// a fresh real reservation/request and a literal synthetic positive response
/// from the previously observed remote. No production fixture limit is widened.
pub(crate) struct RecoveryFixture {
    pub(crate) dom: VirtualDom,
    state: Signal<ShellState>,
    _worker: observation::WorkerSource,
    editor: Arc<Editor>,
    evidence: Arc<crate::drafts::protected::baseline::Evidence>,
    stream: durable::Stream,
    cycle: usize,
}
pub(crate) struct RecoveryRequest {
    pub(crate) operation: Operation,
    pub(crate) future:
        std::pin::Pin<Box<dyn std::future::Future<Output = Result<Status, durable::Error>>>>,
    pub(crate) applied: tokio::sync::oneshot::Receiver<Box<Applied>>,
}
impl RecoveryFixture {
    pub(crate) async fn new() -> Self {
        let (dom, state, worker, editor, evidence, stream) = setup().await;
        Self {
            dom,
            state,
            _worker: worker,
            editor,
            evidence,
            stream,
            cycle: 0,
        }
    }
    pub(crate) fn account_id(&self) -> String {
        self.editor.owner.account.id().into()
    }
    pub(crate) fn retained(&self) -> Vec<Operation> {
        self.editor.owner.account.retained_operations().unwrap()
    }
    pub(crate) async fn reserve(&mut self) -> RecoveryRequest {
        if self.cycle > 0 {
            // Reopen the same actual owner after the prior unit-observed
            // projection. Native installation is deliberately not simulated.
            self.editor = Editor::open(self.editor.owner.clone()).await.unwrap();
            let ops = self
                .editor
                .owner
                .account
                .owned
                .as_ref()
                .unwrap()
                .operations
                .clone();
            // A previous positive choice verified this epoch. A later baseline
            // needs a genuine new continuity epoch, not re-use of that proof.
            let scope = self.editor.owner.scope().clone();
            Held::new(ops.mark_gap(o::GapInput {
                stream: self.stream.clone(),
                reason: durable::GapReason::Disconnect,
            }))
            .observed_success()
            .await
            .unwrap();
            assert!(!self.stream.is_live());
            let stamp = Held::new(ops.stream_state(o::TeamInput {
                scope: scope.clone(),
            }))
            .observed_success()
            .await
            .unwrap();
            self.stream = Held::new(ops.open_stream(o::OpenInput {
                scope,
                expected: stamp,
            }))
            .observed_success()
            .await
            .unwrap();
            let generation = 9 + u64::try_from(self.cycle).unwrap();
            let source = self.dom.in_runtime(|| {
                let mut shell = self.state.write();
                assert!(self._worker.can_accept(&shell, "T1", generation));
                shell.core.workspaces.get_mut("T1").unwrap().rt_generation = generation;
                let socket = Arc::new(self._worker.accept(&mut shell, "T1", generation).unwrap());
                Source::captured(
                    self.editor.owner.account.clone(),
                    socket,
                    generation,
                    self.stream.clone(),
                    &shell,
                )
                .unwrap()
            });
            let expected = self.editor.owner.review().await.unwrap();
            assert_ne!(
                expected.row().lineage().verified_epoch(),
                self.stream.epoch()
            );
            // The real positive read retains its 30-second floor even on success.
            // Advance the explicit synthetic clock; never sleep or reset deadlines.
            let now_ms = 1000 + u64::try_from(self.cycle).unwrap() * 30_000;
            let stream = self.stream.clone();
            let remote = self.evidence.evidence.remote().clone();
            let evidence=tokio::task::spawn_blocking(move || {
                let reserved=ops.baseline(o::BaselineInput::new(stream,expected,now_ms)).unwrap();
                reserved.submit().unwrap();assert_eq!(reserved.wait().unwrap(),o::JobState::Ready);
                let claim=reserved.claim().unwrap();claim.submit().unwrap();assert_eq!(claim.wait().unwrap(),o::JobState::Ready);
                let completion=claim.completion().unwrap();let request=completion.take_request().unwrap();
                assert_eq!(request.scope(),&remote.scope);assert_eq!(request.draft_id(),remote.id);
                completion.bind(o::BaselineRead {response:durable::BaselineResponse::Positive {raw:serde_json::json!({"ok":true,"draft":{
                    "id":remote.id,"client_msg_id":remote.client_msg_id,"last_updated_ts":remote.last_updated_ts,
                    "team_id":"T1","user_id":"U1","is_deleted":false,"is_sent":false,"date_scheduled":0,
                    "attachments":[],"blocks":remote.snapshot.blocks,"file_ids":remote.snapshot.file_ids,"destinations":remote.snapshot.destinations
                }}),user:"U1".into()},known_until:0}).unwrap();
                completion.submit().unwrap();completion.wait().unwrap().unwrap();
                let (result,ack)=completion.observe_success().unwrap();let evidence=result.evidence().unwrap();assert!(ack.acknowledge().is_ok());evidence
            }).await.unwrap();
            self.evidence = Arc::new(crate::drafts::protected::baseline::Evidence {
                evidence,
                binding: self.evidence.binding.clone(),
                source,
            });
        }
        self.cycle += 1;
        let local = self.evidence.review().local().clone();
        let lease = self
            .editor
            .prepare_settlement(Visit::new(), &local)
            .await
            .unwrap();
        let operation = Operation::reserve(
            &self.editor,
            Seed {
                request: Request::Baseline {
                    evidence: self.evidence.clone(),
                    choice: super_platinum_core::drafts::store::sync::RemoteChoice::Remote,
                },
                local: Arc::new(local),
                book: None,
                serial: self.editor.snapshot().serial,
                epoch: 7,
            },
        )
        .unwrap_or_else(|_| panic!("same-account capacity"));
        let editor = self.editor.clone();
        let evidence = self.evidence.clone();
        let stream = self.stream.clone();
        let state = self.state;
        let retained = operation.clone();
        let (send, applied) = tokio::sync::oneshot::channel();
        let future = Box::pin(async move {
            let value = editor
                .apply_review_owned(
                    lease,
                    Action::Baseline {
                        evidence,
                        choice: super_platinum_core::drafts::store::sync::RemoteChoice::Remote,
                        state,
                    },
                    retained,
                    move || Ok(Some(stream)),
                )
                .await?;
            let _ = send.send(value);
            // Explicit unit failure at the native-projection boundary.
            Err(durable::Error::ReviewRequired)
        });
        RecoveryRequest {
            operation,
            future,
            applied,
        }
    }
    pub(crate) async fn drive(
        &mut self,
        mut recv: tokio::sync::oneshot::Receiver<Box<Applied>>,
    ) -> Box<Applied> {
        tokio::time::timeout(std::time::Duration::from_secs(10),async {
            loop {tokio::select! {value=&mut recv=>break value.unwrap(),_=self.dom.wait_for_work()=>{self.dom.render_immediate_to_vec();}}}
        }).await.unwrap()
    }
    pub(crate) fn observe_projection(&self, operation: &Operation, result: &Applied) {
        // Unit observation only; the native scenario supplies actual receipt.
        let barrier = result.barrier.as_ref().unwrap().clone();
        assert!(
            self.editor
                .snapshot()
                .projection
                .as_ref()
                .is_some_and(|v| Arc::ptr_eq(v, &barrier))
        );
        operation
            .record_baseline_projection(Arc::new(Acknowledged {
                barrier,
                review: result.after.clone(),
                editor: Arc::downgrade(&self.editor),
            }))
            .unwrap();
    }
}
