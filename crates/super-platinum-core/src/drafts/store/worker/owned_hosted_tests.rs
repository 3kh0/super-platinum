//! Actual private owned worker/SQLite; no configured store, network or UI.
use super::*;
use p::owned_store::{FactoryIdentity, OwnedRoute};
use serde_json::json;

fn done<I: Send + Sync + 'static, T: Send + Clone + 'static>(job: StorageJob<I, T>) -> T {
    job.submit().unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Ready);
    job.result().unwrap().unwrap()
}
struct Fixture {
    ops: Operations,
    input: Arc<HostedPrepared>,
    stream: p::Stream,
}
impl Fixture {
    fn new() -> Self {
        Self::caption("Owned caption Ω")
    }
    fn caption(text: &str) -> Self {
        let route = OwnedRoute::fresh_fixture().unwrap();
        let before = route.inspect().unwrap().wait().unwrap();
        route.commit(&before).unwrap().wait().unwrap();
        let review = route.inspect().unwrap().wait().unwrap();
        route.initialize_slash(&review).unwrap().wait().unwrap();
        let review = route.inspect().unwrap().wait().unwrap();
        route.initialize_hosted(&review).unwrap().wait().unwrap();
        let initialized = route
            .inspect()
            .unwrap()
            .wait()
            .unwrap()
            .open()
            .unwrap()
            .initialize_cold(&FactoryIdentity::default())
            .unwrap()
            .wait()
            .unwrap();
        let ops = initialized.operations();
        let scope = Scope {
            account_id: ops.account().into(),
            key: DraftKey {
                team: "T00000001".into(),
                channel: "C00000001".into(),
                thread: None,
            },
        };
        let local = done(ops.local_inventory(scope.clone()).unwrap());
        let review = done(ops.enroll(local).unwrap());
        let state = done(
            ops.stream_state(TeamInput {
                scope: scope.clone(),
            })
            .unwrap(),
        );
        let stream = done(
            ops.open_stream(OpenInput {
                scope,
                expected: state,
            })
            .unwrap(),
        );
        let review = done(
            ops.verify_local(MutationInput {
                stream: stream.clone(),
                expected: review,
            })
            .unwrap(),
        );
        let writer = done(ops.register(review.clone()).unwrap());
        let checkpoint = ops
            .checkpoint(CheckpointInput {
                writer: writer.clone(),
                serial: 1,
                expected: review,
                local: sync::tests::local(text),
                direct: None,
            })
            .unwrap();
        checkpoint.submit().unwrap();
        assert_eq!(checkpoint.wait().unwrap(), JobState::Ready);
        let WriteOutcome::Applied(review) = checkpoint.write_result().unwrap().unwrap() else {
            panic!("paired writer")
        };
        let mut snapshot = review.row().state().desired().cloned().unwrap_or_else(|| {
            crate::drafts::sync::Snapshot {
                blocks: vec![],
                file_ids: vec![],
                destinations: vec![json!({"channel_id":review.scope().key.channel})],
            }
        });
        snapshot.file_ids = vec!["FSECOND".into(), "FFIRST".into()];
        let source = |id| json!({"id":id,"mode":"hosted","is_external":false,"filetype":"pdf"});
        let input = HostedInput::new(snapshot, vec![source("FSECOND"), source("FFIRST")]).unwrap();
        let input =
            Arc::new(HostedPrepared::new(stream.clone(), review, writer, 1, input).unwrap());
        done(
            ops.save_hosted_draft(HostedDraftInput {
                input: input.clone(),
                prior: None,
                clear: false,
            })
            .unwrap(),
        );
        Self { ops, input, stream }
    }
    fn claim(&self) -> HostedClaimed {
        let job = self.ops.hosted(self.input.clone()).unwrap();
        job.submit().unwrap();
        assert_eq!(job.wait().unwrap(), JobState::Ready);
        job
    }
}
#[test]
fn actual_claim_is_retained_reconnectable_and_transfers_once() {
    let f = Fixture::new();
    let job = f.ops.hosted(f.input.clone()).unwrap();
    let key = job.key();
    job.submit().unwrap();
    drop(job);
    let job = f.ops.resume_hosted_claimed(&key).unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Ready);
    let completion = job.completion().unwrap();
    let key = completion.key();
    drop(completion);
    let completion = f.ops.resume_hosted_completion(&key).unwrap();
    completion.fixture_return(HostedReturned::Unknown).unwrap();
    assert_eq!(completion.wait().unwrap(), JobState::Ready);
    assert!(completion.fixture_return(HostedReturned::Unknown).is_err());
    assert!(job.completion().unwrap().returned().is_some());
    let row = completion.result().unwrap().unwrap();
    assert!(matches!(row.returned(), Some(HostedReturned::Unknown)));
    assert_eq!(row.input().snapshot().file_ids, vec!["FSECOND", "FFIRST"]);
}
#[test]
fn returned_result_survives_revocation_and_retains_exact_batch() {
    let f = Fixture::new();
    let job = f.claim();
    let completion = job.completion().unwrap();
    completion
        .queue
        .worker
        .arm_hosted(&completion.held)
        .unwrap();
    f.stream.revoke();
    completion
        .queue
        .worker
        .bind_hosted_returned(
            &completion.held,
            HostedReturned::Response(json!({"ok":true})),
        )
        .unwrap();
    completion.submit_returned().unwrap();
    assert_eq!(completion.wait().unwrap(), JobState::Ready);
    assert!(matches!(
        completion.result().unwrap().unwrap().returned(),
        Some(HostedReturned::Response(_))
    ));
    assert!(completion.retry_storage().is_err());
}
#[test]
fn revoked_stream_prevents_claim_and_final_handoff() {
    let f = Fixture::new();
    let job = f.claim();
    let completion = job.completion().unwrap();
    f.stream.revoke();
    assert!(completion.fixture_return(HostedReturned::Unknown).is_err());
    assert!(completion.returned().is_none());
    let g = Fixture::new();
    g.stream.revoke();
    let job = g.ops.hosted(g.input.clone()).unwrap();
    job.submit().unwrap();
    assert!(matches!(
        job.wait().unwrap(),
        JobState::Failed(p::Error::Gap)
    ));
    assert!(job.completion().is_err());
}
#[test]
fn another_owned_operation_retires_original_dispatch_activity() {
    let f = Fixture::new();
    let completion = f.claim().completion().unwrap();
    done(f.ops.review(f.input.review().scope().clone()).unwrap());
    assert!(completion.fixture_return(HostedReturned::Unknown).is_err());
    assert!(completion.returned().is_none());
}
#[test]
fn cross_owned_route_cannot_resume_claim_or_completion() {
    let f = Fixture::new();
    let g = Fixture::new();
    let job = f.claim();
    assert!(g.ops.resume_hosted_claimed(&job.key()).is_err());
    let completion = job.completion().unwrap();
    assert!(g.ops.resume_hosted_completion(&completion.key()).is_err());
}
#[test]
fn fresh_policy_refuses_changed_set_or_destination() {
    let f = Fixture::new();
    let current = f.input.input();
    let raw = json!({"id":"C00000001","is_channel":true,"is_member":true,"is_ext_shared":false,"is_read_only":false,"is_thread_only":false});
    let mut facts = PostingFacts::new("C00000001".into());
    assert!(facts.observe(&raw, None));
    assert!(HostedFresh::new(f.input.clone(), current, &raw, &facts, None).is_ok());
    let mut changed = current.snapshot().clone();
    changed.file_ids.reverse();
    let mut sources = current.sources().to_vec();
    sources.reverse();
    let changed = HostedInput::new(changed, sources).unwrap();
    assert!(HostedFresh::new(f.input.clone(), &changed, &raw, &facts, None).is_err());
    let mut denied = raw;
    denied["is_archived"] = json!(true);
    assert!(HostedFresh::new(f.input.clone(), current, &denied, &facts, None).is_err());
}

