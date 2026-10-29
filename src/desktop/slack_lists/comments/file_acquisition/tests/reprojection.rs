use super::*;
use dioxus::prelude::{VNode, VirtualDom, use_signal};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
};

fn publish(shell: &mut ShellState, id: &str, revision: u64) {
    let raw = json!({"ok":true,"original_receipt":revision,"file":{"id":id,"mode":"quip",
        "filetype":"quip","created":101,"updated":revision,"title":" \t ","title_blocks":[],
        "permalink":"https://fixture.invalid/original"}});
    let received = OrderedFileInfo::decode(&serde_json::to_vec(&raw).unwrap()).unwrap();
    FileInfoRead::capture(shell, "T1", id)
        .unwrap()
        .publish(shell, &received)
        .unwrap();
}

async fn ready(
    count: usize,
) -> (
    VirtualDom,
    Signal<ShellState>,
    u64,
    Attempt,
    Arc<AtomicBool>,
) {
    let (mut shell, generation, work) = loaded(count);
    queue(&mut shell, generation, work);
    let attempt = shell.slack_lists.comments.file_acquisition.attempts[0].clone();
    publish(&mut shell, &attempt.demand.id, 1);
    type Input = (
        Rc<RefCell<Option<ShellState>>>,
        Rc<Cell<Option<Signal<ShellState>>>>,
    );
    let slot = Rc::new(Cell::new(None));
    let mut dom = VirtualDom::new_with_props(
        |(input, slot): Input| {
            let state = use_signal(|| input.borrow_mut().take().unwrap());
            slot.set(Some(state));
            VNode::empty()
        },
        (Rc::new(RefCell::new(Some(shell))), slot.clone()),
    );
    dom.rebuild_to_vec();
    let mut state = slot.get().unwrap();
    let guard = Arc::new(AtomicBool::new(true));
    let retained = guard.clone();
    let demand = attempt.demand.clone();
    let titles = title_users::acquire_transcript(
        state,
        "T1",
        "original-transcript",
        vec![attempt.demand.raw.clone()],
        Arc::new(move |s| demand.current(s) && retained.load(Ordering::SeqCst)),
    )
    .await
    .unwrap();
    let mut shell = state.write();
    let a = &mut shell.slack_lists.comments.file_acquisition.attempts[0];
    a.phase = Phase::Ready;
    a.titles = Some(titles);
    let original = a.clone();
    assert!(original.demand.ready(&shell));
    drop(shell);
    (dom, state, generation, original, guard)
}

#[tokio::test]
async fn new_same_file_receipt_rebuilds_independent_transcript_once_with_no_http() {
    let (_dom, mut state, generation, original, _guard) = ready(1).await;
    assert!(pending(&state.peek(), generation).is_empty());
    // Same displayed title, independently accepted new original file receipt.
    publish(&mut state.write(), &original.demand.id, 2);
    assert!(!original.titles.as_ref().unwrap().current(&state.peek()));
    let work = pending(&state.peek(), generation);
    assert_eq!(work.len(), 1, "duplicate occurrences still deduplicate");
    queue(&mut state.write(), generation, work);
    let replacement = state.peek().slack_lists.comments.file_acquisition.attempts[0].clone();
    assert_ne!(replacement.serial, original.serial);
    assert!(replacement.titles.is_none());
    let worker = Arc::new(());
    state.write().slack_lists.comments.file_acquisition.worker = Some(worker.clone());
    assert!(!attempt_current(&state.peek(), &original, &worker));
    run::worker(state, worker).await;
    let shell = state.peek();
    let current = &shell.slack_lists.comments.file_acquisition.attempts[0];
    assert!(current.phase == Phase::Ready && current.demand.ready(&shell));
    assert!(!Arc::ptr_eq(
        current.titles.as_ref().unwrap(),
        original.titles.as_ref().unwrap()
    ));
    let proof = current.titles.as_ref().unwrap().snapshot(&shell);
    assert_eq!(
        proof["titles"]["files"][0]["receipt"]["original_receipt"],
        2
    );
    assert_eq!(proof["titles"]["tokens"], json!([original.demand.raw]));
    assert!(pending(&shell, generation).is_empty());
    assert!(shell.slack_lists.comments.file_acquisition.worker.is_none());
}

