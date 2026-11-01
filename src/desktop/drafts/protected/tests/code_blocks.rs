//! Actual isolated worker/Editor and immutable request; server outcome is inert.
//! No native input admission, field clear, HTTP or process-restart claim.
use super::super::{actions::Action, editor::Editor};
use super::*;
use super_platinum_core::{
    FormatMark,
    config::WorkspaceSession,
    drafts::{
        codec,
        sync::{ConsumeKind, Workflow},
    },
    slack::{SlackClient, api, client::RequestBody},
};

async fn fixture(thread: bool) -> Fixture {
    let mut fixture = Fixture::new().await;
    if thread {
        let owner = fixture
            .account
            .owner(DraftKey {
                team: "T1".into(),
                channel: "C1".into(),
                thread: Some("1719800000.000001".into()),
            })
            .unwrap();
        let client = fixture.account.client().unwrap();
        let inventory = owner.local_inventory().await.unwrap();
        let lease = wait(client.renew_local(inventory.clone())).await.unwrap();
        let serial = lease.initial_serial();
        let inventory = match wait(client.write_local(
            lease,
            serial,
            serial + 1,
            inventory.local().clone(),
            local("Original"),
        ))
        .await
        .unwrap()
        {
            durable::LocalWriteResult::Applied(value) => value,
            _ => panic!("thread input must enter the original isolated worker"),
        };
        wait(client.enroll_local(owner.scope().clone(), inventory))
            .await
            .unwrap();
        fixture.owner = owner;
    }
    fixture
}

#[tokio::test]
async fn whole_code_block_owned_worker_retains_uncertain_payload_newer_input_and_read_only_recovery()
 {
    for thread in [false, true] {
        let fixture = fixture(thread).await;
        let client = fixture.account.client().unwrap();
        let mut original = local("日本語 🚀\n\t*literal* <@U1> :party: `tick`");
        let length = original.composer.text.len();
        original.composer.set_selection(0, length);
        original.composer.apply_format(FormatMark::CodeBlock);
        original.composer.reply_broadcast = thread;
        let expected =
            codec::encode_composer(&fixture.owner.scope().key, &original.composer).unwrap();
        let editor = Editor::open(fixture.owner.clone()).await.unwrap();
        editor.submit(&original).unwrap();
        editor.flush().await.unwrap();
        let state = wait(client.stream_state("T1".into())).await.unwrap();
        let stream = wait(client.open_stream("T1".into(), state)).await.unwrap();
        wait(client.verify_local(stream.clone(), fixture.owner.review().await.unwrap()))
            .await
            .unwrap();
        editor.refresh_receipt().await.unwrap();
        let lease = editor
            .prepare_review(Visit::new(), &original)
            .await
            .unwrap();
        let consumed = editor
            .clone()
            .apply_inner(lease, Action::Consume(ConsumeKind::Send), || {
                Ok(Some(stream.clone()))
            })
            .await
            .unwrap()
            .after;
        assert_eq!(
            consumed.row().state().consumption().unwrap().snapshot,
            expected
        );
        let ticket = wait(client.prepare(stream.clone(), consumed))
            .await
            .unwrap();
        let claimed = wait(client.claim_dispatch(ticket)).await.unwrap();
        let (operation, receipt) = claimed.into_parts();
        assert_eq!(operation.snapshot.as_ref(), Some(&expected));
        let session = WorkspaceSession {
            team_id: "T1".into(),
            enterprise_id: None,
            user_id: "U1".into(),
            name: "Fixture".into(),
            url: "https://fixture.slack.com".into(),
            token: "test-token".into(),
        };
        let request = api::prepare_draft_operation(
            &SlackClient::default(),
            &session,
            &operation,
            1_800_000_000_123,
        )
        .unwrap();
        assert!(request.request.url.contains("chat.postMessage"));
        assert!(!request.request.retry_safe());
        let RequestBody::Form(fields) = &request.request.body else {
            panic!("original form request required")
        };
        let fields = fields
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(fields["blocks"]).unwrap(),
            serde_json::json!(expected.blocks)
        );
        assert_eq!(fields["channel"], "C1");
        if thread {
            assert_eq!(fields["thread_ts"], "1719800000.000001");
            assert_eq!(fields["reply_broadcast"], "true");
        } else {
            assert!(!fields.contains_key("thread_ts") && !fields.contains_key("reply_broadcast"));
        }
        // Hold the original immutable dispatch while the real Editor persists
        // a newer local draft. Recording uncertainty must retain both versions.
        let newer = local("Newer input while the code block is sending 日本語");
        editor.submit(&newer).unwrap();
        editor.flush().await.unwrap();
        let uncertain = wait(client.record(receipt, durable::Outcome::Uncertain, 0))
            .await
            .unwrap();
        assert_eq!(
            uncertain.row().state().workflow(),
            &Workflow::Uncertain(operation.clone())
        );
        assert!(same_local(uncertain.local(), &newer));
        assert_eq!(
            uncertain.row().state().consumption().unwrap().snapshot,
            expected
        );
        assert!(
            wait(client.prepare(stream.clone(), uncertain.clone()))
                .await
                .is_err(),
            "uncertainty must never dispatch another send"
        );
        drop(editor);
        let reopened = Editor::open(fixture.owner.clone()).await.unwrap();
        assert!(same_local(reopened.snapshot().view.local(), &newer));
        let review = fixture.owner.review().await.unwrap();
        assert_eq!(review.row().state().pending(), Some(&operation));
        let projected = codec::decode(
            &fixture.owner.scope().key,
            operation.snapshot.as_ref().unwrap(),
        )
        .unwrap();
        assert_eq!(projected.composer().text, original.composer.text);
        assert_eq!(
            projected.composer().reply_broadcast,
            original.composer.reply_broadcast
        );
        let receipt = fixture.owner.status_receipt(review.clone()).await.unwrap();
        assert_eq!(receipt.operation(), &operation);
        let recovery = fixture.owner.take_over().await.unwrap();
        let ticket = wait(client.reserve_check(recovery.clone(), review, receipt, 1_000_000))
            .await
            .unwrap();
        let completion = match wait(client.claim_check(recovery, ticket, 1_000_000))
            .await
            .unwrap()
        {
            durable::CheckClaim::Ready {
                operation: checked,
                completion,
            } => {
                assert_eq!(checked, operation);
                completion
            }
            _ => panic!("fresh original read-only check unexpectedly deferred"),
        };
        let checked = wait(client.complete_check(
            completion,
            durable::CheckResult::Nonpositive(durable::KnownReadFailure::Unavailable),
            0,
        ))
        .await
        .unwrap();
        assert!(!checked.positive_accepted());
        let after = fixture.owner.review().await.unwrap();
        assert!(same_local(after.local(), &newer));
        assert_eq!(after.row().state().pending(), Some(&operation));
        assert_eq!(
            after.row().state().consumption().unwrap().snapshot,
            expected
        );
        let retained = codec::decode(&fixture.owner.scope().key, &expected).unwrap();
        assert_eq!(retained.composer().text, original.composer.text);
        assert_eq!(
            retained.composer().reply_broadcast,
            original.composer.reply_broadcast
        );
    }
}