#[test]
fn file_only_batch_uses_exact_empty_local_caption_without_remote_decode() {
    let f = Fixture::caption("");
    assert!(f.input.review().row().state().desired().is_none());
    assert!(f.input.input().snapshot().blocks.is_empty());
    let completion = f.claim().completion().unwrap();
    completion
        .fixture_return(HostedReturned::Response(json!({"ok":true})))
        .unwrap();
    assert_eq!(completion.wait().unwrap(), JobState::Ready);
    assert_eq!(
        completion
            .result()
            .unwrap()
            .unwrap()
            .input()
            .snapshot()
            .file_ids,
        vec!["FSECOND", "FFIRST"]
    );
}

#[test]
fn actual_returned_storage_failure_retries_same_claim_without_second_arm() {
    let f = Fixture::new();
    let completion = f.claim().completion().unwrap();
    completion
        .queue
        .worker
        .arm_hosted(&completion.held)
        .unwrap();
    // Arm the existing isolated transaction-commit fault AFTER the HTTP handoff.
    // Its extra worker activity must not suppress retaining the original reply.
    done(
        f.ops
            .fixture_fault(
                f.input.review().scope().clone(),
                p::test_support::FaultPoint::MutationReceipt,
            )
            .unwrap(),
    );
    let raw = json!({"ok":true,"message_ts":"1700000000.000007","opaque":{"retain":[2,1]}});
    completion
        .queue
        .worker
        .bind_hosted_returned(&completion.held, HostedReturned::Response(raw.clone()))
        .unwrap();
    let retained = completion.returned().unwrap();
    completion.submit_returned().unwrap();
    assert_eq!(
        completion.wait().unwrap(),
        JobState::Failed(p::Error::Storage)
    );
    assert!(
        completion
            .queue
            .worker
            .arm_hosted(&completion.held)
            .is_err()
    );
    assert!(Arc::ptr_eq(&retained, &completion.returned().unwrap()));
    let key = completion.key();
    drop(completion);
    let completion = f.ops.resume_hosted_completion(&key).unwrap();
    completion.retry_storage().unwrap();
    assert_eq!(completion.wait().unwrap(), JobState::Ready);
    let record = completion.result().unwrap().unwrap();
    assert!(matches!(record.returned(),Some(HostedReturned::Response(value)) if value==&raw));
    assert_eq!(
        record.input().snapshot().file_ids,
        vec!["FSECOND", "FFIRST"]
    );
    assert!(Arc::ptr_eq(&retained, &completion.returned().unwrap()));
    assert!(
        completion
            .queue
            .worker
            .arm_hosted(&completion.held)
            .is_err()
    );
    assert!(completion.retry_storage().is_err());
}
#[test]
fn actual_editor_cas_and_removal_preserve_caption() {
    let f = Fixture::new();
    let scope = f.input.review().scope().clone();
    let saved = done(f.ops.load_hosted_draft(scope.clone()).unwrap()).unwrap();
    assert_eq!(saved.input().snapshot().file_ids, vec!["FSECOND", "FFIRST"]);
    let stale = f
        .ops
        .save_hosted_draft(HostedDraftInput {
            input: f.input.clone(),
            prior: None,
            clear: true,
        })
        .unwrap();
    stale.submit().unwrap();
    assert_eq!(stale.wait().unwrap(), JobState::Failed(p::Error::Stale));
    assert!(
        done(
            f.ops
                .save_hosted_draft(HostedDraftInput {
                    input: f.input.clone(),
                    prior: Some(saved),
                    clear: true
                })
                .unwrap()
        )
        .is_none()
    );
    assert!(done(f.ops.load_hosted_draft(scope.clone()).unwrap()).is_none());
    let current = done(f.ops.review(scope).unwrap());
    assert!(current.same(f.input.review()));
    let unsaved = f.ops.hosted(f.input.clone()).unwrap();
    unsaved.submit().unwrap();
    assert!(matches!(unsaved.wait().unwrap(), JobState::Failed(_)));
    assert!(unsaved.completion().is_err());
}

