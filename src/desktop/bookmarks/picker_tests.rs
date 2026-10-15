//! Actual panel mount/update/reopen mutations; browser behavior needs native checks.
use super::*;
use dioxus::core::{AttributeValue, Mutation, Mutations};
use std::{cell::Cell, rc::Rc};
type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
fn harness() -> (VirtualDom, Signal<ShellState>, Mutations) {
    let slot = Slot::default();
    let mut dom = VirtualDom::new_with_props(
        |slot: Slot| {
            let state = use_signal(|| {
                ShellState::fixture_variant(
                    crate::media::MediaRegistry::default(),
                    "bookmarks-move",
                )
            });
            slot.set(Some(state));
            rsx! {BookmarksPanel {state, channel:"C2"}}
        },
        slot.clone(),
    );
    let edits = dom.rebuild_to_vec();
    (dom, slot.get().unwrap(), edits)
}
fn selected(edits: &Mutations, value: &str) -> bool {
    edits.edits.iter().any(|edit| {
        let Mutation::SetAttribute {id,name:"value",value:AttributeValue::Text(v),..}=edit else {return false;};
        v==value && edits.edits.iter().any(|e| matches!(e,Mutation::SetAttribute {id:other,name:"selected",value:AttributeValue::Bool(true),..} if other==id))
    })
}
#[test]
fn prepopulated_parent_and_before_are_selected_on_actual_mount() {
    let (_dom, state, edits) = harness();
    let shell = state.peek();
    let m = shell
        .core
        .bookmarks
        .draft
        .as_ref()
        .unwrap()
        .movement
        .as_ref()
        .unwrap();
    assert_eq!((&*m.parent, &*m.before), ("Bk2", "Bk3"));
    assert!(selected(&edits, "Bk2") && selected(&edits, "Bk3"));
    assert!(shell.core.session.is_none() && shell.core.transport.is_none());
}
#[test]
fn existing_child_reopens_with_parent_after_changed_destination_and_position() {
    let (mut dom, mut state, _) = harness();
    dom.in_runtime(|| state.write().core.bookmarks.draft = None);
    dom.render_immediate_to_vec();
    let child = state
        .peek()
        .core
        .bookmarks
        .items
        .iter()
        .find(|b| b.id == "Bk3")
        .unwrap()
        .clone();
    for _ in 0..2 {
        dom.in_runtime(|| organize(state, "C2", child.clone()));
        let edits = dom.render_immediate_to_vec();
        assert!(selected(&edits, "Bk2"));
        assert_eq!(
            state
                .peek()
                .core
                .bookmarks
                .draft
                .as_ref()
                .unwrap()
                .movement
                .as_ref()
                .unwrap()
                .before,
            ""
        );
        dom.in_runtime(|| {
            change(state, |d| {
                d.movement = Some(Movement {
                    parent: String::new(),
                    before: "Bk1".into(),
                })
            })
        });
        let edits = dom.render_immediate_to_vec();
        assert!(selected(&edits, "Bk1"));
        dom.in_runtime(|| {
            change(state, |d| {
                let m = d.movement.as_mut().unwrap();
                m.parent = "Bk2".into();
                m.before.clear();
            })
        });
        dom.render_immediate_to_vec();
        assert_eq!(
            state
                .peek()
                .core
                .bookmarks
                .draft
                .as_ref()
                .unwrap()
                .movement
                .as_ref()
                .unwrap()
                .before,
            ""
        );
        dom.in_runtime(|| state.write().core.bookmarks.draft = None);
        dom.render_immediate_to_vec();
    }
}
