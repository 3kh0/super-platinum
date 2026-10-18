//! Same Worker and journal; only the browser-owned note extraction is injected.
mod new_direct;
use super::*;
use crate::file_share::fixture::{self, Capability, Case};
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    time::Duration,
};
use super_platinum_core::{
    file_sharing::{NotSent, State as ReceiptState, store::Journal},
    slack::api::file_share::FileShareDisposition,
};

struct Temp(PathBuf);
impl Temp {
    fn new(case: Case) -> Self {
        let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "sp-share-worker-{}-{}",
            std::process::id(),
            next_id().unwrap()
        ));
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(root.join("journal")).unwrap();
        std::fs::write(root.join("file-share-fixture.json"),serde_json::to_vec(&json!({"schema":1,"kind":"file-share-fixture","nonce":fixture::ACCOUNT,"fixture":"file-share","case":case})).unwrap()).unwrap();
        Self(root)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let Ok(path) = self.0.canonicalize() else {
            return;
        };
        let Ok(base) = std::env::temp_dir().canonicalize() else {
            return;
        };
        if path.parent() == Some(base.as_path())
            && path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("sp-share-worker-"))
        {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}
fn seed(temp: &Temp, case: Case) -> (ShellState, crate::file_actions::Target, Capability) {
    // This existing fixture constructor never loads a credential backend.
    let (mut shell, _, _) = crate::file_actions::attachment::tests::sample();
    assert!(shell.core.session.is_none() && shell.core.transport.is_none());
    let old = shell.core.active_team.take().unwrap();
    let mut workspace = shell.core.workspaces.remove(&old).unwrap();
    workspace.team_id = fixture::TEAM.into();
    workspace.self_user_id = fixture::USER.into();
    workspace.rt_generation = 11;
    workspace.channels.insert(
        fixture::DESTINATION.into(),
        serde_json::from_value(fixture::destination()).unwrap(),
    );
    workspace.users.insert(
        fixture::USER.into(),
        serde_json::from_value(json!({"id":fixture::USER,"is_bot":false})).unwrap(),
    );
    let channel = shell.core.active_channel.clone().unwrap();
    workspace.messages.get_mut(&channel).unwrap().messages[0].files =
        vec![serde_json::from_value(fixture::source_file()).unwrap()];
    shell
        .core
        .workspaces
        .insert(fixture::TEAM.into(), workspace);
    shell.core.active_team = Some(fixture::TEAM.into());
    shell.core.active_account = Some(fixture::ACCOUNT.into());
    shell.core.account_epoch = 7;
    shell.self_account.user_id = fixture::USER.into();
    let capability = Capability::for_test(temp.0.clone(), case, 7, 11).unwrap();
    shell.file_share_fixture = Some(capability.clone());
    let workspace = &shell.core.workspaces[fixture::TEAM];
    let raw = &workspace.messages[&channel].messages[0];
    let row = crate::message_vm::message_vm(workspace, raw, &shell.media);
    let source = crate::message_buttons::source::Source::capture(
        &shell,
        &row,
        &channel,
        crate::message_buttons::source::Bag::Timeline,
    )
    .unwrap();
    let target = crate::file_actions::attachment::target(
        &shell,
        &source,
        &super_platinum_core::file_actions::identity(&raw.files[0]).unwrap(),
        &super_platinum_core::state::file_title(&raw.files[0]),
    )
    .unwrap();
    assert!(can_open(&shell, &target));
    (shell, target, capability)
}
fn note() -> Vec<Value> {
    vec![
        json!({"type":"rich_text","future_block":{"keep":true},"elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Share \u{65e5}\u{672c}\u{8a9e} \u{1f680}","style":{"bold":true,"future_style":7}}]}]}),
    ]
}
struct Run {
    initial: RefCell<Option<ShellState>>,
    signal: Cell<Option<Signal<ShellState>>>,
    id: String,
    done: RefCell<Option<Result<(), Refusal>>>,
}
fn mount(shell: ShellState, id: String) -> (VirtualDom, Rc<Run>) {
    let shared = Rc::new(Run {
        initial: RefCell::new(Some(shell)),
        signal: Cell::new(None),
        id,
        done: RefCell::new(None),
    });
    let mut dom = VirtualDom::new_with_props(
        |shared: Rc<Run>| {
            let initial = shared.clone();
            let signal = use_signal(move || initial.initial.borrow_mut().take().unwrap());
            shared.signal.set(Some(signal));
            use_hook(move || {
                let mut worker = capture_worker(signal, shared.id.clone()).unwrap();
                worker.extracted_note = Some(note());
                dioxus::core::spawn_forever(async move {
                    let result = worker.run().await;
                    if let Err(error) = result {
                        worker.stop(error);
                    }
                    *shared.done.borrow_mut() = Some(result);
                });
            });
            rsx! {div{}}
        },
        shared.clone(),
    );
    dom.rebuild_in_place();
    (dom, shared)
}
async fn until(dom: &mut VirtualDom, ready: impl Fn() -> bool) {
    for _ in 0..300 {
        if ready() {
            return;
        }
        let _ = tokio::time::timeout(Duration::from_millis(10), dom.wait_for_work()).await;
        dom.render_immediate(&mut dioxus::core::NoOpMutations);
    }
    assert!(ready(), "common Worker did not reach bounded checkpoint");
}
fn scope() -> Scope {
    Scope {
        team: fixture::TEAM.into(),
        user: fixture::USER.into(),
        file: fixture::FILE.into(),
        destination: fixture::DESTINATION.into(),
    }
}
fn receipt(cap: &Capability) -> Receipt {
    Journal::open(cap.storage_path(), fixture::ACCOUNT)
        .unwrap()
        .get(&scope())
        .unwrap()
        .unwrap()
}
async fn open_selected(shell: &mut ShellState, target: crate::file_actions::Target) -> String {
    let source = admission::Source::capture(shell, &target).unwrap();
    let storage = Storage::capture(&source.context).unwrap();
    let load =
        durable::open::prepare_captured(shell, source, "source".into(), Ok(storage)).unwrap();
    let outcome = load.outcome().await;
    load.publish(shell, outcome);
    let id = load.id;
    choose(shell, &id, fixture::DESTINATION).unwrap();
    id
}

