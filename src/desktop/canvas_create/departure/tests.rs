//! Exact private temporary SQLite only; no session, transport, DOM or OS actions.
use super::super::{adapter::Storage, editor::Editor};
use super::*;
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    sync::Arc,
    time::Duration,
};

struct Temp {
    path: PathBuf,
    owner: domain::Owner,
}
impl Temp {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("sp-canvas-departure-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        Self {
            path,
            owner: domain::Owner::new(&uuid::Uuid::new_v4().to_string(), "T1", "U00000001")
                .unwrap(),
        }
    }
    fn shell(&self) -> (ShellState, Arc<Editor>) {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        shell.core.active_account = Some(self.owner.account().into());
        shell.core.active_team = Some("T1".into());
        shell.self_account.user_id = self.owner.user().into();
        shell.core.workspaces.get_mut("T1").unwrap().self_user_id = self.owner.user().into();
        let mut store =
            domain::Store::open(self.path.join("private.sqlite"), self.owner.account()).unwrap();
        let base = store
            .create(
                &self.owner,
                &uuid::Uuid::new_v4().to_string(),
                content("before"),
            )
            .unwrap();
        let storage =
            Storage::isolated(self.path.join("private.sqlite"), self.owner.clone()).unwrap();
        let editor = shell.canvas_creation.install(storage, base, true).unwrap();
        let mut d = Draft::new(Some(self.owner.clone()));
        d.loading = false;
        d.editor = Some(editor.clone());
        shell.canvas_create = Some(d);
        shell.overlay = Some(Overlay::CanvasCreate);
        (shell, editor)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
fn content(title: &str) -> domain::Content {
    domain::Content::new(title.into(), "exact retained body".into()).unwrap()
}
fn visit(shell: &ShellState) -> String {
    shell.canvas_create.as_ref().unwrap().visit.clone()
}
fn mounted(shell: ShellState) -> (VirtualDom, Signal<ShellState>) {
    let initial = Rc::new(RefCell::new(Some(shell)));
    let slot = Rc::new(Cell::new(None));
    let mut dom = VirtualDom::new_with_props(
        |(initial, slot): (
            Rc<RefCell<Option<ShellState>>>,
            Rc<Cell<Option<Signal<ShellState>>>>,
        )| {
            let state = use_signal(|| initial.borrow_mut().take().unwrap());
            slot.set(Some(state));
            rsx! {}
        },
        (initial, slot.clone()),
    );
    dom.rebuild_in_place();
    (dom, slot.get().unwrap())
}

#[test]
fn first_typed_destination_wins_and_is_taken_once() {
    let temp = Temp::new();
    let (mut shell, editor) = temp.shell();
    let visit = visit(&shell);
    assert!(request(&mut shell, Destination::Palette));
    assert!(request(
        &mut shell,
        Destination::Settings(SettingsSection::Storage)
    ));
    assert_eq!(shell.overlay, Some(Overlay::CanvasCreate));
    assert!(shell.canvas_create.as_ref().unwrap().close_requested);
    let base = editor.snapshot().base;
    let next = take_saved(&mut shell, &visit, &editor, &base)
        .unwrap()
        .unwrap();
    assert!(matches!(next.destination, Destination::Palette));
    assert!(
        take_saved(&mut shell, &visit, &editor, &base)
            .unwrap()
            .is_none()
    );
}

#[test]
fn account_epoch_visit_and_new_navigation_revoke_old_destination() {
    for kind in 0..5 {
        let temp = Temp::new();
        let (mut shell, editor) = temp.shell();
        let visit = visit(&shell);
        assert!(request(&mut shell, Destination::Palette));
        match kind {
            0 => shell.core.active_account = Some(uuid::Uuid::new_v4().to_string()),
            1 => shell.core.account_epoch += 1,
            2 => shell.canvas_create.as_mut().unwrap().visit = uuid::Uuid::new_v4().to_string(),
            3 => shell.channel_generation += 1,
            _ => shell.shortcut_generation += 1,
        }
        assert!(
            take_saved(&mut shell, &visit, &editor, &editor.snapshot().base)
                .unwrap()
                .is_none()
        );
        assert_eq!(shell.overlay, Some(Overlay::CanvasCreate));
    }
}

#[test]
fn workspace_destination_keeps_id_not_mutable_row_index() {
    let temp = Temp::new();
    let (mut shell, _) = temp.shell();
    let target = shell.workspaces[0].id.clone();
    assert!(!shell.select_workspace(0));
    let next = shell
        .canvas_create
        .as_ref()
        .unwrap()
        .pending_departure
        .as_ref()
        .unwrap();
    assert!(matches!(&next.destination,Destination::Workspace(team) if team==&target));
}

#[tokio::test(flavor = "current_thread")]
async fn actual_save_then_close_runs_original_destination_once() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let temp = Temp::new();
            let (mut shell, editor) = temp.shell();
            let visit = visit(&shell);
            assert!(request(&mut shell, Destination::Palette));
            let token = editor.reserve_capture().unwrap();
            let serial = editor
                .retain_capture(&token, content("native captured version"))
                .unwrap();
            assert!(take_saved(&mut shell, &visit, &editor, &editor.snapshot().base).is_err());
            let saved = tokio::time::timeout(Duration::from_secs(5), editor.saved(serial))
                .await
                .unwrap()
                .unwrap();
            editor.release_capture(&token);
            let next = take_saved(&mut shell, &visit, &editor, &saved)
                .unwrap()
                .unwrap();
            assert!(shell.canvas_creation.release(&editor));
            let store = domain::Store::open(temp.path.join("private.sqlite"), temp.owner.account())
                .unwrap();
            assert!(
                store.active(&temp.owner).unwrap().unwrap().content()
                    == &content("native captured version")
            );
            let (dom, mut state) = mounted(shell);
            dom.in_runtime(|| {
                controller::close_exact(state, &visit);
                next.run(state);
            });
            assert_eq!(state.peek().overlay, Some(Overlay::Palette));
            assert!(state.peek().canvas_create.is_none());
            assert!(take(&mut state.write(), &visit).is_none());
            assert!(state.peek().core.session.is_none() && state.peek().core.transport.is_none());
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn uncertain_local_write_never_authorizes_navigation() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let temp = Temp::new();
            let (mut shell, editor) = temp.shell();
            let visit = visit(&shell);
            assert!(request(&mut shell, Destination::Palette));
            editor.storage.fail_next(true); // actual write commits but its result is lost.
            let token = editor.reserve_capture().unwrap();
            let serial = editor
                .retain_capture(&token, content("saved but unconfirmed"))
                .unwrap();
            assert!(
                tokio::time::timeout(Duration::from_secs(5), editor.saved(serial))
                    .await
                    .unwrap()
                    .is_err()
            );
            editor.release_capture(&token);
            assert!(take_saved(&mut shell, &visit, &editor, &editor.snapshot().base).is_err());
            let (dom, state) = mounted(shell);
            dom.in_runtime(|| {
                controller::update_error(state, &visit, "Save could not be confirmed.")
            });
            let s = state.peek();
            let d = s.canvas_create.as_ref().unwrap();
            assert_eq!(s.overlay, Some(Overlay::CanvasCreate));
            assert!(!d.close_requested && d.pending_departure.is_none());
            assert!(s.canvas_creation.find(&temp.owner).is_some());
            assert_eq!(editor.snapshot().content.title(), "saved but unconfirmed");
        })
        .await;
}

