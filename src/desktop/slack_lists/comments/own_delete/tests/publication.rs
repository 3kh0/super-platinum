//! Mounted signal tests for the actual controller adoption/settlement boundary.
//! Included beneath run.rs so no production visibility is broadened for tests.
use super::super::tests as host;
use super::*;
use dioxus::prelude::*;
use host::{TS, durable::Local};
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

fn mount(shell: ShellState) -> (VirtualDom, Signal<ShellState>) {
    type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
    type Input = (Rc<RefCell<Option<ShellState>>>, Slot);
    let slot = Slot::default();
    let input = Rc::new(RefCell::new(Some(shell)));
    let mut dom = VirtualDom::new_with_props(
        |(input, slot): Input| {
            let state = use_signal(|| input.borrow_mut().take().unwrap());
            slot.set(Some(state));
            VNode::empty()
        },
        (input, slot.clone()),
    );
    dom.rebuild_to_vec();
    (dom, slot.get().unwrap())
}

#[test]
fn actual_confirm_entry_refuses_duplicate_busy_unknown_and_foreign_id_without_new_read() {
    let (mut shell, source, policy) = host::setup();
    let record = host::intent(&shell, &source, &policy).record().clone();
    let id = host::install(&mut shell, source, record);
    session_mut(&mut shell, id).unwrap().busy = true;
    let original_read = shell.slack_lists.comments.entry.as_ref().unwrap().read;
    let (_dom, mut state) = mount(shell);
    dispatch::confirm(state, id);
    dispatch::confirm(state, id);
    assert!(session(&state.peek(), id).unwrap().busy);
    session_mut(&mut state.write(), id).unwrap().busy = false;
    session_mut(&mut state.write(), id).unwrap().phase = Some(DeletePhase::Uncertain);
    dispatch::confirm(state, id);
    session_mut(&mut state.write(), id).unwrap().phase = None;
    dispatch::confirm(state, id.wrapping_add(1));
    assert_eq!(
        state
            .peek()
            .slack_lists
            .comments
            .entry
            .as_ref()
            .unwrap()
            .read,
        original_read
    );
    assert!(!session(&state.peek(), id).unwrap().busy);
    assert!(session(&state.peek(), id).unwrap().recovery.is_none());
}

#[tokio::test]
async fn retained_ack_publishes_only_after_local_commit_and_duplicate_does_not_retire_new_read() {
    let (mut shell, source, policy) = host::setup();
    let intent = host::intent(&shell, &source, &policy);
    let record = intent.record().clone();
    let id = host::install(&mut shell, source.clone(), record.clone());
    let local = Local::new();
    let recovery = Arc::new(
        Recovery::load(local.disk(&record), TS.into())
            .await
            .unwrap(),
    );
    let attempt = recovery.begin(&record).await.unwrap();
    session_mut(&mut shell, id).unwrap().recovery = Some(recovery.clone());
    let raw = json!({"ok":true,"channel":source.target.channel_id(),"ts":TS,"opaque":"immutable"});
    let ack = Arc::new(intent.accept(raw.clone()).unwrap());
    let original = std::fs::read(&local.0).unwrap();
    std::fs::write(&local.0, b"temporary invalid test database").unwrap();
    assert!(
        recovery
            .resolve(attempt, Outcome::Confirmed(ack), 0)
            .await
            .is_err()
    );
    let (_dom, mut state) = mount(shell);
    adopt(state, id, &recovery, None).await;
    assert_eq!(
        state
            .peek()
            .slack_lists
            .comments
            .entry
            .as_ref()
            .unwrap()
            .rows
            .len(),
        1
    );
    assert!(presentation(&state.peek()).unwrap().can_retry_local);
    let page = state
        .peek()
        .slack_lists
        .comments
        .entry
        .as_ref()
        .unwrap()
        .page
        .as_ref()
        .unwrap()
        .raw()
        .clone();
    let (owner, read, _, _) = controller::begin(&mut state.write(), false).unwrap();
    std::fs::write(&local.0, original).unwrap();
    let outcome = recovery.flush().await.unwrap();
    adopt(state, id, &recovery, outcome).await;
    assert!(!owner.reading(&state.peek(), read));
    {
        let shell = state.peek();
        let e = shell.slack_lists.comments.entry.as_ref().unwrap();
        assert!(e.rows.is_empty() && e.deleted.contains(TS));
        assert_eq!(e.page.as_ref().unwrap().raw(), &page);
        assert!(presentation(&shell).unwrap().terminal);
    }
    assert_eq!(
        recovery.inventory().await.attempt().unwrap().raw(),
        Some(&raw)
    );
    let (owner, read, _, _) = controller::begin(&mut state.write(), false).unwrap();
    let rows = state
        .peek()
        .slack_lists
        .comments
        .entry
        .as_ref()
        .unwrap()
        .rows
        .clone();
    adopt(state, id, &recovery, None).await;
    assert!(owner.reading(&state.peek(), read));
    assert!(Arc::ptr_eq(
        &rows,
        &state
            .peek()
            .slack_lists
            .comments
            .entry
            .as_ref()
            .unwrap()
            .rows
    ));
}