#[test]
fn exact_saved_set_and_clear_reconcile_a_lost_commit_reply_without_replacing_identity() {
    let f = Fixture::new();
    let scope = f.input.review().scope().clone();
    let original = done(f.ops.load_hosted_draft(scope.clone()).unwrap()).unwrap();
    let duplicate = done(
        f.ops
            .save_hosted_draft(HostedDraftInput {
                input: f.input.clone(),
                prior: None,
                clear: false,
            })
            .unwrap(),
    )
    .unwrap();
    assert_eq!(original.id(), duplicate.id());
    let clear = HostedDraftInput {
        input: f.input.clone(),
        prior: Some(original),
        clear: true,
    };
    assert!(done(f.ops.save_hosted_draft(clear.clone()).unwrap()).is_none());
    assert!(done(f.ops.save_hosted_draft(clear).unwrap()).is_none());
    assert!(done(f.ops.review(scope).unwrap()).same(f.input.review()));
}

#[test]
fn save_storage_failure_retries_the_same_owned_slot_then_retires_exact_failed_attempts() {
    let f = Fixture::new();
    let prior = done(
        f.ops
            .load_hosted_draft(f.input.review().scope().clone())
            .unwrap(),
    );
    done(
        f.ops
            .fixture_fault(
                f.input.review().scope().clone(),
                p::test_support::FaultPoint::MutationReceipt,
            )
            .unwrap(),
    );
    let job = f
        .ops
        .save_hosted_draft(HostedDraftInput {
            input: f.input.clone(),
            prior,
            clear: true,
        })
        .unwrap();
    let key = job.key();
    job.submit().unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Failed(p::Error::Storage));
    let held = job.input();
    job.retry_hosted_storage().unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Ready);
    assert!(Arc::ptr_eq(&held, &job.input()));
    let (value, ack) = job.observe_hosted_saved().unwrap();
    assert!(value.is_none());
    ack.acknowledge().map_err(|(e, _)| e).unwrap();
    assert!(f.ops.resume_hosted_draft_save(&key).is_err());
    assert!(job.retry_hosted_storage().is_err());
    assert_eq!(
        held.input.input().snapshot().file_ids,
        vec!["FSECOND", "FFIRST"]
    );
}

