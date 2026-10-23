use super::*;
use dioxus::prelude::{VNode, VirtualDom, use_signal};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};
use super_platinum_core::slack_lists::comments::journal::Scope as DiskScope;
#[path = "titles/empty.rs"]
mod empty;

struct Local(std::path::PathBuf);
impl Drop for Local {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_file(self.0.with_extension("sqlite-journal"));
    }
}
async fn original() -> (VirtualDom, Signal<ShellState>, Job, Local) {
    let (shell, binding, _) = super::super::tests::setup();
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
    let mut suspended = Box::pin(draft::suspend_for_edit(
        state,
        binding.source.owner.serial,
        |_| true,
    ));
    let Poll::Ready(Ok(suspension)) = dom.in_runtime(|| {
        suspended
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
    }) else {
        panic!("unmounted reply suspension did not settle");
    };
    drop(suspended);
    let seed = session(&state.peek(), &binding).unwrap().seed.clone();
    let local = Local(std::env::temp_dir().join(format!(
        "list-own-edit-title-{}.sqlite",
        uuid::Uuid::new_v4()
    )));
    let disk = storage::Disk::test(
        DiskScope::capture(seed.workspace(), seed.user(), seed.target()).unwrap(),
        local.0.clone(),
    );
    let durable = durable::Durable::load(disk, binding.source.ts.clone())
        .await
        .unwrap();
    durable.capture(&seed, seed.editable()).await.unwrap();
    let mut shell = state.write();
    let s = session_mut(&mut shell, &binding).unwrap();
    s.durable = Some(durable.clone());
    *s.suspension.lock().unwrap() = Some(suspension);
    let job = Job {
        binding,
        seed,
        blocks: s.blocks.clone(),
        revision: s.revision,
        durable,
        suspension: s.suspension.clone(),
        tokens: vec![json!({"type":"canvas","file_id":"F_TITLE"})],
        serial: Arc::new(()),
        loading: true,
        retry_at: 0,
        error: None,
        observed: None,
    };
    s.titles = Some(job.clone());
    drop(shell);
    assert!(job.active(&state.peek()));
    (dom, state, job, local)
}

#[tokio::test]
async fn own_edit_title_job_retires_on_original_owner_buffer_suspension_or_election_change() {
    for mutation in 0..8 {
        let (_dom, mut state, job, _local) = original().await;
        let mut shell = state.write();
        match mutation {
            0 => shell.core.account_epoch += 1,
            1 => shell.core.workspaces.get_mut("T1").unwrap().rt_generation += 1,
            2 => session_mut(&mut shell, &job.binding).unwrap().revision += 1,
            3 => session_mut(&mut shell, &job.binding)
                .unwrap()
                .blocks
                .push(json!({"opaque":"new"})),
            4 => {
                session_mut(&mut shell, &job.binding)
                    .unwrap()
                    .suspension
                    .lock()
                    .unwrap()
                    .take();
            }
            5 => {
                session_mut(&mut shell, &job.binding)
                    .unwrap()
                    .titles
                    .as_mut()
                    .unwrap()
                    .serial = Arc::new(())
            }
            6 => session_mut(&mut shell, &job.binding).unwrap().finishing = true,
            _ => {
                shell.slack_lists.comments.own_edit.active.take();
            }
        }
        assert!(!job.active(&shell), "mutation {mutation}");
    }
}

