use super::*;
use crate::drafts::checkpoint::RemoteReviewFixture;
use super_platinum_core::ComposerState;
use super_platinum_core::drafts::store::sync::LocalChoice;
fn local(text: &str) -> LocalDraft {
    LocalDraft {
        composer: ComposerState::with_text(text),
        attachments: vec![],
    }
}

#[test]
fn selection_and_broadcast_changes_invalidate_the_complete_reviewed_predecessor() {
    let original = local("same text");
    let mut changed = original.clone();
    changed.composer.set_selection(0, 1);
    assert!(!plain_same(&original, &changed));
    changed = original.clone();
    changed.composer.reply_broadcast = true;
    assert!(!plain_same(&original, &changed));
}

#[test]
fn missing_book_and_explicit_empty_book_are_distinct_projection_predecessors() {
    let empty = local("");
    let absent = Pair {
        live: empty.clone(),
        book: None,
    };
    let present = Pair {
        live: empty.clone(),
        book: Some(empty),
    };
    assert!(!absent.same(&present));
    assert!(absent.same(&absent.clone()));
}

#[test]
fn an_attachment_never_becomes_plain_authority_through_lossy_serialization() {
    let mut draft = local("file");
    draft.attachments.push(serde_json::from_value(serde_json::json!({"id":1,"path":"fixture-only.txt","name":"fixture-only.txt","bytes":8})).unwrap());
    let mut changed = draft.clone();
    changed.attachments[0].uploading = true;
    assert_eq!(
        serde_json::to_value(&draft).unwrap(),
        serde_json::to_value(&changed).unwrap()
    );
    assert!(!plain_same(&draft, &changed));
    assert!(!plain_same(&draft, &draft));
}

#[tokio::test]
async fn selected_ui_remote_row_commits_exact_snapshot_and_stays_parked() {
    let fixture = RemoteReviewFixture::new().await;
    let foreground = local("local");
    let (lease, review) = fixture.prepare(foreground.clone()).await;
    let rows = choices::build(&review);
    let choice = rows
        .iter()
        .find(|row| row.choice == Some(RemoteChoice::Remote))
        .unwrap()
        .choice
        .clone()
        .unwrap();
    let expected = review.remote().snapshot.clone();
    let barrier = commit_choice(&fixture.editor, lease, review, choice)
        .await
        .unwrap();
    assert!(plain_same(&foreground, &barrier.before.local));
    assert_eq!(
        barrier.after.as_ref().unwrap().local.composer.text,
        "remote"
    );
    let inventory = fixture.editor.owner().review_local().await.unwrap();
    assert_eq!(inventory.stored.unwrap().state.desired(), Some(&expected));
    assert_eq!(fixture.editor.snapshot().error, Some(Fault::Stale));
    assert!(Arc::ptr_eq(
        fixture.editor.snapshot().projection.as_ref().unwrap(),
        &barrier
    ));
    // A usable ticket is not an acknowledgment. No view code or fabricated
    // receipt in this test can unblock the writer or mutate the foreground.
    assert!(fixture.editor.prepare_projection(&barrier.id).await.is_ok());
    assert_eq!(foreground.composer.text, "local");
    assert!(fixture.editor.snapshot().projection.is_some());
}

#[tokio::test]
async fn type_then_undo_cannot_apply_an_old_ui_selection_even_with_equal_text() {
    let fixture = RemoteReviewFixture::new().await;
    let (lease, review) = fixture.prepare(local("local")).await;
    let selected = choices::build(&review)
        .last()
        .unwrap()
        .choice
        .clone()
        .unwrap();
    fixture.editor.submit(local("new input"));
    fixture.editor.flush().await.unwrap();
    fixture.editor.submit(local("local"));
    fixture.editor.flush().await.unwrap();
    assert!(!fixture.editor.remote_review_current(&lease));
    assert!(matches!(
        commit_choice(&fixture.editor, lease, review, selected).await,
        Err(Fault::Stale)
    ));
    let inventory = fixture.editor.owner().review_local().await.unwrap();
    assert_eq!(inventory.local.composer.text, "local");
    assert_eq!(
        inventory
            .stored
            .unwrap()
            .state
            .acknowledged()
            .unwrap()
            .last_updated_ts,
        "10.000001"
    );
    assert!(fixture.editor.snapshot().projection.is_none());
}

