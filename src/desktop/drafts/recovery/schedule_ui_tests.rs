use super::*;
use crate::drafts::recovery::schedule_tests::{Fixture, local, positive};
use super_platinum_core::scheduled::DraftFilter;

fn shell(op: &Operation, view: View) -> ShellState {
    let mut s = ShellState::fixture_variant(crate::media::MediaRegistry::default(), "drafts-empty");
    s.core.active_account = Some(op.scope.account_id.clone());
    s.core.active_team = Some(op.scope.key.team.clone());
    s.core.account_epoch = 7;
    s.managed_drafts = session::Session {
        account: Some(op.scope.account_id.clone()),
        epoch: 7,
        ready: true,
        ..Default::default()
    };
    s.managed_drafts
        .apply_view(&op.scope.account_id, 7, op.scope.key.clone(), view);
    s.core.composer.text = "Current channel input".into();
    s.core.thread_composer.text = "Current thread input".into();
    s.core.scheduled.ensure_scope(7, &op.scope.key.team);
    s.core.scheduled.filter = DraftFilter::Drafts;
    s.main_view = crate::state::MainView::Scheduled;
    s
}

fn transcripts(shell: &ShellState, team: &str) -> serde_json::Value {
    serde_json::to_value(
        shell.core.workspaces[team]
            .messages
            .iter()
            .map(|(channel, bag)| {
                (
                    channel.clone(),
                    serde_json::json!({"messages":bag.messages,"pending":bag.pending,
            "deleted":bag.remote_deletions,"unread":bag.unread_count}),
                )
            })
            .collect::<BTreeMap<_, _>>(),
    )
    .unwrap()
}

async fn confirmed(f: &Fixture, op: &Operation) -> Checked {
    let ticket = f.owner.reserve_check(op, 900_000).await.unwrap();
    f.owner
        .finish_check_with(op.clone(), ticket, || async { positive(op) })
        .await
        .unwrap()
}

#[tokio::test]
async fn confirmed_schedule_projects_queue_after_storage_without_chat_or_input_changes() {
    let (f, op) = Fixture::new().await;
    let mut s = shell(&op, f.owner.load().await.unwrap());
    let messages = transcripts(&s, &op.scope.key.team);
    let mut checked = confirmed(&f, &op).await;
    assert!(checked.confirmed);
    let queue = QueueVisit::capture(&s, &op, 7);
    let lineage = Lineage::capture(&op, &checked);
    assert!(project(
        &mut s,
        &op,
        7,
        &None,
        &mut checked,
        &queue,
        &lineage
    ));
    let item = &s.core.scheduled.items[op.base.as_ref().unwrap().id.as_str()];
    assert_eq!(item.post_at, 500);
    assert_eq!(item.blocks, op.snapshot.as_ref().unwrap().blocks);
    assert_eq!(s.core.scheduled.filter, DraftFilter::Drafts);
    assert_eq!(s.core.composer.text, "Current channel input");
    assert_eq!(s.core.thread_composer.text, "Current thread input");
    assert_eq!(transcripts(&s, &op.scope.key.team), messages);
    assert!(
        s.managed_drafts
            .entry(&op.scope.account_id, &op.scope.key)
            .is_some()
    );
    assert!(f.owner.load().await.unwrap().stored.is_some());
}

#[tokio::test]
async fn receipt_preserves_newer_or_retired_queue_and_equal_conflicting_revision() {
    for scenario in 0..4 {
        let (f, op) = Fixture::new().await;
        let mut s = shell(&op, f.owner.load().await.unwrap());
        let mut checked = confirmed(&f, &op).await;
        let mut competing = scheduled_message(&op, &checked.read).unwrap();
        competing.blocks = vec![serde_json::json!({"type":"rich_text","elements":[]})];
        match scenario {
            0 => competing.last_updated_ts = "200.000001".into(),
            1 => competing.is_sent = true,
            2 => {}
            _ => competing.last_updated_ts.clear(),
        }
        s.core.scheduled.accept_remote(competing.clone());
        let generation = s.core.scheduled.generation;
        let queue = QueueVisit::capture(&s, &op, 7);
        let lineage = Lineage::capture(&op, &checked);
        assert!(project(
            &mut s,
            &op,
            7,
            &None,
            &mut checked,
            &queue,
            &lineage
        ));
        assert_eq!(s.core.scheduled.generation, generation);
        if scenario == 1 {
            assert!(s.core.scheduled.items.is_empty());
        } else {
            assert_eq!(
                s.core.scheduled.items[&competing.id].blocks,
                competing.blocks
            );
        }
    }
}