#[test]
fn accepted_caption_and_file_only_cleanup_emit_one_fresh_clear_and_retire_exact_family() {
    for text in ["Owned caption Ω", ""] {
        let f = Fixture::caption(text);
        let job = f.claim();
        let completion = job.completion().unwrap();
        completion
            .fixture_return(HostedReturned::Response(json!({"ok":true})))
            .unwrap();
        assert_eq!(completion.wait().unwrap(), JobState::Ready);
        let cleanup = completion.cleanup().unwrap();
        cleanup.submit().unwrap();
        assert_eq!(cleanup.wait().unwrap(), JobState::Ready);
        let (receipt, clear, ack) = cleanup.observe().unwrap();
        assert_eq!(receipt.effect(), HostedEffect::ClearedExact);
        let clear = clear.expect("new exact commit");
        assert_eq!(clear.claim_id(), receipt.record().id());
        assert!(clear.after().unwrap().local().empty());
        assert!(cleanup.observe().unwrap().1.is_none());
        let scope = f.input.review().scope().clone();
        assert!(done(f.ops.load_hosted_draft(scope).unwrap()).is_none());
        assert!(ack.acknowledge().is_ok());
        assert!(f.ops.resume_hosted_claimed(&job.key()).is_err());
        assert!(f.ops.resume_hosted_completion(&completion.key()).is_err());
        assert!(completion.fixture_return(HostedReturned::Unknown).is_err());
    }
}

#[test]
fn accepted_cleanup_preserves_newer_caption_and_saved_set_without_native_clear() {
    let f = Fixture::new();
    let completion = f.claim().completion().unwrap();
    completion
        .fixture_return(HostedReturned::Response(json!({"ok":true})))
        .unwrap();
    assert_eq!(completion.wait().unwrap(), JobState::Ready);
    let scope = f.input.review().scope().clone();
    let expected = done(f.ops.review(scope.clone()).unwrap());
    let changed = f
        .ops
        .checkpoint(CheckpointInput {
            writer: f.input.writer.clone(),
            serial: 2,
            expected,
            local: sync::tests::local("New caption remains"),
            direct: None,
        })
        .unwrap();
    changed.submit().unwrap();
    assert_eq!(changed.wait().unwrap(), JobState::Ready);
    let cleanup = completion.cleanup().unwrap();
    cleanup.submit().unwrap();
    assert_eq!(cleanup.wait().unwrap(), JobState::Ready);
    let (receipt, clear, _ack) = cleanup.observe().unwrap();
    assert_eq!(receipt.effect(), HostedEffect::PreservedCurrent);
    assert!(clear.is_none());
    assert_eq!(
        done(f.ops.review(scope.clone()).unwrap())
            .local()
            .composer
            .text,
        "New caption remains"
    );
    assert_eq!(
        done(f.ops.load_hosted_draft(scope).unwrap())
            .unwrap()
            .input()
            .snapshot()
            .file_ids,
        vec!["FSECOND", "FFIRST"]
    );
}