#[test]
fn fixture_context_requires_exact_installed_allocation_owner_and_no_credentials() {
    for change in 0..8 {
        let temp = Temp::new(Case::Accepted);
        let (mut shell, target, cap) = seed(&temp, Case::Accepted);
        let source = admission::Source::capture(&shell, &target).unwrap();
        assert!(cap.current(&shell));
        match change {
            0 => shell.file_share_fixture = None,
            1 => shell.core.account_epoch += 1,
            2 => {
                shell
                    .core
                    .workspaces
                    .get_mut(fixture::TEAM)
                    .unwrap()
                    .rt_generation += 1
            }
            3 => shell.core.active_account = Some("other".into()),
            4 => shell.self_account.user_id = "UOTHER".into(),
            5 => {
                shell.core.session = Some(super_platinum_core::config::Session {
                    unified_user_client_enabled: Default::default(),
                    d_cookie: String::new(),
                    workspaces: Default::default(),
                })
            }
            6 => {
                // Construction is local only; no request is executed. Presence
                // alone must retire the disconnected capability.
                shell.core.transport = Some(Arc::new(
                    super_platinum_core::slack::Transport::new("").unwrap(),
                ));
            }
            _ => {
                let other = Temp::new(Case::Accepted);
                shell.file_share_fixture =
                    Some(Capability::for_test(other.0.clone(), Case::Accepted, 7, 11).unwrap());
                assert!(!cap.current(&shell));
            }
        }
        assert!(!source.current(&shell));
        assert!(!cap.current(&shell));
    }
}