#[tokio::test]
async fn scope_or_new_editor_prevents_all_schedule_projection() {
    for scenario in 0..5 {
        let (f, op) = Fixture::new().await;
        let mut s = shell(&op, f.owner.load().await.unwrap());
        let mut checked = confirmed(&f, &op).await;
        let queue = QueueVisit::capture(&s, &op, 7);
        let lineage = Lineage::capture(&op, &checked);
        match scenario {
            0 => s.core.active_account = Some("different".into()),
            1 => s.core.account_epoch += 1,
            2 => s.core.active_team = Some("Tother".into()),
            3 => s.managed_drafts.epoch += 1,
            _ => {
                s.draft_writes
                    .writers
                    .editor(
                        &f.owner.account,
                        7,
                        op.scope.key.clone(),
                        checked.view.clone(),
                    )
                    .unwrap();
            }
        }
        assert!(!project(
            &mut s,
            &op,
            7,
            &None,
            &mut checked,
            &queue,
            &lineage
        ));
        assert!(s.core.scheduled.items.is_empty());
        assert_eq!(s.core.composer.text, "Current channel input");
    }
}

#[tokio::test]
async fn editor_pending_input_and_writer_replacement_are_preserved() {
    let (f, op) = Fixture::new().await;
    let initial = f.owner.load().await.unwrap();
    let mut s = shell(&op, initial.clone());
    let editor = s
        .draft_writes
        .writers
        .editor(&f.owner.account, 7, op.scope.key.clone(), initial)
        .unwrap();
    let ticket = f.owner.reserve_check(&op, 900_000).await.unwrap();
    let write = editor.clone();
    let mut checked = f
        .owner
        .finish_check_with(op.clone(), ticket, || async {
            write.submit(local("Typed during schedule check"));
            write.flush().await.unwrap();
            positive(&op)
        })
        .await
        .unwrap();
    let refreshed = editor.refresh_receipt().await.unwrap();
    assert_eq!(
        refreshed.view.local.composer.text,
        "Typed during schedule check"
    );
    let queue = QueueVisit::capture(&s, &op, 7);
    let lineage = Lineage::capture(&op, &checked);
    editor.submit(local("Typed after receipt"));
    editor.flush().await.unwrap();
    assert_eq!(
        f.owner.load().await.unwrap().local.composer.text,
        "Typed after receipt"
    );
    assert!(project(
        &mut s,
        &op,
        7,
        &Some(editor.clone()),
        &mut checked,
        &queue,
        &lineage
    ));
    assert_eq!(s.core.composer.text, "Current channel input");
    s.draft_writes
        .writers
        .remove(&op.scope.account_id, 7, &op.scope.key);
    assert!(!project(
        &mut s,
        &op,
        7,
        &Some(editor),
        &mut checked,
        &queue,
        &lineage
    ));
}

#[tokio::test]
async fn uncertain_schedule_feedback_never_claims_delivery_or_projects_queue() {
    let (f, op) = Fixture::new().await;
    let mut s = shell(&op, f.owner.load().await.unwrap());
    let ticket = f.owner.reserve_check(&op, 900_000).await.unwrap();
    let mut checked = f
        .owner
        .finish_check_with(op.clone(), ticket, || async {
            Readback {
                outcome: None,
                raw: Some(serde_json::json!({"ok":false,"error":"draft_not_found"})),
                issue: None,
                cooldown_until_ms: None,
                requested: true,
                has_more: false,
            }
        })
        .await
        .unwrap();
    assert!(!checked.confirmed);
    let queue = QueueVisit::capture(&s, &op, 7);
    let lineage = Lineage::capture(&op, &checked);
    assert!(project(
        &mut s,
        &op,
        7,
        &None,
        &mut checked,
        &queue,
        &lineage
    ));
    assert!(s.core.scheduled.items.is_empty());
    assert_eq!(
        operation(
            s.managed_drafts
                .entry(&op.scope.account_id, &op.scope.key)
                .unwrap()
        ),
        Some(op.clone())
    );
    assert!(!feedback(&checked).contains("Delivery"));
    assert!(inline_feedback(
        &s,
        &op,
        7,
        false,
        feedback(&checked),
        &mut |_| true
    ));
    assert!(!inline_feedback(
        &s,
        &op,
        7,
        false,
        feedback(&checked),
        &mut |_| false
    ));
    assert!(!inline_feedback(
        &s,
        &op,
        7,
        true,
        feedback(&checked),
        &mut |_| panic!("confirmed row")
    ));
}

#[tokio::test]
async fn projection_requires_exact_raw_schedule_receipt() {
    let (f, op) = Fixture::new().await;
    let checked = confirmed(&f, &op).await;
    assert!(scheduled_message(&op, &checked.read).is_some());
    for field in [
        "id",
        "client_msg_id",
        "date_scheduled",
        "last_updated_ts",
        "blocks",
        "destinations",
        "is_sent",
    ] {
        let mut read = checked.read.clone();
        read.raw.as_mut().unwrap()["draft"][field] = serde_json::json!(true);
        assert!(scheduled_message(&op, &read).is_none(), "{field}");
    }
}

