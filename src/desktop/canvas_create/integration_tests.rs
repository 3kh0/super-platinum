//! Private temporary database tests only; no default storage or network.
use super::*;
use adapter::Storage;
use domain::{Content, Owner, Store};
use std::{path::PathBuf, time::Duration};

struct Temp {
    path: PathBuf,
    owner: Owner,
}
impl Temp {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("sp-canvas-controller-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        Self {
            path,
            owner: Owner::new(&uuid::Uuid::new_v4().to_string(), "T00000001", "U00000001").unwrap(),
        }
    }
    fn store(&self) -> Store {
        Store::open(self.path.join("private.sqlite"), self.owner.account()).unwrap()
    }
    fn storage(&self) -> Storage {
        Storage::isolated(self.path.join("private.sqlite"), self.owner.clone()).unwrap()
    }
    fn editor(&self, root: &Root) -> Arc<editor::Editor> {
        let base = self
            .store()
            .create(
                &self.owner,
                &uuid::Uuid::new_v4().to_string(),
                text("before"),
            )
            .unwrap();
        root.install(self.storage(), base, true).unwrap()
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
fn text(value: &str) -> Content {
    Content::new(
        value.into(),
        "\u{4e2d}\u{1f642}e\u{301}\n  markdown  ".into(),
    )
    .unwrap()
}
async fn saved(editor: &Arc<editor::Editor>, serial: u64) -> domain::Draft {
    tokio::time::timeout(Duration::from_secs(10), editor.saved(serial))
        .await
        .expect("bounded save")
        .unwrap()
}
async fn failed(editor: &Arc<editor::Editor>) {
    tokio::time::timeout(Duration::from_secs(10), async {
        let mut changes = editor.changes();
        while !editor.snapshot().failed {
            changes.changed().await.unwrap();
        }
    })
    .await
    .expect("bounded failure");
}

#[test]
fn root_editor_custody_remains_send_for_actual_startup_worker() {
    fn send<T: Send>() {}
    send::<Root>();
    send::<Arc<editor::Editor>>();
}
#[test]
fn application_opener_refuses_under_desktop_unit_tests() {
    let owner = Owner::new(&uuid::Uuid::new_v4().to_string(), "T00000001", "U00000001").unwrap();
    assert!(Storage::application(owner).is_err());
}
#[tokio::test(flavor = "current_thread")]
async fn actual_initial_load_retry_reloads_only_the_current_owner_and_visit() {
    use std::{cell::Cell, rc::Rc};
    tokio::task::LocalSet::new()
        .run_until(async {
            assert!(
                !crate::fixture::is_fixture(),
                "this test never enables a native fixture"
            );
            let slot = Rc::new(Cell::new(None));
            let mut dom = VirtualDom::new_with_props(
                |slot: Rc<Cell<Option<Signal<ShellState>>>>| {
                    let state =
                        use_signal(|| ShellState::fixture(crate::media::MediaRegistry::default()));
                    slot.set(Some(state));
                    rsx! {}
                },
                slot.clone(),
            );
            dom.rebuild_in_place();
            let mut state = slot.get().unwrap();
            dom.in_runtime(|| {
                let mut shell = state.write();
                shell.core.active_account = Some(uuid::Uuid::new_v4().to_string());
                shell.core.active_team = Some("T1".into());
                shell.self_account.user_id = "U00000001".into();
                shell.core.workspaces.get_mut("T1").unwrap().self_user_id = "U00000001".into();
                shell.overlay = None;
                shell.canvas_create = None;
            });
            dom.in_runtime(|| controller::open(state));
            async fn load_failed(state: Signal<ShellState>) {
                tokio::time::timeout(Duration::from_secs(3), async {
                    loop {
                        if state
                            .peek()
                            .canvas_create
                            .as_ref()
                            .is_some_and(|d| !d.loading && d.error.is_some())
                        {
                            break;
                        }
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .expect("bounded test-only opener refusal");
            }
            // Storage::application rejects cfg(test) before reading a real path.
            load_failed(state).await;
            let first = state.peek().canvas_create.as_ref().unwrap().visit.clone();
            assert!(
                state
                    .peek()
                    .canvas_create
                    .as_ref()
                    .unwrap()
                    .editor
                    .is_none()
            );
            dom.in_runtime(|| controller::retry(state, &first));
            let second = state.peek().canvas_create.as_ref().unwrap().visit.clone();
            assert_ne!(first, second);
            assert!(state.peek().canvas_create.as_ref().unwrap().loading);
            load_failed(state).await;
            assert!(
                state
                    .peek()
                    .canvas_create
                    .as_ref()
                    .unwrap()
                    .editor
                    .is_none()
            );
            dom.in_runtime(|| controller::retry(state, &first));
            assert_eq!(state.peek().canvas_create.as_ref().unwrap().visit, second);
            dom.in_runtime(|| {
                state.write().core.active_account = Some(uuid::Uuid::new_v4().to_string())
            });
            dom.in_runtime(|| controller::retry(state, &second));
            assert_eq!(state.peek().canvas_create.as_ref().unwrap().visit, second);
            assert!(state.peek().core.session.is_none() && state.peek().core.transport.is_none());
        })
        .await;
}
#[test]
fn stale_result_cannot_install_a_different_revision_over_retained_editor() {
    let temp = Temp::new();
    let root = Root::default();
    let editor = temp.editor(&root);
    let before = editor.snapshot().base;
    let after = temp
        .store()
        .save(
            &before,
            &uuid::Uuid::new_v4().to_string(),
            text("external revision"),
        )
        .unwrap();
    assert!(root.install(temp.storage(), after.clone(), true).is_err());
    assert!(Arc::ptr_eq(&root.find(&temp.owner).unwrap(), &editor));
    assert!(editor.snapshot().base == before);
    assert!(root.release(&editor));
    assert!(root.install(temp.storage(), after, true).is_ok());
}
#[tokio::test(flavor = "current_thread")]
async fn accepted_input_coalesces_only_unstarted_work_and_retains_latest_exact_text() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let temp = Temp::new();
            let root = Root::default();
            let editor = temp.editor(&root);
            editor.accept(text("first")).unwrap();
            let serial = editor.accept(text("second")).unwrap();
            assert!(!root.release(&editor));
            let after = saved(&editor, serial).await;
            assert!(after.content() == &text("second"));
            assert!(temp.store().active(&temp.owner).unwrap().unwrap() == after);
            assert!(root.release(&editor));
        })
        .await;
}
#[tokio::test(flavor = "current_thread")]
async fn lost_save_reply_then_newer_input_recovers_exact_predecessor_without_rebase() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let temp = Temp::new();
            let root = Root::default();
            let editor = temp.editor(&root);
            editor.storage.fail_next(true);
            editor.accept(text("committed reply lost")).unwrap();
            failed(&editor).await;
            let serial = editor.accept(text("newer accepted input")).unwrap();
            assert!(!root.release(&editor));
            editor.retry().unwrap();
            let after = saved(&editor, serial).await;
            assert!(after.content() == &text("newer accepted input"));
            assert_eq!(after.revision(), 3);
        })
        .await;
}
#[tokio::test(flavor = "current_thread")]
async fn successful_late_capture_is_retained_after_ui_interest_ends_and_blocks_old_input() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let temp = Temp::new();
            let root = Root::default();
            let editor = temp.editor(&root);
            let token = editor.reserve_capture().unwrap();
            assert!(editor.reserve_capture().is_err());
            assert!(!root.release(&editor));
            // No current dialog is required to accept the original successful reply.
            let serial = editor.retain_capture(&token, text("native ahead")).unwrap();
            assert!(editor.accept(text("queued older event")).is_err());
            let after = saved(&editor, serial).await;
            assert!(after.content() == &text("native ahead"));
            editor.release_capture(&token);
            assert!(root.release(&editor));
        })
        .await;
}
#[tokio::test(flavor = "current_thread")]
async fn close_after_lost_save_reply_observes_exact_saved_base_without_an_extra_write() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let temp = Temp::new();
            let root = Root::default();
            let editor = temp.editor(&root);
            editor.storage.fail_next(true);
            editor.accept(text("committed reply lost")).unwrap();
            failed(&editor).await;
            let durable = temp.store().active(&temp.owner).unwrap().unwrap();
            let token = editor.reserve_capture().unwrap();
            editor
                .retain_capture(&token, text("committed reply lost"))
                .unwrap();
            tokio::time::timeout(Duration::from_secs(10), editor.retain_for_review())
                .await
                .unwrap()
                .unwrap();
            assert!(editor.snapshot().saved);
            assert!(editor.snapshot().base == durable);
            assert!(temp.store().active(&temp.owner).unwrap().unwrap() == durable);
            assert!(root.delivery.tickets(&temp.owner).is_empty());
            editor.release_capture(&token);
            assert!(root.release(&editor));
        })
        .await;
}
#[tokio::test(flavor = "current_thread")]
async fn competing_writer_is_not_adopted_and_latest_input_remains_charged() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let temp = Temp::new();
            let root = Root::default();
            let editor = temp.editor(&root);
            let base = editor.snapshot().base;
            temp.store()
                .save(
                    &base,
                    &uuid::Uuid::new_v4().to_string(),
                    text("foreign revision"),
                )
                .unwrap();
            editor.accept(text("our local input")).unwrap();
            failed(&editor).await;
            assert!(!root.release(&editor));
            assert!(editor.snapshot().content == text("our local input"));
            let conflicts = temp.store().conflicts(&temp.owner).unwrap();
            assert_eq!(conflicts.len(), 1);
            assert!(
                temp.store().active(&temp.owner).unwrap().unwrap().content()
                    == &text("foreign revision")
            );
        })
        .await;
}
#[tokio::test(flavor = "current_thread")]
async fn closing_a_retained_conflict_reuses_its_identity_and_can_release_exact_input() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let temp = Temp::new();
            let root = Root::default();
            let editor = temp.editor(&root);
            let base = editor.snapshot().base;
            temp.store()
                .save(&base, &uuid::Uuid::new_v4().to_string(), text("foreign"))
                .unwrap();
            editor.accept(text("ours")).unwrap();
            failed(&editor).await;
            let before = temp.store().conflicts(&temp.owner).unwrap();
            assert_eq!(before.len(), 1);
            let token = editor.reserve_capture().unwrap();
            editor.retain_capture(&token, text("ours")).unwrap();
            tokio::time::timeout(Duration::from_secs(10), editor.retain_for_review())
                .await
                .unwrap()
                .unwrap();
            assert!(temp.store().conflicts(&temp.owner).unwrap() == before);
            editor.release_capture(&token);
            assert!(root.release(&editor));
        })
        .await;
}
#[tokio::test(flavor = "current_thread")]
async fn startup_carry_keeps_root_input_but_never_old_dialog_or_navigation_grant() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let temp = Temp::new();
            let mut previous = ShellState::fixture(crate::media::MediaRegistry::default());
            let editor = temp.editor(&previous.canvas_creation);
            let token = editor.reserve_capture().unwrap();
            let mut loaded = ShellState::fixture(crate::media::MediaRegistry::default());
            carry_pending(&mut previous, &mut loaded);
            assert!(Arc::ptr_eq(
                &loaded.canvas_creation.find(&temp.owner).unwrap(),
                &editor
            ));
            assert!(loaded.canvas_create.is_none());
            let serial = editor
                .retain_capture(&token, text("late after replacement"))
                .unwrap();
            saved(&editor, serial).await;
        })
        .await;
}
#[test]
fn departure_fence_keeps_unpersisted_editor_and_serializes_repeated_leaves() {
    let temp = Temp::new();
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    let editor = temp.editor(&shell.canvas_creation);
    let mut draft = Draft::new(Some(temp.owner.clone()));
    draft.editor = Some(editor);
    draft.loading = false;
    let visit = draft.visit.clone();
    shell.canvas_create = Some(draft);
    shell.overlay = Some(Overlay::CanvasCreate);
    assert!(departure(&mut shell));
    assert!(departure(&mut shell));
    assert!(shell.overlay == Some(Overlay::CanvasCreate));
    assert!(
        shell
            .canvas_create
            .as_ref()
            .is_some_and(|d| d.visit == visit && d.close_requested)
    );
}