#[tokio::test]
async fn pre_mount_title_gate_refuses_native_install_but_does_not_invent_capture_for_cancel() {
    let (_dom, mut state, job, _local) = original().await;
    let mut shell = state.write();
    let view = presentation(&shell).unwrap();
    assert!(view.title_loading && !view.mount_ready && !view.can_save && view.can_cancel);
    assert!(!install_native(
        &mut shell,
        &job.binding,
        bridge::test_mounted(&job.binding, "too-early")
    ));
    let s = session(&shell, &job.binding).unwrap();
    assert!(s.native.is_none() && !s.native_ready && s.native_sequence == 0);
    // A recovered unknown delivery must not gain Cancel from the new path.
    let record = super_platinum_core::slack_lists::comments::OwnEditIntent::capture(
        &s.seed,
        &job.binding.source.original_for_test(),
        &super::super::tests::setup().2,
        s.blocks.clone(),
        1_700_000_100_000,
    )
    .unwrap()
    .record()
    .clone();
    let account = shell.core.active_account.clone();
    session_mut(&mut shell, &job.binding).unwrap().attempt = Some(Arc::new(Mutex::new(Attempt {
        id: next_id(),
        disk_id: None,
        account,
        record,
        phase: Phase::Unknown,
        receipt: None,
        retry_at: 0,
    })));
    assert!(!presentation(&shell).unwrap().can_cancel);
}

#[test]
fn unsupported_saved_tokens_keep_original_mount_fallback_without_a_title_job() {
    let (mut shell, binding, _) = super::super::tests::setup();
    session_mut(&mut shell, &binding).unwrap().blocks =
        vec![json!({"type":"rich_text","elements":[
        {"type":"rich_text_section","elements":[
            {"type":"file","file_id":"F_HOSTED","opaque":"keep"},
            {"type":"canvas","file_id":"F_SUBOBJECT","section_id":"S1","style":{"bold":true}}
        ]} ]})];
    let view = presentation(&shell).unwrap();
    assert!(view.mount_ready && !view.title_loading && !view.can_retry_titles);
    assert!(session(&shell, &binding).unwrap().titles.is_none());
}

#[tokio::test]
async fn mounted_editor_is_not_relabelled_or_unmounted_by_retired_title_receipt() {
    let (_dom, mut state, job, _local) = original().await;
    let mut shell = state.write();
    let s = session_mut(&mut shell, &job.binding).unwrap();
    s.native = Some(bridge::test_mounted(&job.binding, "original"));
    s.native_ready = true;
    s.native_sequence = 1;
    s.blocks.push(json!({"opaque":"actual-new-edit"}));
    s.revision += 1;
    assert!(!job.current(&shell));
    assert!(presentation(&shell).unwrap().mount_ready);
    assert!(labels(&shell, &job.binding).is_empty());
    assert_eq!(native(&shell, &job.binding).unwrap().mount(), "original");
}

#[tokio::test]
async fn restored_old_seed_keeps_display_custody_but_changed_current_source_stops_loading() {
    let (_dom, mut state, mut job, _local) = original().await;
    let mut shell = state.write();
    let e = shell.slack_lists.comments.entry.as_mut().unwrap();
    let mut raw = e.page.as_ref().unwrap().raw().clone();
    raw["messages"][1]["blocks"] = json!([{"type":"rich_text","elements":[
        {"type":"rich_text_section","elements":[{"type":"text","text":"New server body"}]}]}]);
    e.page = Some(Page::parse(&e.target, raw).unwrap());
    // This is the ordinary restored edit distinction: new visit/source, old seed.
    let source = Arc::new(
        Source::capture(
            &shell,
            job.binding.source.owner.serial,
            &job.binding.source.ts,
        )
        .unwrap(),
    );
    shell
        .slack_lists
        .comments
        .own_edit
        .active
        .as_mut()
        .unwrap()
        .binding
        .source = source.clone();
    job.binding.source = source;
    session_mut(&mut shell, &job.binding).unwrap().titles = Some(job.clone());
    assert!(job.current(&shell));
    let view = presentation(&shell).unwrap();
    assert!(view.title_loading && view.can_cancel && !view.can_save);
    // A later body-bearing event retires this acquisition, not its Cancel path.
    shell.slack_lists.comments.own_edit.events += 1;
    assert!(!job.active(&shell));
    let view = presentation(&shell).unwrap();
    assert!(!view.title_loading && !view.mount_ready && view.title_error.is_some());
    assert!(view.can_cancel && !view.can_save);
}