#[test]
fn unknown_and_dlp_processing_cannot_unlock_or_clear_hosted_input() {
    for returned in [
        HostedReturned::Unknown,
        HostedReturned::Response(json!({"ok":false,"error":"dlp_processing"})),
    ] {
        let f = Fixture::new();
        let completion = f.claim().completion().unwrap();
        completion.fixture_return(returned).unwrap();
        assert_eq!(completion.wait().unwrap(), JobState::Ready);
        let cleanup = completion.cleanup().unwrap();
        cleanup.submit().unwrap();
        assert_eq!(cleanup.wait().unwrap(), JobState::Failed(p::Error::Pending));
        assert!(cleanup.observe().is_err());
        assert!(
            done(
                f.ops
                    .load_hosted_draft(f.input.review().scope().clone())
                    .unwrap()
            )
            .is_some()
        );
        assert!(completion.fixture_return(HostedReturned::Unknown).is_err());
    }
}

#[test]
fn returned_and_cleanup_storage_failures_retire_only_with_exact_successor_evidence() {
    let f = Fixture::new();
    let completion = f.claim().completion().unwrap();
    completion
        .queue
        .worker
        .arm_hosted(&completion.held)
        .unwrap();
    done(
        f.ops
            .fixture_fault(
                f.input.review().scope().clone(),
                p::test_support::FaultPoint::MutationReceipt,
            )
            .unwrap(),
    );
    completion
        .queue
        .worker
        .bind_hosted_returned(
            &completion.held,
            HostedReturned::Response(json!({"ok":true,"retained":"original"})),
        )
        .unwrap();
    let original = completion.returned().unwrap();
    completion.submit_returned().unwrap();
    assert_eq!(
        completion.wait().unwrap(),
        JobState::Failed(p::Error::Storage)
    );
    completion.retry_storage().unwrap();
    assert_eq!(completion.wait().unwrap(), JobState::Ready);
    done(
        f.ops
            .fixture_fault(
                f.input.review().scope().clone(),
                p::test_support::FaultPoint::MutationReceipt,
            )
            .unwrap(),
    );
    let cleanup = completion.cleanup().unwrap();
    cleanup.submit().unwrap();
    assert_eq!(cleanup.wait().unwrap(), JobState::Failed(p::Error::Storage));
    assert!(cleanup.observe().is_err());
    assert!(
        done(
            f.ops
                .load_hosted_draft(f.input.review().scope().clone())
                .unwrap()
        )
        .is_some()
    );
    cleanup.retry_storage().unwrap();
    assert_eq!(cleanup.wait().unwrap(), JobState::Ready);
    let (receipt, clear, ack) = cleanup.observe().unwrap();
    assert_eq!(receipt.effect(), HostedEffect::ClearedExact);
    assert!(clear.is_some());
    assert!(Arc::ptr_eq(&original, &completion.returned().unwrap()));
    assert!(ack.acknowledge().is_ok());
    assert!(completion.fixture_return(HostedReturned::Unknown).is_err());
    assert!(cleanup.retry_storage().is_err());
}

#[test]
fn saved_draft_observation_retires_only_its_successful_step() {
    let f = Fixture::new();
    let job = f
        .ops
        .load_hosted_draft(f.input.review().scope().clone())
        .unwrap();
    job.submit().unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Ready);
    let key = job.key();
    let (draft, ack) = job.observe_success().unwrap();
    assert!(draft.is_some());
    assert!(ack.acknowledge().is_ok());
    assert!(f.ops.resume_hosted_draft_load(&key).is_err());
}