#[tokio::test]
async fn queue_scope_reset_suppresses_resurrection_but_keeps_confirmed_managed_local() {
    let (f, op) = Fixture::new().await;
    let mut s = shell(&op, f.owner.load().await.unwrap());
    let queue = QueueVisit::capture(&s, &op, 7);
    let mut checked = confirmed(&f, &op).await;
    let lineage = Lineage::capture(&op, &checked);
    s.core.scheduled.ensure_scope(7, "Tother");
    s.core.scheduled.ensure_scope(7, &op.scope.key.team);
    assert!(project(
        &mut s,
        &op,
        7,
        &None,
        &mut checked,
        &queue,
        &lineage
    ));
    assert!(s.core.scheduled.items.is_empty());
    let current = s
        .managed_drafts
        .entry(&op.scope.account_id, &op.scope.key)
        .unwrap();
    assert!(clean_local(current));
    assert!(f.owner.load().await.unwrap().stored.is_some());
}

#[tokio::test]
async fn late_completion_never_releases_or_overwrites_a_later_intent() {
    let (f, op) = Fixture::new().await;
    let mut s = shell(&op, f.owner.load().await.unwrap());
    let queue = QueueVisit::capture(&s, &op, 7);
    let mut checked = confirmed(&f, &op).await;
    let lineage = Lineage::capture(&op, &checked);
    let edited = f
        .owner
        .checkpoint(
            checked.view.stored.as_ref().unwrap().revision,
            local("Later independent intent"),
        )
        .await
        .unwrap();
    let next = f
        .owner
        .consume(
            edited.revision,
            super_platinum_core::drafts::sync::ConsumeKind::Send,
        )
        .await
        .unwrap();
    let intent = next.state.consumption().unwrap().clone();
    let current = f.owner.load().await.unwrap();
    s.managed_drafts
        .apply_view(&op.scope.account_id, 7, op.scope.key.clone(), current);
    assert!(!project(
        &mut s,
        &op,
        7,
        &None,
        &mut checked,
        &queue,
        &lineage
    ));
    assert_eq!(
        f.owner
            .load()
            .await
            .unwrap()
            .stored
            .unwrap()
            .state
            .consumption(),
        Some(&intent)
    );
    assert_eq!(
        s.managed_drafts
            .entry(&op.scope.account_id, &op.scope.key)
            .unwrap()
            .stored
            .as_ref()
            .unwrap()
            .state
            .consumption(),
        Some(&intent)
    );
}

#[tokio::test]
async fn released_and_reenrolled_destination_is_not_overwritten_by_old_checked_revision() {
    let (f, op) = Fixture::new().await;
    let mut s = shell(&op, f.owner.load().await.unwrap());
    let queue = QueueVisit::capture(&s, &op, 7);
    let mut checked = confirmed(&f, &op).await;
    let lineage = Lineage::capture(&op, &checked);
    let edited = f
        .owner
        .checkpoint(
            checked.view.stored.as_ref().unwrap().revision,
            local("Fresh enrollment"),
        )
        .await
        .unwrap();
    f.owner.release(edited.revision).await.unwrap();
    let fresh = f
        .owner
        .enroll(local("Fresh enrollment"), None)
        .await
        .unwrap();
    assert_ne!(
        fresh.state.client_msg_id(),
        lineage.successor.as_deref().unwrap()
    );
    assert!(fresh.revision < checked.view.stored.as_ref().unwrap().revision);
    let old = s
        .managed_drafts
        .entry(&op.scope.account_id, &op.scope.key)
        .unwrap()
        .stored
        .as_ref()
        .unwrap()
        .revision;
    s.managed_drafts
        .remove_entry(&op.scope.account_id, 7, &op.scope.key, old);
    s.managed_drafts.apply_view(
        &op.scope.account_id,
        7,
        op.scope.key.clone(),
        f.owner.load().await.unwrap(),
    );
    assert!(!project(
        &mut s,
        &op,
        7,
        &None,
        &mut checked,
        &queue,
        &lineage
    ));
    assert_eq!(
        s.managed_drafts
            .entry(&op.scope.account_id, &op.scope.key)
            .unwrap()
            .stored
            .as_ref()
            .unwrap()
            .state,
        fresh.state
    );
}

#[tokio::test]
async fn equal_revision_conflicting_state_refuses_metadata_projection() {
    let (f, op) = Fixture::new().await;
    let mut s = shell(&op, f.owner.load().await.unwrap());
    let queue = QueueVisit::capture(&s, &op, 7);
    let mut checked = confirmed(&f, &op).await;
    let lineage = Lineage::capture(&op, &checked);
    let mut changed = checked.view.clone();
    changed.stored.as_mut().unwrap().state.edit(
        &op.scope,
        Some(codec::encode(&op.scope.key, "Other current edit").unwrap()),
    );
    s.managed_drafts.apply_view(
        &op.scope.account_id,
        7,
        op.scope.key.clone(),
        changed.clone(),
    );
    assert!(!project(
        &mut s,
        &op,
        7,
        &None,
        &mut checked,
        &queue,
        &lineage
    ));
    assert_eq!(
        s.managed_drafts
            .entry(&op.scope.account_id, &op.scope.key)
            .unwrap()
            .stored
            .as_ref()
            .unwrap()
            .state,
        changed.stored.unwrap().state
    );
}