#[tokio::test]
async fn unrelated_file_and_non_file_dependency_retirement_do_not_restart_title_job() {
    let (_dom, mut state, generation, original, guard) = ready(1).await;
    publish(&mut state.write(), "FUNRELATED01", 2);
    assert!(
        original
            .titles
            .as_ref()
            .unwrap()
            .files
            .metadata_current(&state.peek())
    );
    assert!(pending(&state.peek(), generation).is_empty());
    // A policy/owner guard may retire independently of the original file.
    // Such a failure must not be mistaken for newly acquired file metadata.
    guard.store(false, Ordering::SeqCst);
    assert!(
        !original
            .titles
            .as_ref()
            .unwrap()
            .files
            .source_current(&state.peek())
    );
    assert!(
        original
            .titles
            .as_ref()
            .unwrap()
            .files
            .metadata_current(&state.peek())
    );
    assert!(pending(&state.peek(), generation).is_empty());
    assert!(!reprojectable(&state.peek(), &original));
}

#[tokio::test]
async fn missing_file_and_refresh_failure_remain_latched_for_explicit_retry() {
    let (_dom, mut state, generation, original, _guard) = ready(1).await;
    crate::file_events::invalidate(&mut state.write(), "T1", &original.demand.id);
    assert!(pending(&state.peek(), generation).is_empty());
    publish(&mut state.write(), &original.demand.id, 2);
    let work = pending(&state.peek(), generation);
    queue(&mut state.write(), generation, work);
    let replacement = state.peek().slack_lists.comments.file_acquisition.attempts[0].clone();
    run::finish(
        &mut state.write(),
        &replacement,
        Err(Error::Offline("synthetic".into())),
    );
    for _ in 0..3 {
        assert!(pending(&state.peek(), generation).is_empty());
    }
    publish(&mut state.write(), &original.demand.id, 3);
    assert!(
        pending(&state.peek(), generation).is_empty(),
        "Unavailable is never automatically retried"
    );
    assert!(retry_allowed(&state.peek(), &replacement.demand));
}

#[tokio::test]
async fn refresh_replaces_at_full_capacity_but_respects_endpoint_cooldown() {
    let (_dom, mut state, generation, original, _guard) = ready(MAX_FILES).await;
    publish(&mut state.write(), &original.demand.id, 2);
    state.write().slack_lists.comments.file_acquisition.cooldown =
        Some((original.demand.source.receipt_scope(), now() + 100));
    assert!(pending(&state.peek(), generation).is_empty());
    state.write().slack_lists.comments.file_acquisition.cooldown = None;
    let work = pending(&state.peek(), generation);
    assert_eq!(work.len(), 1);
    queue(&mut state.write(), generation, work);
    let shell = state.peek();
    assert_eq!(
        shell.slack_lists.comments.file_acquisition.attempts.len(),
        MAX_FILES
    );
    assert!(
        !shell
            .slack_lists
            .comments
            .file_acquisition
            .attempts
            .iter()
            .any(|a| a.serial == original.serial)
    );
    assert_eq!(
        shell
            .slack_lists
            .comments
            .file_acquisition
            .attempts
            .iter()
            .filter(|a| a.demand.same(&original.demand))
            .count(),
        1
    );
}

#[tokio::test]
async fn account_visit_socket_and_old_worker_retirement_refuse_refresh_adoption() {
    for mutation in 0..4 {
        let (_dom, mut state, generation, original, _guard) = ready(1).await;
        publish(&mut state.write(), &original.demand.id, 2);
        let work = pending(&state.peek(), generation);
        queue(&mut state.write(), generation, work);
        let replacement = state.peek().slack_lists.comments.file_acquisition.attempts[0].clone();
        let worker = Arc::new(());
        state.write().slack_lists.comments.file_acquisition.worker = Some(worker.clone());
        let mut shell = state.write();
        match mutation {
            0 => shell.core.account_epoch += 1,
            1 => controller::close_shell(&mut shell),
            2 => shell.core.workspaces.get_mut("T1").unwrap().rt_generation += 1,
            _ => shell.slack_lists.comments.file_acquisition.worker = Some(Arc::new(())),
        }
        assert!(!attempt_current(&shell, &replacement, &worker));
        assert!(!attempt_current(&shell, &original, &worker));
        if mutation < 3 {
            assert!(pending(&shell, generation).is_empty());
        }
    }
}

