//! Actual isolated queue and SQLite, including repeated lifetime use.
use super::*;
fn observed<I: Send + Sync + 'static, T: Send + Clone + 'static>(job: StorageJob<I, T>) -> T {
    job.submit().unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Ready);
    let (value, ack) = job.observe_success().unwrap();
    assert!(ack.acknowledge().is_ok());
    value
}
fn accept(f: &Fixture) -> (HostedReceipt, HostedCompletion) {
    let completion = f.claim().completion().unwrap();
    completion
        .fixture_return(HostedReturned::Response(json!({"ok":true})))
        .unwrap();
    assert_eq!(completion.wait().unwrap(), JobState::Ready);
    let cleanup = completion.cleanup().unwrap();
    cleanup.submit().unwrap();
    assert_eq!(cleanup.wait().unwrap(), JobState::Ready);
    let (receipt, _clear, ack) = cleanup.observe().unwrap();
    assert!(ack.acknowledge().is_ok());
    (receipt, completion)
}
fn retire(f: &Fixture, input: HostedRetirement) -> HostedRetired {
    let job = f.ops.retire_hosted(input).unwrap();
    job.submit().unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Ready);
    let (value, ack) = job.observe_retirement().unwrap();
    assert!(ack.acknowledge().is_ok());
    assert!(f.ops.resume_hosted_retirement(&job.key()).is_err());
    assert!(job.retry_retirement_storage().is_err());
    value
}
#[test]
fn ninety_six_accepted_shares_retire_exact_history_without_lifetime_capacity_exhaustion() {
    let mut f = Fixture::new();
    for index in 0..96u64 {
        let (receipt, completion) = accept(&f);
        let result = retire(&f, HostedRetirement::new(receipt, None).unwrap());
        assert_eq!(result.disposition(), HostedRetirementDisposition::Removed);
        assert!(result.remaining().is_none());
        assert!(completion.fixture_return(HostedReturned::Unknown).is_err());
        let inventory = observed(f.ops.inventory().unwrap());
        assert!(inventory.hosted().completed().is_empty());
        assert!(inventory.hosted().claims().is_empty());
        if index == 95 {
            break;
        }
        let review = observed(f.ops.review(f.input.review().scope().clone()).unwrap());
        let job = f
            .ops
            .checkpoint(CheckpointInput {
                writer: f.input.writer.clone(),
                serial: index + 2,
                expected: review,
                local: sync::tests::local("Next caption"),
                direct: None,
            })
            .unwrap();
        job.submit().unwrap();
        assert_eq!(job.wait().unwrap(), JobState::Ready);
        let (review, ack) = job.observe_checkpoint().unwrap();
        assert!(ack.acknowledge().is_ok());
        let mut snapshot = review.row().state().desired().unwrap().clone();
        snapshot.file_ids = vec!["FSECOND".into(), "FFIRST".into()];
        let input = HostedInput::new(snapshot, f.input.input().sources().to_vec()).unwrap();
        f.input = Arc::new(
            HostedPrepared::new(
                f.stream.clone(),
                review,
                f.input.writer.clone(),
                index + 2,
                input,
            )
            .unwrap(),
        );
        let save = f
            .ops
            .save_hosted_draft(HostedDraftInput {
                input: f.input.clone(),
                prior: None,
                clear: false,
            })
            .unwrap();
        save.submit().unwrap();
        assert_eq!(save.wait().unwrap(), JobState::Ready);
        let (_, ack) = save.observe_hosted_saved().unwrap();
        assert!(ack.acknowledge().is_ok());
    }
}
#[test]
fn retirement_storage_retry_keeps_original_receipt_and_retires_failed_attempts_only_after_success()
{
    let f = Fixture::new();
    let (receipt, _) = accept(&f);
    let fault = f
        .ops
        .fixture_fault(
            f.input.review().scope().clone(),
            p::test_support::FaultPoint::MutationReceipt,
        )
        .unwrap();
    done(fault);
    let job = f
        .ops
        .retire_hosted(HostedRetirement::new(receipt.clone(), None).unwrap())
        .unwrap();
    let input = job.input();
    job.submit().unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Failed(p::Error::Storage));
    assert!(job.observe_retirement().is_err());
    let inventory = observed(f.ops.inventory().unwrap());
    assert_eq!(inventory.hosted().completed().len(), 1);
    job.retry_retirement_storage().unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Ready);
    assert!(Arc::ptr_eq(&input, &job.input()));
    let (value, ack) = job.observe_retirement().unwrap();
    assert_eq!(value.disposition(), HostedRetirementDisposition::Removed);
    assert!(ack.acknowledge().is_ok());
    // A separate repeat can truthfully report absence, but cannot mint clear/dispatch.
    assert_eq!(
        retire(&f, HostedRetirement::new(receipt, None).unwrap()).disposition(),
        HostedRetirementDisposition::AlreadyAbsent
    );
}
#[test]
fn reviewed_preserved_attachment_removal_keeps_newer_caption() {
    let f = Fixture::new();
    let scope = f.input.review().scope().clone();
    let original = observed(f.ops.load_hosted_draft(scope.clone()).unwrap()).unwrap();
    let completion = f.claim().completion().unwrap();
    completion
        .fixture_return(HostedReturned::Response(json!({"ok":true})))
        .unwrap();
    completion.wait().unwrap();
    let expected = observed(f.ops.review(scope.clone()).unwrap());
    let checkpoint = f
        .ops
        .checkpoint(CheckpointInput {
            writer: f.input.writer.clone(),
            serial: 2,
            expected,
            local: sync::tests::local("Newer caption preserved"),
            direct: None,
        })
        .unwrap();
    checkpoint.submit().unwrap();
    assert_eq!(checkpoint.wait().unwrap(), JobState::Ready);
    let cleanup = completion.cleanup().unwrap();
    cleanup.submit().unwrap();
    assert_eq!(cleanup.wait().unwrap(), JobState::Ready);
    let (receipt, clear, ack) = cleanup.observe().unwrap();
    assert_eq!(receipt.effect(), HostedEffect::PreservedCurrent);
    assert!(clear.is_none());
    assert!(ack.acknowledge().is_ok());
    // Acceptance changes only the claim's returned response; the original
    // saved attachment binding still has returned=None and must remain owned.
    let current = observed(f.ops.load_hosted_draft(scope.clone()).unwrap()).unwrap();
    assert_eq!(
        serde_json::to_value(&current).unwrap(),
        serde_json::to_value(&original).unwrap()
    );
    let original_binding = serde_json::to_value(&original).unwrap();
    assert!(original_binding["binding"]["returned"].is_null());
    assert!(receipt.record().returned().is_some());
    let preserved = retire(&f, HostedRetirement::new(receipt.clone(), None).unwrap());
    assert_eq!(
        preserved.disposition(),
        HostedRetirementDisposition::Preserved
    );
    assert_eq!(
        serde_json::to_value(preserved.completed()).unwrap(),
        serde_json::to_value(vec![receipt.clone()]).unwrap()
    );
    assert_eq!(
        serde_json::to_value(preserved.remaining()).unwrap(),
        serde_json::to_value(Some(&original)).unwrap()
    );
    let mut changed = serde_json::to_value(&original).unwrap();
    changed["binding"]["input"]["sources"][0]["title"] = json!("Different reviewed file");
    let mismatched: HostedDraft = serde_json::from_value(changed).unwrap();
    let refused = retire(
        &f,
        HostedRetirement::new(receipt.clone(), Some(mismatched)).unwrap(),
    );
    assert_eq!(
        refused.disposition(),
        HostedRetirementDisposition::Preserved
    );
    assert_eq!(
        serde_json::to_value(refused.remaining()).unwrap(),
        serde_json::to_value(Some(&original)).unwrap()
    );
    assert_eq!(
        serde_json::to_value(refused.completed()).unwrap(),
        serde_json::to_value(vec![receipt.clone()]).unwrap()
    );
    let result = retire(
        &f,
        HostedRetirement::new(receipt.clone(), Some(original)).unwrap(),
    );
    assert_eq!(result.disposition(), HostedRetirementDisposition::Removed);
    assert!(result.remaining().is_none());
    assert!(result.completed().is_empty());
    assert!(result.claims().is_empty());
    assert_eq!(
        retire(&f, HostedRetirement::new(receipt, None).unwrap()).disposition(),
        HostedRetirementDisposition::AlreadyAbsent
    );
    assert_eq!(
        observed(f.ops.review(scope.clone()).unwrap())
            .local()
            .composer
            .text,
        "Newer caption preserved"
    );
    assert!(observed(f.ops.load_hosted_draft(scope).unwrap()).is_none());
}
#[test]
fn foreign_receipt_cannot_retire_another_account_or_uncertain_claim() {
    let f = Fixture::new();
    let (receipt, _) = accept(&f);
    let g = Fixture::new();
    assert!(
        g.ops
            .retire_hosted(HostedRetirement::new(receipt.clone(), None).unwrap())
            .is_err()
    );
    let completion = g.claim().completion().unwrap();
    completion.fixture_return(HostedReturned::Unknown).unwrap();
    completion.wait().unwrap();
    assert!(completion.cleanup().unwrap().observe().is_err());
    assert_eq!(
        observed(g.ops.inventory().unwrap()).hosted().claims().len(),
        1
    );
    assert_eq!(
        observed(f.ops.inventory().unwrap())
            .hosted()
            .completed()
            .len(),
        1
    );
}