#[tokio::test]
async fn confirmed_original_receipt_does_not_remove_changed_body_socket_or_account_successor() {
    for mutation in 0..3 {
        let (mut shell, source, policy) = host::setup();
        let intent = host::intent(&shell, &source, &policy);
        let record = intent.record().clone();
        let id = host::install(&mut shell, source.clone(), record.clone());
        let local = Local::new();
        let recovery = Recovery::load(local.disk(&record), TS.into())
            .await
            .unwrap();
        let attempt = recovery.begin(&record).await.unwrap();
        let raw = json!({"ok":true,"channel":source.target.channel_id(),"ts":TS});
        let outcome = recovery
            .resolve(
                attempt,
                Outcome::Confirmed(Arc::new(intent.accept(raw.clone()).unwrap())),
                0,
            )
            .await
            .unwrap();
        match mutation {
            0 => host::change_page(&mut shell, |raw| {
                raw["messages"][1]["blocks"] = json!(host::rich("Newer body"))
            }),
            1 => shell.core.workspaces.get_mut("T1").unwrap().rt_generation += 1,
            _ => shell.core.account_epoch += 1,
        }
        let before = shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .unwrap()
            .page
            .as_ref()
            .unwrap()
            .raw()
            .clone();
        let (_dom, state) = mount(shell);
        adopt(state, id, &recovery, outcome).await;
        let shell = state.peek();
        let e = shell.slack_lists.comments.entry.as_ref().unwrap();
        assert_eq!(e.rows.len(), 1);
        assert!(e.deleted.is_empty());
        assert_eq!(e.page.as_ref().unwrap().raw(), &before);
        drop(shell);
        assert_eq!(
            recovery.inventory().await.attempt().unwrap().raw(),
            Some(&raw)
        );
    }
}

#[tokio::test]
async fn socket_before_ack_keeps_tombstone_and_does_not_cancel_later_read_twice() {
    let (mut shell, source, policy) = host::setup();
    let intent = host::intent(&shell, &source, &policy);
    let record = intent.record().clone();
    let id = host::install(&mut shell, source.clone(), record.clone());
    let local = Local::new();
    let recovery = Recovery::load(local.disk(&record), TS.into())
        .await
        .unwrap();
    let attempt = recovery.begin(&record).await.unwrap();
    let event = super_platinum_core::slack::events::RtEvent::MessageDeleted {
        channel: source.target.channel_id().into(),
        deleted_ts: TS.into(),
    };
    observe_event(&mut shell, "T1", 7, &event);
    assert!(deletions::apply(&mut shell, "T1", 7, &event));
    let (owner, read, _, _) = controller::begin(&mut shell, false).unwrap();
    let raw = json!({"ok":true,"channel":source.target.channel_id(),"ts":TS});
    let outcome = recovery
        .resolve(
            attempt,
            Outcome::Confirmed(Arc::new(intent.accept(raw).unwrap())),
            0,
        )
        .await
        .unwrap();
    let (_dom, state) = mount(shell);
    adopt(state, id, &recovery, outcome).await;
    assert!(owner.reading(&state.peek(), read));
    assert!(
        state
            .peek()
            .slack_lists
            .comments
            .entry
            .as_ref()
            .unwrap()
            .deleted
            .contains(TS)
    );
    assert!(presentation(&state.peek()).unwrap().terminal);
}

#[test]
fn settlement_retains_unknown_receipts_and_typed_429_retry_metadata_without_false_ack() {
    let (outcome, deadline) = dispatch::settle(Err(ListCommentDeleteFailure::NotRequested(
        Error::Offline("closed".into()),
    )));
    assert!(matches!(outcome, Outcome::NotRequested));
    assert_eq!(deadline, 0);
    let before = now();
    let (outcome, deadline) = dispatch::settle(Err(ListCommentDeleteFailure::Unconfirmed {
        error: Error::RateLimited {
            retry_after_secs: None,
        },
        receipt: None,
    }));
    assert!(matches!(outcome, Outcome::RateLimited));
    assert!(deadline >= before.max(0) as u64 + 30);
    let raw = json!({"ok":false,"error":"internal_error","opaque":1});
    let (outcome, deadline) = dispatch::settle(Err(ListCommentDeleteFailure::Unconfirmed {
        error: Error::Api("internal_error".into()),
        receipt: Some(raw.clone()),
    }));
    assert_eq!(deadline, 0);
    assert!(matches!(outcome,Outcome::Uncertain(Some(value)) if value==raw));
    let raw = json!({"ok":false,"error":"cant_delete_message"});
    let (outcome, _) = dispatch::settle(Err(ListCommentDeleteFailure::Rejected(raw.clone())));
    assert!(matches!(outcome,Outcome::Rejected(value) if value==raw));
}