#[test]
fn transcript_started_before_reconnect_cannot_authorize_new_socket_file_work() {
    let (mut shell, generation, _) = loaded(1);
    let entry = shell.slack_lists.comments.entry.as_ref().unwrap().clone();
    let mut raw = entry.page.as_ref().unwrap().raw().clone();
    raw["messages"][0]["ts"] = json!("1700000002.000001");
    raw["messages"][0]["text"] = json!("Accepted transcript from the older socket");
    let page = Page::parse(&entry.target, raw.clone()).unwrap();
    let (owner, serial, _, cursor) = controller::begin(&mut shell, false).unwrap();
    shell.core.workspaces.get_mut("T1").unwrap().rt_generation += 1;
    controller::finish(
        &mut shell,
        &owner,
        serial,
        cursor,
        Ok(network::Loaded {
            page,
            projection: entry.projection.unwrap(),
        }),
    );
    let current = shell.slack_lists.comments.entry.as_ref().unwrap();
    assert!(!current.loading);
    // A valid transcript still publishes. Only automatic title authority waits
    // for a read that actually began on the new realtime generation.
    assert_eq!(current.page.as_ref().unwrap().raw(), &raw);
    assert_eq!(current.rows[0].ts, "1700000002.000001");
    assert!(!Arc::ptr_eq(&current.rows, &entry.rows));
    for _ in 0..3 {
        assert!(pending(&shell, generation).is_empty());
    }
}

#[test]
fn preflight_completion_never_grants_new_socket_transcript_authority() {
    let (mut shell, generation, _) = loaded(1);
    let entry = shell.slack_lists.comments.entry.as_ref().unwrap().clone();
    let (old_owner, old_serial, _, _) = controller::begin_preflight(&mut shell).unwrap();
    shell.core.workspaces.get_mut("T1").unwrap().rt_generation += 1;
    super::super::super::reply::finish_preflight(&mut shell, &old_owner, old_serial);
    assert!(pending(&shell, generation).is_empty());
    let (owner, serial, _, _) = controller::begin_preflight(&mut shell).unwrap();
    // An older completion cannot release the newer read's busy flag.
    super::super::super::reply::finish_preflight(&mut shell, &old_owner, old_serial);
    assert!(owner.reading(&shell, serial));
    super::super::super::reply::finish_preflight(&mut shell, &owner, serial);
    let current = shell.slack_lists.comments.entry.as_ref().unwrap();
    assert!(!current.loading);
    assert!(Arc::ptr_eq(&current.rows, &entry.rows));
    assert_eq!(
        current.page.as_ref().unwrap().raw(),
        entry.page.as_ref().unwrap().raw()
    );
    assert!(pending(&shell, generation).is_empty());
    let (owner, serial, _, cursor) = controller::begin(&mut shell, false).unwrap();
    controller::finish(
        &mut shell,
        &owner,
        serial,
        cursor,
        Ok(network::Loaded {
            page: entry.page.unwrap(),
            projection: entry.projection.unwrap(),
        }),
    );
    let work = pending(&shell, generation);
    assert_eq!(work.len(), 1);
    queue(&mut shell, generation, work);
    assert!(pending(&shell, generation).is_empty());
}

#[test]
fn retired_socket_requires_successful_transcript_read_before_new_automatic_work() {
    let (mut shell, generation, _) = loaded(1);
    shell.core.workspaces.get_mut("T1").unwrap().rt_generation += 1;
    assert!(pending(&shell, generation).is_empty());
    let entry = shell.slack_lists.comments.entry.as_ref().unwrap().clone();
    let (owner, serial, _, cursor) = controller::begin(&mut shell, false).unwrap();
    controller::finish(
        &mut shell,
        &owner,
        serial,
        cursor,
        Err(Error::Offline("synthetic".into())),
    );
    assert!(pending(&shell, generation).is_empty());
    let (owner, serial, _, cursor) = controller::begin(&mut shell, false).unwrap();
    controller::finish(
        &mut shell,
        &owner,
        serial,
        cursor,
        Ok(network::Loaded {
            page: entry.page.unwrap(),
            projection: entry.projection.unwrap(),
        }),
    );
    let work = pending(&shell, generation);
    assert_eq!(work.len(), 1);
    queue(&mut shell, generation, work);
    assert!(pending(&shell, generation).is_empty());
}