#[tokio::test]
async fn same_worker_prepares_once_and_persists_typed_synthetic_outcomes() {
    for case in [
        Case::Accepted,
        Case::DlpWarning,
        Case::Processing,
        Case::Unconfirmed,
    ] {
        let temp = Temp::new(case);
        let (mut shell, target, cap) = seed(&temp, case);
        let id = open_selected(&mut shell, target).await;
        let (mut dom, run) = mount(shell, id);
        until(&mut dom, || cap.evidence().unwrap().awaiting_release).await;
        let before = receipt(&cap);
        assert_eq!(before.state(), ReceiptState::Unconfirmed);
        assert_eq!(before.intent.note(), Some(note().as_slice()));
        assert_eq!(
            run.signal
                .get()
                .unwrap()
                .peek()
                .file_share
                .dialog
                .as_ref()
                .unwrap()
                .phase,
            Phase::Sending
        );
        let evidence = cap.evidence().unwrap();
        assert_eq!(evidence.reads, ["files.info", "conversations.info"]);
        assert_eq!(evidence.dispatch_count, 1);
        assert_eq!(
            evidence.attempt.as_deref(),
            Some(before.attempt_id.as_str())
        );
        cap.release(&before.attempt_id).unwrap();
        until(&mut dom, || run.done.borrow().is_some()).await;
        assert_eq!(*run.done.borrow(), Some(Ok(())));
        let after = receipt(&cap);
        assert_eq!(after.attempt_id, before.attempt_id);
        assert_eq!(after.intent.note(), before.intent.note());
        match case {
            Case::Accepted => assert!(matches!(
                after.state(),
                ReceiptState::Server(FileShareDisposition::Accepted { .. })
            )),
            Case::DlpWarning => assert!(matches!(
                after.state(),
                ReceiptState::Server(FileShareDisposition::DlpWarning { .. })
            )),
            Case::Processing => assert!(matches!(
                after.state(),
                ReceiptState::Server(FileShareDisposition::DlpProcessing)
            )),
            Case::Unconfirmed => assert_eq!(after.state(), ReceiptState::Unconfirmed),
            _ => unreachable!(),
        }
        let signal = run.signal.get().unwrap();
        let shell = signal.peek();
        assert!(shell.core.session.is_none() && shell.core.transport.is_none());
        assert!(shell.file_share.custody.lock().unwrap().pending.is_empty());
        assert_eq!(cap.evidence().unwrap().dispatch_count, 1);
        let drafts = Journal::open(cap.storage_path(), fixture::ACCOUNT)
            .unwrap()
            .drafts(fixture::TEAM, fixture::USER)
            .unwrap();
        if matches!(case, Case::Accepted) {
            assert!(drafts.is_empty());
        } else {
            assert_eq!(drafts.len(), 1);
            assert_eq!(drafts[0].content.note, note());
        }
    }
}

#[tokio::test]
async fn same_worker_final_fence_cancels_source_or_permission_change_after_real_begin() {
    for permission_change in [false, true] {
        let temp = Temp::new(Case::SourceStale);
        let (mut shell, target, cap) = seed(&temp, Case::SourceStale);
        let id = open_selected(&mut shell, target).await;
        let (mut dom, run) = mount(shell, id);
        until(&mut dom, || cap.evidence().unwrap().awaiting_begin_release).await;
        let before = receipt(&cap);
        assert_eq!(before.state(), ReceiptState::Unconfirmed);
        {
            let mut signal = run.signal.get().unwrap();
            let mut shell = signal.write();
            if permission_change {
                shell
                    .core
                    .workspaces
                    .get_mut(fixture::TEAM)
                    .unwrap()
                    .channels
                    .get_mut(fixture::DESTINATION)
                    .unwrap()
                    .extra
                    .insert("is_read_only".into(), json!(true));
            } else {
                crate::file_actions::attachment::tests::message(&mut shell).files[0].title =
                    Some("Changed after Begin".into());
            }
        }
        cap.release_begin(&before.attempt_id).unwrap();
        until(&mut dom, || run.done.borrow().is_some()).await;
        assert_eq!(*run.done.borrow(), Some(Ok(())));
        let after = receipt(&cap);
        assert_eq!(after.attempt_id, before.attempt_id);
        assert_eq!(after.intent.note(), before.intent.note());
        assert_eq!(
            after.state(),
            ReceiptState::NotSent(NotSent::AdmissionCanceled)
        );
        let evidence = cap.evidence().unwrap();
        assert_eq!(evidence.dispatch_count, 0);
        assert!(!evidence.awaiting_release && !evidence.released);
        assert_eq!(
            evidence.begin_attempt.as_deref(),
            Some(before.attempt_id.as_str())
        );
        assert!(
            run.signal
                .get()
                .unwrap()
                .peek()
                .file_share
                .custody
                .lock()
                .unwrap()
                .pending
                .is_empty()
        );
    }
}

#[tokio::test]
async fn same_worker_settles_after_account_replacement_without_republishing_old_dialog() {
    let temp = Temp::new(Case::Accepted);
    let (mut shell, target, cap) = seed(&temp, Case::Accepted);
    let id = open_selected(&mut shell, target).await;
    let (mut dom, run) = mount(shell, id);
    until(&mut dom, || cap.evidence().unwrap().awaiting_release).await;
    let before = receipt(&cap);
    let mut signal = run.signal.get().unwrap();
    {
        let mut old = signal.write();
        let mut replacement = ShellState::fixture(crate::media::MediaRegistry::default());
        replacement.core.active_account = Some("new-account".into());
        carry_pending(&mut old, &mut replacement);
        *old = replacement;
    }
    cap.release(&before.attempt_id).unwrap();
    until(&mut dom, || run.done.borrow().is_some()).await;
    assert_eq!(*run.done.borrow(), Some(Ok(())));
    assert!(matches!(
        receipt(&cap).state(),
        ReceiptState::Server(FileShareDisposition::Accepted { .. })
    ));
    assert!(signal.peek().file_share.dialog.is_none());
    assert!(
        signal
            .peek()
            .file_share
            .custody
            .lock()
            .unwrap()
            .pending
            .is_empty()
    );
}