#[test]
fn whole_owned_inventory_keeps_saved_active_and_completed_hosted_content_distinct() {
    let f = Fixture::new();
    let before = done(f.ops.inventory().unwrap());
    assert_eq!(before.hosted().drafts().len(), 1);
    assert!(before.hosted().claims().is_empty());
    assert!(before.hosted().completed().is_empty());
    let completion = f.claim().completion().unwrap();
    let claimed = done(f.ops.inventory().unwrap());
    assert_eq!(claimed.hosted().claims().len(), 1);
    assert_eq!(claimed.hosted().drafts().len(), 1);
    // Read-only inventory retires the dispatch activity. It never grants a new
    // Claim or permits a resumed HTTP, even though all original IDs are visible.
    assert!(completion.fixture_return(HostedReturned::Unknown).is_err());
    let g = Fixture::new();
    let completion = g.claim().completion().unwrap();
    completion
        .fixture_return(HostedReturned::Response(json!({"ok":true})))
        .unwrap();
    assert_eq!(completion.wait().unwrap(), JobState::Ready);
    let cleanup = completion.cleanup().unwrap();
    cleanup.submit().unwrap();
    assert_eq!(cleanup.wait().unwrap(), JobState::Ready);
    let after = done(g.ops.inventory().unwrap());
    assert!(after.hosted().drafts().is_empty());
    assert!(after.hosted().claims().is_empty());
    assert_eq!(after.hosted().completed().len(), 1);
    assert_eq!(
        after.hosted().completed()[0]
            .record()
            .input()
            .snapshot()
            .file_ids,
        vec!["FSECOND", "FFIRST"]
    );
}

fn fresh_for_arm(f: &Fixture) -> HostedFresh {
    let raw = json!({"id":"C00000001","is_channel":true,"is_member":true,"is_ext_shared":false,"is_read_only":false,"is_thread_only":false});
    let mut facts = PostingFacts::new("C00000001".into());
    assert!(facts.observe(&raw, None));
    HostedFresh::new(f.input.clone(), f.input.input(), &raw, &facts, None).unwrap()
}
fn arm_session() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T00000001".into(),
        user_id: "U00000001".into(),
        enterprise_id: None,
        name: "Disconnected".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic-token".into(),
    }
}
#[test]
fn dropping_armed_request_never_restores_dispatch_authority() {
    let f = Fixture::new();
    let completion = f.claim().completion().unwrap();
    let attempt = completion.arm(fresh_for_arm(&f), &arm_session()).unwrap();
    assert!(
        attempt
            .request
            .url
            .starts_with("https://fixture.slack.com/api/files.share")
    );
    assert!(completion.returned().is_none());
    drop(attempt);
    assert!(completion.arm(fresh_for_arm(&f), &arm_session()).is_err());
    let retained = done(f.ops.inventory().unwrap());
    assert_eq!(retained.hosted().claims().len(), 1);
    assert!(retained.hosted().claims()[0].returned().is_none());
}
#[tokio::test]
async fn failed_prearm_owner_does_not_spend_request_and_postarm_revocation_keeps_returned_custody()
{
    let f = Fixture::new();
    let completion = f.claim().completion().unwrap();
    let mut wrong = arm_session();
    wrong.team_id = "T00000002".into();
    assert!(completion.arm(fresh_for_arm(&f), &wrong).is_err());
    let attempt = completion.arm(fresh_for_arm(&f), &arm_session()).unwrap();
    f.stream.revoke();
    let transport = Transport::disconnected_fixture("synthetic-cookie");
    attempt.execute(&transport).await.unwrap();
    assert_eq!(completion.wait().unwrap(), JobState::Ready);
    assert!(matches!(
        completion.returned().as_deref(),
        Some(HostedReturned::Unknown)
    ));
    assert!(completion.arm(fresh_for_arm(&f), &arm_session()).is_err());
}

#[path = "owned_hosted_retirement_tests.rs"]
mod retirement;
