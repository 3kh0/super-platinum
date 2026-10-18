//! Actual isolated owned worker/SQLite and renderer executor. Socket acceptance
//! is synthetic; these are not native capture, HTTP, or end-to-end Send proofs.
use super::*;
use crate::drafts::protected::owned_route::{Held as Job, o};
use dioxus::prelude::*;
use std::{
    cell::{Cell, RefCell},
    future::Future,
    rc::Rc,
};
use super_platinum_core::{
    ComposerState,
    config::{Session, WorkspaceSession},
    slack::{Transport, realtime::flannel_url},
};

fn signal(shell: ShellState) -> (VirtualDom, Signal<ShellState>) {
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
    (dom, slot.get().unwrap())
}
async fn drive<T: 'static>(dom: &mut VirtualDom, future: impl Future<Output = T> + 'static) -> T {
    let (send, mut recv) = tokio::sync::oneshot::channel();
    dom.in_runtime(|| {
        dioxus::core::spawn_forever(async move {
            let _ = send.send(future.await);
        })
    });
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop { tokio::select! { result = &mut recv => break result.unwrap(), _ = dom.wait_for_work() => { dom.render_immediate_to_vec(); } } }
    }).await.unwrap()
}
async fn fixture() -> (
    VirtualDom,
    Signal<ShellState>,
    Worker,
    Arc<observation::Source>,
    Arc<Account>,
    o::Operations,
    DraftKey,
) {
    let initialized = tokio::task::spawn_blocking(|| {
        let route = o::OwnedRoute::fresh_fixture().unwrap();
        let before = route.inspect().unwrap().wait().unwrap();
        route.commit(&before).unwrap().wait().unwrap();
        route
            .inspect()
            .unwrap()
            .wait()
            .unwrap()
            .open()
            .unwrap()
            .initialize_cold(&o::FactoryIdentity::default())
            .unwrap()
            .wait()
            .unwrap()
    })
    .await
    .unwrap();
    let ops = initialized.operations();
    let account = Account::from_retained(initialized);
    let key = DraftKey {
        team: "T1".into(),
        channel: "C1".into(),
        thread: None,
    };
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.core.active_account = Some(account.id().into());
    shell.core.account_epoch = 7;
    shell.core.transport = Some(Arc::new(Transport::new("fixture-only-cookie").unwrap()));
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
    let mut socket = WorkerSource::capture(
        &mut shell,
        ConnectParams {
            team: "T1".into(),
            ws_url: flannel_url("fixture-only-token", "T1"),
            d_cookie: "fixture-only-cookie".into(),
            user_agent: "fixture-only-agent".into(),
        },
    )
    .unwrap();
    socket.take_connect_params(&shell).unwrap();
    shell.core.workspaces.get_mut("T1").unwrap().rt_generation = 9;
    let accepted = Arc::new(socket.accept(&mut shell, "T1", 9).unwrap());
    let worker = Worker {
        socket,
        account: account.clone(),
        team: "T1".into(),
        connection: None,
    };
    let (dom, state) = signal(shell);
    (dom, state, worker, accepted, account, ops, key)
}
async fn local(
    account: &Arc<Account>,
    key: &DraftKey,
    value: &LocalDraft,
) -> durable::LocalInventory {
    let inventory = account
        .owner(key.clone())
        .unwrap()
        .local_inventory()
        .await
        .unwrap();
    let writer = local_writer::LocalWriter::renew(account.clone(), inventory)
        .await
        .unwrap();
    writer.submit(value).unwrap();
    let result = writer.flush().await.unwrap();
    assert!(result.pending.is_none() && result.error.is_none());
    result.inventory.unwrap()
}

