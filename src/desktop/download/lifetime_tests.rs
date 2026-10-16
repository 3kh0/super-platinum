//! Actual FileViewer -> DownloadButton hooks. Never click or open a native dialog.
use super::*;
use crate::file_preview::{FileViewer, refresh, refresh::tests as setup};
use dioxus::core::{AttributeValue, ElementId, Mutation, Mutations};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone)]
struct Mounted {
    saving: Signal<bool>,
    canceled: Arc<AtomicBool>,
}

#[derive(Clone, Default)]
struct Probe(Rc<RefCell<Option<Mounted>>>);

// This observation adds no hook and performs no mutation. Only these tests
// provide Probe; ordinary tests and every non-test build have no such context.
pub(super) fn observe(saving: Signal<bool>, canceled: &Arc<AtomicBool>) {
    if let Some(probe) = try_consume_context::<Probe>() {
        *probe.0.borrow_mut() = Some(Mounted {
            saving,
            canceled: canceled.clone(),
        });
    }
}

impl Probe {
    fn mounted(&self) -> Mounted {
        self.0
            .borrow()
            .clone()
            .expect("actual DownloadButton rendered")
    }
}

type Slot = Rc<RefCell<Option<Signal<ShellState>>>>;

fn mount(with_lease: bool) -> (VirtualDom, Signal<ShellState>, Probe, ElementId) {
    let slot = Slot::default();
    let probe = Probe::default();
    let mut dom = VirtualDom::new_with_props(
        |(slot, probe, with_lease): (Slot, Probe, bool)| {
            use_context_provider(|| probe);
            let state = use_signal(|| {
                let mut shell = setup::shell();
                if !with_lease {
                    refresh::close(&mut shell);
                }
                shell
            });
            *slot.borrow_mut() = Some(state);
            let viewer = state.read().viewer.clone().unwrap();
            rsx! { FileViewer {state,viewer,media_epoch:0} }
        },
        (slot.clone(), probe.clone(), with_lease),
    );
    let edits = dom.rebuild_to_vec();
    let button = label_element(&edits, "Download notes.txt");
    let state = slot.borrow().unwrap();
    (dom, state, probe, button)
}

fn label_elements(edits: &Mutations, expected: &str) -> Vec<ElementId> {
    edits
        .edits
        .iter()
        .filter_map(|edit| match edit {
            Mutation::SetAttribute {
                id,
                name: "aria-label",
                value: AttributeValue::Text(value),
                ..
            } if value == expected => Some(*id),
            _ => None,
        })
        .collect()
}

fn label_element(edits: &Mutations, expected: &str) -> ElementId {
    let ids = label_elements(edits, expected);
    assert_eq!(ids.len(), 1, "exactly one actual button label {expected}");
    ids[0]
}

fn removed_or_replaced(edits: &Mutations, old: ElementId) -> bool {
    edits.edits.iter().any(|edit| {
        matches!(edit, Mutation::Remove { id } | Mutation::ReplaceWith { id, .. } if *id == old)
    })
}

fn saving(dom: &mut VirtualDom, probe: &Probe, button: ElementId) -> Mounted {
    let mut mounted = probe.mounted();
    assert!(!*mounted.saving.peek());
    assert!(!mounted.canceled.load(Ordering::Acquire));
    // Model an already-admitted Save hold using the actual component's signal.
    // No handler is invoked, no request is built, and no OS dialog is opened.
    dom.in_runtime(|| mounted.saving.set(true));
    let edits = dom.render_immediate_to_vec();
    assert_eq!(label_element(&edits, "Cancel download notes.txt"), button);
    mounted
}

#[test]
fn actual_viewer_download_preserves_hook_and_button_for_same_lease() {
    let (mut dom, mut state, probe, button) = mount(true);
    let old = saving(&mut dom, &probe, button);
    dom.in_runtime(|| state.write().show_toast("Unrelated update."));
    let edits = dom.render_immediate_to_vec();
    let current = probe.mounted();
    assert_eq!(current.saving, old.saving);
    assert!(*current.saving.peek());
    assert!(Arc::ptr_eq(&current.canceled, &old.canceled));
    assert!(!old.canceled.load(Ordering::Acquire));
    assert!(label_elements(&edits, "Download notes.txt").is_empty());
    assert!(label_elements(&edits, "Cancel download notes.txt").is_empty());
    let shell = state.peek();
    let lease = refresh::lease(&shell, &shell.viewer.as_ref().unwrap().id).unwrap();
    assert!(Arc::ptr_eq(&lease.cancellation(), &current.canceled));
}

#[test]
fn actual_viewer_refresh_replaces_held_download_hook_and_button() {
    let (mut dom, mut state, probe, old_button) = mount(true);
    let old = saving(&mut dom, &probe, old_button);
    let old_id = state.peek().viewer.as_ref().unwrap().id.clone();
    dom.in_runtime(|| setup::publish(&mut state.write()));
    // Use the actual refresh Ticket::publish, including synchronous retirement.
    assert!(old.canceled.load(Ordering::Acquire));
    let new_latch = {
        let shell = state.peek();
        let viewer = shell.viewer.as_ref().unwrap();
        assert_ne!(viewer.id, old_id);
        refresh::lease(&shell, &viewer.id).unwrap().cancellation()
    };
    assert!(!new_latch.load(Ordering::Acquire));
    let edits = dom.render_immediate_to_vec();
    let current = probe.mounted();
    assert_ne!(current.saving, old.saving, "new lease needs new Save hooks");
    assert!(old.saving.try_read().is_err(), "old hook owner was dropped");
    assert!(!*current.saving.peek(), "fresh viewer is ready to Download");
    assert!(Arc::ptr_eq(&current.canceled, &new_latch));
    assert!(!Arc::ptr_eq(&current.canceled, &old.canceled));
    label_element(&edits, "Download notes.txt");
    assert!(removed_or_replaced(&edits, old_button));
    assert!(!new_latch.load(Ordering::Acquire));
    drop(dom);
    assert!(
        new_latch.load(Ordering::Acquire),
        "actual use_drop retires new button"
    );
}

#[test]
fn actual_viewer_default_asset_key_is_stable_then_drops_old_button() {
    let (mut dom, mut state, probe, old_button) = mount(false);
    let old = saving(&mut dom, &probe, old_button);
    dom.in_runtime(|| state.write().show_toast("Unrelated update."));
    dom.render_immediate_to_vec();
    assert_eq!(probe.mounted().saving, old.saving);
    assert!(!old.canceled.load(Ordering::Acquire));
    dom.in_runtime(|| {
        let mut shell = state.write();
        let id = shell.media.register_deferred(
            super_platinum_core::MediaAssetKind::Attachment,
            "https://files.slack.com/other-lifetime-test",
            "text/plain",
        );
        shell
            .media
            .insert(id.clone(), "text/plain", b"other contents".as_slice());
        shell.viewer.as_mut().unwrap().id = id;
    });
    // No refresh lease exists in this branch; only actual button unmount can
    // retire this old private cancellation latch.
    assert!(!old.canceled.load(Ordering::Acquire));
    let edits = dom.render_immediate_to_vec();
    let current = probe.mounted();
    assert_ne!(current.saving, old.saving);
    assert!(old.saving.try_read().is_err());
    assert!(old.canceled.load(Ordering::Acquire));
    assert!(!current.canceled.load(Ordering::Acquire));
    assert!(!*current.saving.peek());
    label_element(&edits, "Download notes.txt");
    assert!(removed_or_replaced(&edits, old_button));
    drop(dom);
    assert!(current.canceled.load(Ordering::Acquire));
}
