//! Actual owned worker/HTTP builder/release/Host cycles. No browser/native claim:
//! only the already-tested renderer Finished/publication state is modeled here.
use super::*;
use crate::state::ShellState;

async fn sent(f: &Fixture) -> (operation::Operation, dispatch::Dispatched) {
    let (parent, applied, held) = Box::pin(f.consume()).await;
    let intent = applied.after.row().state().consumption().unwrap().clone();
    let binding = crate::drafts::network::Binding::retained_route_test(f.account.id());
    let copy = parent.clone();
    let dispatched = held
        .dispatch_with(&parent, &binding, move |request| {
            assert_eq!(
                url::Url::parse(&request.url).unwrap().path(),
                "/api/chat.postMessage"
            );
            let receipt = copy
                .record()
                .evidence
                .lock()
                .unwrap()
                .receipt
                .clone()
                .unwrap();
            let id = receipt.operation().client_msg_id.clone();
            std::future::ready(Ok(json!({"ok":true,"channel":"C1","ts":"200.000001",
            "message":{"channel":"C1","ts":"200.000001","user":"U1","client_msg_id":id}})))
        })
        .await
        .unwrap();
    f.editor.refresh_receipt().await.unwrap();
    f.editor
        .release_completion_owned(dispatched.review.clone(), intent, parent.clone())
        .await
        .unwrap();
    (parent, dispatched)
}
fn rendered_finished(parent: &operation::Operation, dispatched: dispatch::Dispatched) {
    parent
        .retain(|evidence| {
            evidence.finished = Some(Arc::new(delivery::Finished {
                dispatched,
                inventory_published: true,
                queue_published: false,
                local_handoff: Ok(()),
            }));
        })
        .unwrap();
    parent.finish(Ok(operation::Status::Finished));
}
async fn load(host: &mut lifecycle::Host, shell: &ShellState) {
    let load = host.begin_load(shell).unwrap();
    load.ready().await;
    host.finish_load(shell, &load).unwrap();
}
async fn observed<I: Send + Sync + 'static, T: Clone + Send + 'static>(
    job: o::StorageJob<I, T>,
) -> T {
    tokio::task::spawn_blocking(move || {
        job.submit().unwrap();
        assert_eq!(job.wait().unwrap(), o::JobState::Ready);
        let (value, receipt) = job.observe_success().unwrap();
        assert!(receipt.acknowledge().is_ok());
        value
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn ninety_six_actual_sent_handoffs_return_same_writer_and_allow_next_enrollment() {
    let mut f = Box::pin(Fixture::new(false)).await;
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.core.active_account = Some(f.account.id().into());
    shell.core.account_epoch = 7;
    let mut host = lifecycle::Host::default();
    host.seed_registered_for_test(&shell, f.editor.clone())
        .unwrap();
    let selected = host.select(&shell, f.account.clone()).unwrap();
    let count = f.ops().retained().len();
    for index in 0..96 {
        let old = f.editor.clone();
        let (parent, dispatched) = Box::pin(sent(&f)).await;
        let released = old.state.lock().unwrap().released.clone().unwrap();
        // Actual typing after durable release stays in this very same lane.
        old.submit(&local(ORIGINAL)).unwrap();
        old.flush().await.unwrap();
        rendered_finished(&parent, dispatched);
        host.complete_sent(&shell, &f.account, &f.owner.scope().key, &parent)
            .unwrap();
        parent.acknowledge().unwrap(); // Root future removal is a separate renderer test.
        assert!(old.submit(&local("stale editor")).is_err());
        assert!(f.account.retained_editors().unwrap().is_empty());
        assert!(f.account.retained_operations().unwrap().is_empty());
        let opened = host.retained_locals(&f.account).pop().unwrap();
        let lane = host.local(&opened).unwrap();
        assert!(Arc::ptr_eq(lane.writer(), &released));
        assert!(same_local(
            lane.writer()
                .flush()
                .await
                .unwrap()
                .inventory
                .unwrap()
                .local(),
            &local(ORIGINAL)
        ));
        assert_eq!(f.ops().retained().len(), count, "cycle {index}");
        if index == 95 {
            break;
        }
        let inventory = f.owner.local_inventory().await.unwrap();
        let expected = observed(f.ops().enroll(inventory).unwrap()).await;
        load(&mut host, &shell).await;
        let adoption = host
            .begin_adoption(&shell, &selected, &opened, expected, Visit::new())
            .unwrap();
        adoption.ready().await;
        let installed = host
            .install_without_native_for_test(&shell, &adoption)
            .await
            .unwrap();
        f.editor = installed.editor().clone();
        let before = f.owner.review().await.unwrap();
        observed(
            f.ops()
                .verify_local(o::MutationInput {
                    stream: f.stream.clone(),
                    expected: before,
                })
                .unwrap(),
        )
        .await;
        f.editor.refresh_receipt().await.unwrap();
        assert_eq!(host.retained_locals(&f.account).len(), 0);
    }
}

#[tokio::test]
async fn unpublished_or_wrong_scope_handoff_keeps_old_route_and_both_families() {
    let f = Box::pin(Fixture::new(false)).await;
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.core.active_account = Some(f.account.id().into());
    shell.core.account_epoch = 7;
    let mut host = lifecycle::Host::default();
    host.seed_registered_for_test(&shell, f.editor.clone())
        .unwrap();
    let (parent, dispatched) = Box::pin(sent(&f)).await;
    let count = f.ops().retained().len();
    assert!(
        host.complete_sent(&shell, &f.account, &f.owner.scope().key, &parent)
            .is_err()
    );
    assert_eq!(f.ops().retained().len(), count);
    assert!(
        host.installation(&f.account, &f.owner.scope().key)
            .is_some()
    );
    rendered_finished(&parent, dispatched);
    let mut wrong = f.owner.scope().key.clone();
    wrong.channel = "COTHER".into();
    assert!(
        host.complete_sent(&shell, &f.account, &wrong, &parent)
            .is_err()
    );
    assert_eq!(f.ops().retained().len(), count);
    assert!(!f.editor.state.lock().unwrap().retired);
    assert!(host.retained_locals(&f.account).is_empty());
    host.complete_sent(&shell, &f.account, &f.owner.scope().key, &parent)
        .unwrap();
    let after = f.ops().retained().len();
    assert!(
        host.complete_sent(&shell, &f.account, &f.owner.scope().key, &parent)
            .is_err()
    );
    assert_eq!(f.ops().retained().len(), after);
}

#[tokio::test]
async fn managed_typing_observes_applied_result_before_releasing_both_owned_slots() {
    let f = Box::pin(Fixture::new(false)).await;
    let count = f.ops().retained().len();
    for index in 0..128 {
        let value = local(&format!("managed input {index} \u{65e5}\u{1f680}"));
        f.editor.submit(&value).unwrap();
        let snapshot = f.editor.flush().await.unwrap();
        assert!(same_local(snapshot.view.local(), &value));
        assert!(same_local(f.owner.review().await.unwrap().local(), &value));
        assert_eq!(f.ops().retained().len(), count, "input {index}");
    }
}