#[tokio::test]
async fn second_ui_review_selects_exact_preserved_writer_and_renews_predecessor() {
    let fixture = RemoteReviewFixture::new().await;
    let (lease, review) = fixture.prepare(local("local")).await;
    let first = commit_choice(&fixture.editor, lease, review, RemoteChoice::Remote)
        .await
        .unwrap();
    let newer = local("new visible input");
    fixture.editor.submit(newer.clone());
    assert!(fixture.editor.flush().await.is_err());
    let (lease, review) = fixture.prepare(newer.clone()).await;
    let rows = choices::build(&review);
    let row = rows
        .iter()
        .find(|row| {
            matches!(
                &row.choice,
                Some(RemoteChoice::Local(LocalChoice::Alternate { .. }))
            )
        })
        .unwrap();
    assert!(row.unavailable.is_none());
    let choice = row.choice.clone().unwrap();
    assert!(plain_same(
        review
            .inventory()
            .chosen_local(match &choice {
                RemoteChoice::Local(local) => local,
                _ => unreachable!(),
            })
            .unwrap(),
        &newer
    ));
    let second = commit_choice(&fixture.editor, lease, review, choice)
        .await
        .unwrap();
    assert_ne!(first.id, second.id);
    assert!(plain_same(&second.before.local, &newer));
    assert!(plain_same(&second.after.as_ref().unwrap().local, &newer));
    let inventory = fixture.editor.owner().review_local().await.unwrap();
    assert!(inventory.alternates.is_empty());
    assert!(inventory.blocked.is_none());
    assert!(fixture.editor.prepare_projection(&second.id).await.is_ok());
    assert_eq!(fixture.editor.snapshot().error, Some(Fault::Stale));
}

#[tokio::test]
async fn fabricated_writer_choice_cannot_reach_durable_reconciliation() {
    let fixture = RemoteReviewFixture::new().await;
    let (lease, review) = fixture.prepare(local("local")).await;
    let before = fixture.editor.owner().review_local().await.unwrap();
    let choice = RemoteChoice::Local(LocalChoice::Alternate {
        writer_id: "unreviewed-writer".into(),
        revision: 1,
    });
    assert!(matches!(
        commit_choice(&fixture.editor, lease, review, choice).await,
        Err(Fault::Unsupported)
    ));
    let after = fixture.editor.owner().review_local().await.unwrap();
    assert!(plain_same(&before.local, &after.local));
    assert_eq!(
        before.stored.unwrap().revision,
        after.stored.unwrap().revision
    );
    assert!(fixture.editor.snapshot().projection.is_none());
}

#[tokio::test]
async fn finish_after_dropped_review_reuses_same_choice_without_another_cas() {
    let fixture = RemoteReviewFixture::new().await;
    let (lease, review) = fixture.prepare(local("local")).await;
    let chosen = commit_choice(&fixture.editor, lease, review, RemoteChoice::Remote)
        .await
        .unwrap();
    let revision = chosen
        .after
        .as_ref()
        .unwrap()
        .stored
        .as_ref()
        .unwrap()
        .revision;
    // No Prepared/lease/read proof remains. A new mounted Visit will separately
    // prove its native predecessor; this half recovers only the same barrier.
    let recovered = finish_barrier(
        &fixture.editor,
        chosen.serial,
        &chosen.scope.account_id,
        &chosen.scope.key,
        &local("local"),
    )
    .unwrap();
    assert!(Arc::ptr_eq(&chosen, &recovered));
    assert!(fixture.editor.projection_finish_available());
    let ticket = fixture
        .editor
        .prepare_projection(&recovered.id)
        .await
        .unwrap();
    drop(ticket); // canceled native capture does not consume the saved choice
    assert!(
        fixture
            .editor
            .prepare_projection(&recovered.id)
            .await
            .is_ok()
    );
    let current = fixture.editor.owner().review_local().await.unwrap();
    assert_eq!(current.stored.unwrap().revision, revision);
    assert_eq!(current.local.composer.text, "remote");
    assert_eq!(fixture.editor.snapshot().error, Some(Fault::Stale));
}