#[test]
fn retiring_old_acceptance_preserves_a_newer_attachment_set_and_its_uncertain_claim() {
    let f = Fixture::new();
    let (receipt, _) = accept(&f);
    let scope = f.input.review().scope().clone();
    let review = observed(f.ops.review(scope.clone()).unwrap());
    let job = f
        .ops
        .checkpoint(CheckpointInput {
            writer: f.input.writer.clone(),
            serial: 2,
            expected: review,
            local: sync::tests::local("Later caption"),
            direct: None,
        })
        .unwrap();
    job.submit().unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Ready);
    let (review, ack) = job.observe_checkpoint().unwrap();
    assert!(ack.acknowledge().is_ok());
    let mut snapshot = review.row().state().desired().unwrap().clone();
    snapshot.file_ids = vec!["FLATER".into()];
    let source =
        json!({"id":"FLATER","mode":"hosted","is_external":false,"filetype":"pdf","title":"Later"});
    let input = Arc::new(
        HostedPrepared::new(
            f.stream.clone(),
            review,
            f.input.writer.clone(),
            2,
            HostedInput::new(snapshot, vec![source]).unwrap(),
        )
        .unwrap(),
    );
    let saved = done(
        f.ops
            .save_hosted_draft(HostedDraftInput {
                input: input.clone(),
                prior: None,
                clear: false,
            })
            .unwrap(),
    )
    .unwrap();
    let claim = f.ops.hosted(input).unwrap();
    claim.submit().unwrap();
    assert_eq!(claim.wait().unwrap(), JobState::Ready);
    let completion = claim.completion().unwrap();
    completion.fixture_return(HostedReturned::Unknown).unwrap();
    assert_eq!(completion.wait().unwrap(), JobState::Ready);
    let result = retire(&f, HostedRetirement::new(receipt, None).unwrap());
    assert_eq!(result.disposition(), HostedRetirementDisposition::Removed);
    assert_eq!(result.claims().len(), 1);
    assert!(result.completed().is_empty());
    assert_eq!(
        serde_json::to_value(result.remaining()).unwrap(),
        serde_json::to_value(Some(&saved)).unwrap()
    );
    let inventory = observed(f.ops.inventory().unwrap());
    assert_eq!(inventory.hosted().claims().len(), 1);
    assert_eq!(
        inventory.hosted().drafts()[0].input().snapshot().file_ids,
        vec!["FLATER"]
    );
    assert_eq!(
        observed(f.ops.review(scope).unwrap()).local().composer.text,
        "Later caption"
    );
    assert!(completion.fixture_return(HostedReturned::Unknown).is_err());
}