#[test]
fn capture_or_composition_error_cancels_intent_without_replacing_input() {
    let temp = Temp::new();
    let (mut shell, editor) = temp.shell();
    let visit = visit(&shell);
    assert!(request(&mut shell, Destination::Palette));
    let (dom, state) = mounted(shell);
    dom.in_runtime(|| {
        controller::update_error(state, &visit, "Finish composing text, then try again.")
    });
    let s = state.peek();
    let d = s.canvas_create.as_ref().unwrap();
    assert!(!d.close_requested && d.pending_departure.is_none());
    assert_eq!(s.overlay, Some(Overlay::CanvasCreate));
    assert!(Arc::ptr_eq(d.editor.as_ref().unwrap(), &editor));
}

#[test]
fn consumed_intent_refuses_replacement_dialog_before_dispatch() {
    let temp = Temp::new();
    let (mut shell, editor) = temp.shell();
    let old = visit(&shell);
    assert!(request(&mut shell, Destination::Palette));
    let next = take_saved(&mut shell, &old, &editor, &editor.snapshot().base)
        .unwrap()
        .unwrap();
    shell.canvas_create = Some(Draft::new(Some(temp.owner.clone())));
    let replacement = visit(&shell);
    let (dom, state) = mounted(shell);
    dom.in_runtime(|| next.run(state));
    assert_eq!(state.peek().overlay, Some(Overlay::CanvasCreate));
    assert_eq!(
        state.peek().canvas_create.as_ref().unwrap().visit,
        replacement
    );
}

#[test]
fn carrying_root_custody_does_not_carry_a_navigation_grant() {
    let temp = Temp::new();
    let (mut old, editor) = temp.shell();
    assert!(request(&mut old, Destination::Palette));
    let mut loaded = ShellState::fixture(crate::media::MediaRegistry::default());
    controller::carry_pending(&mut old, &mut loaded);
    assert!(loaded.canvas_create.is_none());
    assert!(Arc::ptr_eq(
        &loaded.canvas_creation.find(&temp.owner).unwrap(),
        &editor
    ));
}

#[test]
fn failed_saved_departure_releases_signal_before_error_projection() {
    let temp = Temp::new();
    let (mut shell, editor) = temp.shell();
    let visit = visit(&shell);
    assert!(request(&mut shell, Destination::Palette));
    let base = editor.snapshot().base;
    // An outstanding capture makes take_saved refuse; this exercises the real
    // controller error projection, not a copied match or a mock Signal.
    let capture = editor.reserve_capture().unwrap();
    let (dom, state) = mounted(shell);
    assert!(
        dom.in_runtime(|| controller::take_saved_departure(state, &visit, &editor, &base))
            .is_err()
    );
    let shell = state.peek();
    let draft = shell.canvas_create.as_ref().unwrap();
    assert_eq!(shell.overlay, Some(Overlay::CanvasCreate));
    assert!(!draft.busy && !draft.close_requested && draft.pending_departure.is_none());
    assert_eq!(
        draft.error.as_deref(),
        Some("Your latest input is still retained. Finish saving before navigating.")
    );
    assert!(Arc::ptr_eq(draft.editor.as_ref().unwrap(), &editor));
    assert_eq!(editor.snapshot().content.title(), "before");
    drop(shell);
    editor.release_capture(&capture);
}