#[tokio::test]
async fn first_enrollment_retries_the_same_accepted_socket_and_retains_original_stale_result() {
    let (mut dom, state, worker, accepted, account, ops, key) = fixture().await;
    let scope = Scope {
        account_id: account.id().into(),
        key: key.clone(),
    };
    // The old missing-continuity failure is an actual failed job, not rewritten
    // into success when the later enrolled destination becomes available.
    let old = Job::new(ops.stream_state(o::TeamInput { scope }));
    assert!(matches!(
        old.value().await.unwrap(),
        Err(durable::Error::Stale)
    ));
    let old_key = old.job().unwrap().key();
    let (mut worker, opened) = drive(&mut dom, async move {
        let mut worker = worker;
        let result = worker.connected(state, accepted, 9).await;
        (worker, result)
    })
    .await;
    opened.unwrap();
    let connection = capture(&state.peek(), "T1").unwrap();
    assert!(matches!(
        connection.source(&state.peek()),
        Err(durable::Error::Stale)
    ));
    assert_eq!(
        connection.held.lock().unwrap().no_destination,
        Some(durable::Error::Stale)
    );
    assert!(account.retained_source_openings().unwrap().is_empty());
    let value = LocalDraft {
        composer: ComposerState::with_text("first actual draft"),
        attachments: vec![],
    };
    let inventory = local(&account, &key, &value).await;
    assert!(same_local(inventory.local(), &value));
    assert!(account.inventory().await.unwrap().managed().is_empty());
    let review = Job::new(ops.enroll(inventory))
        .observed_success()
        .await
        .unwrap();
    assert!(same_local(review.local(), &value));
    let next = connection.clone();
    let source = drive(&mut dom, async move { next.ensure(state).await })
        .await
        .unwrap();
    assert!(Arc::ptr_eq(
        &connection,
        &capture(&state.peek(), "T1").unwrap()
    ));
    assert!(source.current(&state.peek()));
    let again = connection.clone();
    let repeated = drive(&mut dom, async move { again.ensure(state).await })
        .await
        .unwrap();
    assert!(Arc::ptr_eq(&source, &repeated));
    assert_eq!(account.retained_source_openings().unwrap().len(), 1);
    assert_eq!(
        connection.held.lock().unwrap().no_destination,
        Some(durable::Error::Stale)
    );
    assert!(matches!(
        old.job().unwrap().result(),
        Some(Err(durable::Error::Stale))
    ));
    assert!(ops.resume_stream_state(&old_key).is_ok());
    assert!(same_local(
        account.owner(key).unwrap().review().await.unwrap().local(),
        &value
    ));
    worker.close(durable::GapReason::Disconnect).await.unwrap();
}

#[tokio::test]
async fn enrolled_input_cannot_reopen_a_replaced_socket_generation() {
    let (mut dom, mut state, worker, accepted, account, ops, key) = fixture().await;
    let (worker, result) = drive(&mut dom, async move {
        let mut worker = worker;
        let result = worker.connected(state, accepted, 9).await;
        (worker, result)
    })
    .await;
    result.unwrap();
    let connection = capture(&state.peek(), "T1").unwrap();
    let value = LocalDraft {
        composer: ComposerState::with_text("retained after socket replacement"),
        attachments: vec![],
    };
    let inventory = local(&account, &key, &value).await;
    Job::new(ops.enroll(inventory))
        .observed_success()
        .await
        .unwrap();
    state
        .write()
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .rt_generation = 10;
    let retained = connection.clone();
    assert!(matches!(
        drive(&mut dom, async move { retained.ensure(state).await }).await,
        Err(durable::Error::Gap)
    ));
    assert!(account.retained_source_openings().unwrap().is_empty());
    assert!(same_local(
        account.owner(key).unwrap().review().await.unwrap().local(),
        &value
    ));
    assert_eq!(
        connection.held.lock().unwrap().no_destination,
        Some(durable::Error::Stale)
    );
    drop(worker);
}

#[tokio::test]
async fn first_local_attachment_keeps_full_input_without_inventing_managed_continuity() {
    let (mut dom, state, worker, accepted, account, _ops, key) = fixture().await;
    let (worker, result) = drive(&mut dom, async move {
        let mut worker = worker;
        let result = worker.connected(state, accepted, 9).await;
        (worker, result)
    })
    .await;
    result.unwrap();
    let value = LocalDraft {
        composer: ComposerState::with_text("exact caption"),
        attachments: vec![super_platinum_core::domain::ComposerAttachment {
            id: 37,
            path: "fixture-only.txt".into(),
            name: "fixture-only.txt".into(),
            bytes: 19,
            uploading: false,
            upload_started: None,
            upload_cancel: None,
            upload_progress: None,
            preview_path: None,
        }],
    };
    let saved = local(&account, &key, &value).await;
    assert!(same_local(saved.local(), &value));
    let connection = capture(&state.peek(), "T1").unwrap();
    assert!(connection.current(&state.peek()));
    assert!(matches!(
        connection.source(&state.peek()),
        Err(durable::Error::Stale)
    ));
    assert!(account.inventory().await.unwrap().managed().is_empty());
    assert!(account.retained_source_openings().unwrap().is_empty());
    drop(worker);
}