#[tokio::test]
async fn finish_cannot_refresh_barrier_after_new_input_even_if_text_returns() {
    let fixture = RemoteReviewFixture::new().await;
    let (lease, review) = fixture.prepare(local("local")).await;
    let chosen = commit_choice(&fixture.editor, lease, review, RemoteChoice::Remote)
        .await
        .unwrap();
    fixture.editor.submit(local("newer"));
    assert!(fixture.editor.flush().await.is_err());
    fixture.editor.submit(local("local"));
    assert!(fixture.editor.flush().await.is_err());
    assert!(matches!(
        finish_barrier(
            &fixture.editor,
            fixture.editor.snapshot().serial,
            &chosen.scope.account_id,
            &chosen.scope.key,
            &local("local")
        ),
        Err(Fault::Stale)
    ));
    assert!(fixture.editor.prepare_projection(&chosen.id).await.is_err());
    assert!(Arc::ptr_eq(
        fixture.editor.snapshot().projection.as_ref().unwrap(),
        &chosen
    ));
    let current = fixture.editor.owner().review_local().await.unwrap();
    assert_eq!(current.local.composer.text, "remote");
    assert!(
        current
            .alternates
            .iter()
            .any(|row| row.draft.composer.text == "local")
    );
}

#[tokio::test]
async fn explicit_review_preserves_retired_predecessor_then_reviews_fresh_inventory() {
    let fixture = RemoteReviewFixture::new().await;
    let (lease, review) = fixture.prepare(local("local")).await;
    let first = commit_choice(&fixture.editor, lease, review, RemoteChoice::Remote)
        .await
        .unwrap();
    let (lease, proof) = prepare_for_review(
        &fixture.editor,
        first.serial,
        &first.scope.account_id,
        &first.scope.key,
        &local("local"),
        || true,
    )
    .await
    .unwrap();
    let proof = proof.expect("explicit review preserves the retired predecessor");
    assert_eq!(lease.serial(), proof.serial());
    assert_eq!(lease.inventory().local.composer.text, "remote");
    assert!(lease.inventory().contains_local(&local("local")));
    assert!(Arc::ptr_eq(
        fixture.editor.snapshot().projection.as_ref().unwrap(),
        &first
    ));
    assert!(fixture.editor.prepare_projection(&first.id).await.is_err());
    let fresh =
        RemoteReview::prepare(lease.inventory().clone(), fixture.raw.clone(), "U1").unwrap();
    let choice = choices::build(&fresh)
        .into_iter()
        .find(|row| {
            matches!(
                &row.choice,
                Some(RemoteChoice::Local(LocalChoice::Alternate { .. }))
            )
        })
        .unwrap()
        .choice
        .unwrap();
    let second = commit_choice(&fixture.editor, lease, fresh, choice)
        .await
        .unwrap();
    assert_ne!(first.id, second.id);
    assert_eq!(second.before.local.composer.text, "local");
    assert_eq!(second.after.as_ref().unwrap().local.composer.text, "local");
}

#[tokio::test]
async fn review_fallback_cannot_preserve_text_that_was_not_the_barrier_predecessor() {
    let fixture = RemoteReviewFixture::new().await;
    let (lease, review) = fixture.prepare(local("local")).await;
    let first = commit_choice(&fixture.editor, lease, review, RemoteChoice::Remote)
        .await
        .unwrap();
    assert!(matches!(
        prepare_for_review(
            &fixture.editor,
            first.serial,
            &first.scope.account_id,
            &first.scope.key,
            &local("unproven text"),
            || true,
        )
        .await,
        Err(Fault::Stale)
    ));
    let inventory = fixture.editor.owner().review_local().await.unwrap();
    assert!(inventory.alternates.is_empty());
    assert_eq!(inventory.local.composer.text, "remote");
    assert_eq!(fixture.editor.snapshot().serial, first.serial);
    assert!(Arc::ptr_eq(
        fixture.editor.snapshot().projection.as_ref().unwrap(),
        &first
    ));
}

#[tokio::test]
async fn leaving_the_review_while_loading_inventory_prevents_fallback_preservation() {
    let fixture = RemoteReviewFixture::new().await;
    let (lease, review) = fixture.prepare(local("local")).await;
    let first = commit_choice(&fixture.editor, lease, review, RemoteChoice::Remote)
        .await
        .unwrap();
    assert!(matches!(
        prepare_for_review(
            &fixture.editor,
            first.serial,
            &first.scope.account_id,
            &first.scope.key,
            &local("local"),
            || false
        )
        .await,
        Err(Fault::Stale)
    ));
    let inventory = fixture.editor.owner().review_local().await.unwrap();
    assert!(inventory.alternates.is_empty());
    assert_eq!(fixture.editor.snapshot().serial, first.serial);
    assert!(Arc::ptr_eq(
        fixture.editor.snapshot().projection.as_ref().unwrap(),
        &first
    ));
}