#[tokio::test]
async fn same_worker_retains_result_and_note_when_actual_private_storage_fails() {
    let temp = Temp::new(Case::StorageFailure);
    let (mut shell, target, cap) = seed(&temp, Case::StorageFailure);
    let id = open_selected(&mut shell, target).await;
    let (mut dom, run) = mount(shell, id.clone());
    until(&mut dom, || cap.evidence().unwrap().awaiting_release).await;
    let before = receipt(&cap);
    // All Journal connections have closed before the injected response. Replace
    // only this owned directory with a file so actual Journal::open fails.
    let directory = cap.storage_path().parent().unwrap();
    let retained = temp.0.join("retained-journal");
    std::fs::rename(directory, &retained).unwrap();
    std::fs::write(directory, b"private fixture failure").unwrap();
    cap.release(&before.attempt_id).unwrap();
    until(&mut dom, || run.done.borrow().is_some()).await;
    assert_eq!(*run.done.borrow(), Some(Err(Refusal::Storage)));
    let signal = run.signal.get().unwrap();
    let shell = signal.peek();
    assert_eq!(
        shell.file_share.dialog.as_ref().unwrap().phase,
        Phase::StorageFailed
    );
    assert!(matches!(
        retained_status(&shell, &before.attempt_id),
        Some(ReceiptState::Server(FileShareDisposition::Accepted { .. }))
    ));
    let held = shell.file_share.custody.lock().unwrap();
    assert_eq!(
        held.pending[&id].receipt.as_ref().unwrap().intent.note(),
        Some(note().as_slice())
    );
    assert!(held.drafts.is_empty()); // durable saved version owns the unsubmitted body now
    drop(held);
    std::fs::remove_file(directory).unwrap();
    std::fs::rename(&retained, directory).unwrap();
    assert_eq!(receipt(&cap).state(), ReceiptState::Unconfirmed);
    let stored = Journal::open(cap.storage_path(), fixture::ACCOUNT)
        .unwrap()
        .drafts(fixture::TEAM, fixture::USER)
        .unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].content.note, note());

    assert_eq!(cap.evidence().unwrap().dispatch_count, 1);
    drop(shell);
    dom.in_scope(ScopeId::ROOT, || {
        retry_result(signal, id.clone(), before.attempt_id.clone())
    });
    until(&mut dom, || {
        signal
            .peek()
            .file_share
            .custody
            .lock()
            .unwrap()
            .pending
            .is_empty()
    })
    .await;
    let saved = receipt(&cap);
    assert_eq!(saved.attempt_id, before.attempt_id);
    assert_eq!(saved.intent.note(), before.intent.note());
    assert!(matches!(
        saved.state(),
        ReceiptState::Server(FileShareDisposition::Accepted { .. })
    ));
    assert_eq!(cap.evidence().unwrap().dispatch_count, 1);
    assert_eq!(
        cap.evidence().unwrap().reads,
        ["files.info", "conversations.info"]
    );
    assert_eq!(signal.peek().file_share.dialog.as_ref().unwrap().id, id);
    assert_eq!(
        signal.peek().file_share.dialog.as_ref().unwrap().phase,
        Phase::Outcome
    );
    assert!(signal.peek().core.session.is_none() && signal.peek().core.transport.is_none());
    assert!(
        Journal::open(cap.storage_path(), fixture::ACCOUNT)
            .unwrap()
            .drafts(fixture::TEAM, fixture::USER)
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn actual_worker_final_fence_cancels_after_close_block_and_keeps_later_capture_review() {
    let temp = Temp::new(Case::SourceStale);
    let (mut shell, target, cap) = seed(&temp, Case::SourceStale);
    let id = open_selected(&mut shell, target).await;
    let (mut dom, run) = mount(shell, id.clone());
    until(&mut dom, || cap.evidence().unwrap().awaiting_begin_release).await;
    let before = receipt(&cap);
    let mut signal = run.signal.get().unwrap();
    let capture = {
        let mut shell = signal.write();
        let capture = durable::capture::Capture::prepare(&mut shell, &id).unwrap();
        shell.file_share.dialog.as_mut().unwrap().closing = true;
        capture
    };
    let visit = capture.visit.clone();
    let mut later = note();
    later[0]["future_block"] = json!({"later":true});
    let ticket = capture
        .persist(
            later.clone(),
            Some(Destination {
                id: fixture::DESTINATION.into(),
                name: "ship".into(),
                private: false,
            }),
            true,
        )
        .await
        .unwrap();
    let outcome = ticket.wait().await;
    assert!(matches!(&outcome,draft_io::Outcome::Blocked(r) if r.attempt_id==before.attempt_id));
    {
        let mut shell = signal.write();
        assert!(durable::apply_capture(
            &mut shell, &id, &visit, &ticket, &outcome
        ));
        shell.file_share.dialog.as_mut().unwrap().closing = false;
    }
    cap.release_begin(&before.attempt_id).unwrap();
    until(&mut dom, || run.done.borrow().is_some()).await;
    assert_eq!(*run.done.borrow(), Some(Ok(())));
    assert_eq!(
        receipt(&cap).state(),
        ReceiptState::NotSent(NotSent::AdmissionCanceled)
    );
    assert_eq!(cap.evidence().unwrap().dispatch_count, 0);
    let shell = signal.peek();
    let d = visible(&shell).unwrap();
    assert_eq!(d.phase, Phase::DraftReview);
    assert!(d.draft_local);
    assert!(d.draft_task.as_ref().unwrap().same(&ticket));
    assert_eq!(durable::captured(&ticket).unwrap().note, later);
}

#[tokio::test]
async fn actual_worker_cancellation_after_close_reader_failure_cannot_replace_resumed_editing() {
    for fail_storage in [false, true] {
        let temp = Temp::new(Case::SourceStale);
        let (mut shell, target, cap) = seed(&temp, Case::SourceStale);
        let id = open_selected(&mut shell, target).await;
        let (mut dom, run) = mount(shell, id.clone());
        until(&mut dom, || cap.evidence().unwrap().awaiting_begin_release).await;
        let before = receipt(&cap);
        let mut signal = run.signal.get().unwrap();
        let capture = {
            let mut shell = signal.write();
            let capture = durable::capture::Capture::prepare(&mut shell, &id).unwrap();
            shell.file_share.dialog.as_mut().unwrap().closing = true;
            capture
        };
        let previous = capture.predecessor().await;
        let visit = capture.visit.clone();
        let observation = capture.ticket.clone();
        let prior = capture.prior_task.clone();
        drop(capture);
        let mut current = note();
        current[0]["future_block"] = json!({"edited_after_failed_reader":true});
        {
            let mut shell = signal.write();
            durable::close::reader_failed(&mut shell, &id, &visit, &observation, previous, prior);
            let d = shell.file_share.dialog.as_mut().unwrap();
            assert_eq!(d.phase, Phase::Editing);
            d.note = current.clone();
        }
        let directory = cap.storage_path().parent().unwrap();
        let retained = temp.0.join("before-cancel");
        if fail_storage {
            std::fs::rename(directory, &retained).unwrap();
            std::fs::write(directory, b"owned cancellation storage failure").unwrap();
        }
        cap.release_begin(&before.attempt_id).unwrap();
        until(&mut dom, || run.done.borrow().is_some()).await;
        if fail_storage {
            assert_eq!(*run.done.borrow(), Some(Err(Refusal::Storage)));
            std::fs::remove_file(directory).unwrap();
            std::fs::rename(&retained, directory).unwrap();
            assert_eq!(receipt(&cap).state(), ReceiptState::Unconfirmed);
            assert!(
                signal.peek().file_share.custody.lock().unwrap().pending[&id]
                    .receipt
                    .is_some()
            );
        } else {
            assert_eq!(*run.done.borrow(), Some(Ok(())));
            assert_eq!(
                receipt(&cap).state(),
                ReceiptState::NotSent(NotSent::AdmissionCanceled)
            );
        }
        assert_eq!(cap.evidence().unwrap().dispatch_count, 0);
        let shell = signal.peek();
        let d = visible(&shell).unwrap();
        assert_eq!(d.phase, Phase::Editing);
        assert_eq!(d.note, current);
        assert!(!d.closing && d.receipt.is_none());
        assert_eq!(d.error, Some(Refusal::Note.message()));
    }
}

mod direct;