#[tokio::test]
async fn persisted_rate_limit_adoption_preserves_guidance_and_refuses_duplicate_confirm() {
    rate_limit_adoption(false, false).await;
    rate_limit_adoption(false, true).await;
}

#[tokio::test]
async fn expired_persisted_rate_limit_enables_explicit_action_without_replaying_attempt() {
    rate_limit_adoption(true, false).await;
}

async fn rate_limit_adoption(expired: bool, longer_scope_cooldown: bool) {
    let (mut shell, source, policy) = host::setup();
    let record = host::intent(&shell, &source, &policy).record().clone();
    let local = Local::new();
    let disk = local.disk(&record);
    let initial = Recovery::load(disk.clone(), TS.into()).await.unwrap();
    let attempt_id = initial.begin(&record).await.unwrap();
    let deadline = if expired {
        (now().max(0) as u64).saturating_sub(1)
    } else {
        now().max(0) as u64 + 3600
    };
    initial
        .resolve(attempt_id.clone(), Outcome::RateLimited, deadline)
        .await
        .unwrap();
    let committed = initial.inventory().await;
    drop(initial);

    // Reconstruct from the isolated database, not a retained in-memory result.
    let recovery = Arc::new(Recovery::load(disk.clone(), TS.into()).await.unwrap());
    assert_eq!(recovery.inventory().await, committed);
    assert_eq!(committed.attempt().unwrap().id(), attempt_id);
    assert_eq!(
        committed.attempt().unwrap().phase(),
        DeletePhase::RateLimited
    );
    assert!(committed.attempt().unwrap().raw().is_none());
    let scoped_deadline = deadline + u64::from(longer_scope_cooldown) * 60;
    if longer_scope_cooldown {
        shell.slack_lists.comments.cooldown =
            Some((source.owner.scope.clone(), scoped_deadline as i64));
    }
    let id = host::install(&mut shell, source, record.clone());
    let active = session_mut(&mut shell, id).unwrap();
    active.busy = true;
    active.record = None;
    active.recovery = Some(recovery.clone());
    let entry = shell.slack_lists.comments.entry.as_ref().unwrap();
    let read = entry.read;
    let loading = entry.loading;
    let rows = entry.rows.clone();
    let page = entry.page.as_ref().unwrap().raw().clone();
    let (dom, state) = mount(shell);
    let mut adoption = std::pin::pin!(adopt(state, id, &recovery, None));
    std::future::poll_fn(|cx| dom.in_runtime(|| std::future::Future::poll(adoption.as_mut(), cx)))
        .await;

    let guidance =
        "Slack asked you to wait. Your drafts are retained; no request is automatically retried.";
    let view = presentation(&state.peek()).unwrap();
    assert_eq!(view.notice.as_deref(), Some(guidance));
    assert_eq!(view.can_confirm, expired);
    assert!(!view.busy && !view.can_check && !view.can_retry_local && !view.terminal);
    if !expired {
        // Exercise the actual entry point twice. A missing availability guard
        // would mark busy and spawn preflight, not merely change a derived flag.
        dom.in_runtime(|| {
            dispatch::confirm(state, id);
            dispatch::confirm(state, id);
        });
        assert_eq!(cooldown(&state.peek()), scoped_deadline as i64);
        assert_eq!(
            state
                .peek()
                .slack_lists
                .comments
                .entry
                .as_ref()
                .unwrap()
                .retry_at,
            scoped_deadline as i64,
            "the original entry must arm expiry at the longest owned cooldown"
        );
    }
    // Expiry authorizes a new explicit action only. Do not click after expiry;
    // adoption itself must not issue preflight or replace the persisted UUID.
    tokio::task::yield_now().await;
    {
        let shell = state.peek();
        let active = session(&shell, id).unwrap();
        assert!(!active.busy && !active.local_pending);
        assert_eq!(active.phase, Some(DeletePhase::RateLimited));
        assert_eq!(active.retry_at, deadline);
        assert_eq!(active.record.as_ref(), Some(&record));
        assert_eq!(active.notice.as_deref(), Some(guidance));
        let entry = shell.slack_lists.comments.entry.as_ref().unwrap();
        assert_eq!((entry.read, entry.loading), (read, loading));
        assert!(Arc::ptr_eq(&entry.rows, &rows));
        assert_eq!(entry.page.as_ref().unwrap().raw(), &page);
        assert!(entry.deleted.is_empty());
    }
    assert!(!recovery.pending().await);
    assert_eq!(recovery.inventory().await, committed);
    let reloaded = Recovery::load(disk, TS.into()).await.unwrap();
    assert_eq!(reloaded.inventory().await, committed);
}
