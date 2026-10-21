//! Exercise the actual panel's bindings through the pinned renderer diff.
//! Native tests separately prove preservation of the browser's uncommitted values.
use super::*;
use dioxus::core::{AttributeValue, Mutation, Mutations};
use std::{cell::Cell, rc::Rc};

type Slot = Rc<Cell<Option<Signal<ShellState>>>>;

fn mount() -> (VirtualDom, Signal<ShellState>, u64, Mutations) {
    let slot = Slot::default();
    let mut dom = VirtualDom::new_with_props(
        |slot: Slot| {
            let state = use_signal(|| super::super::tests::shell().0);
            let visit = state
                .peek()
                .app_home
                .about
                .management
                .surface
                .as_ref()
                .unwrap()
                .id;
            slot.set(Some(state));
            panel(state, visit)
        },
        slot.clone(),
    );
    let edits = dom.rebuild_to_vec();
    let state = slot.get().unwrap();
    let visit = state
        .peek()
        .app_home
        .about
        .management
        .surface
        .as_ref()
        .unwrap()
        .id;
    (dom, state, visit, edits)
}

fn value(edits: &Mutations, expected: &str) -> bool {
    edits.edits.iter().any(|edit| {
        matches!(edit, Mutation::SetAttribute {
            name: "value", value: AttributeValue::Text(value), ..
        } if value == expected)
    })
}

fn selected(edits: &Mutations, expected: bool) -> usize {
    edits
        .edits
        .iter()
        .filter(|edit| {
            matches!(edit, Mutation::SetAttribute {
            name: "selected", value: AttributeValue::Bool(value), ..
        } if *value == expected)
        })
        .count()
}

fn changed_model(state: &mut ShellState, visit: u64) {
    let a = &mut document_mut(state, visit).unwrap().activity;
    a.filters.from = "2026-06-01".into();
    a.filters.to = "2026-06-02".into();
    a.filters.status = "ERROR".into();
    a.filters.limit = "50".into();
    a.user_search = "Native search model".into();
}

#[test]
fn unrelated_policy_render_does_not_rewrite_native_values_or_selected_options() {
    let (mut dom, mut state, _, initial) = mount();
    // Explicit selection is still mounted after dynamic options are created.
    assert!(value(&initial, "100"));
    assert_eq!(selected(&initial, true), 3);
    dom.in_runtime(|| state.write().app_home.workflow_names_revision += 1);
    let edits = dom.render_immediate_to_vec();
    assert!(
        !edits.edits.iter().any(|edit| matches!(
            edit,
            Mutation::SetAttribute {
                name: "value" | "selected",
                ..
            }
        )),
        "Unrelated policy invalidation must not overwrite renderer-owned buffers: {:?}",
        edits.edits
    );
}

#[test]
fn actual_model_changes_still_update_date_search_and_select_properties() {
    let (mut dom, mut state, visit, _) = mount();
    dom.in_runtime(|| changed_model(&mut state.write(), visit));
    let edits = dom.render_immediate_to_vec();
    for expected in ["Native search model", "ERROR", "50"] {
        assert!(value(&edits, expected), "Missing model value {expected}");
    }
    for (name, expected) in [("data-from", "2026-06-01"), ("data-to", "2026-06-02")] {
        assert!(edits.edits.iter().any(|edit| matches!(edit,
            Mutation::SetAttribute{name: actual,value:AttributeValue::Text(value),..}
            if *actual==name && value==expected)));
    }
    assert_eq!(selected(&edits, true), 2);
    assert_eq!(selected(&edits, false), 2);
}

#[test]
fn clearing_changed_model_updates_current_values_instead_of_only_defaults() {
    let (mut dom, mut state, visit, _) = mount();
    dom.in_runtime(|| changed_model(&mut state.write(), visit));
    dom.render_immediate_to_vec();
    dom.in_runtime(|| {
        let mut s = state.write();
        let a = &mut document_mut(&mut s, visit).unwrap().activity;
        a.filters = Filters::default();
        a.user_search.clear();
    });
    let edits = dom.render_immediate_to_vec();
    assert!(value(&edits, ""));
    assert!(value(&edits, "100"));
    assert_eq!(selected(&edits, true), 2);
    assert_eq!(selected(&edits, false), 2);
    assert!(!edits.edits.iter().any(|edit| matches!(
        edit,
        Mutation::SetAttribute {
            name: "initial_value" | "initial_selected",
            ..
        }
    )));
}

#[test]
fn filter_visibility_explicitly_overrides_display_on_the_same_mounted_panel() {
    let (mut dom, mut state, visit, initial) = mount();
    let panel = initial
        .edits
        .iter()
        .find_map(|edit| match edit {
            Mutation::SetAttribute {
                name: "style",
                value: AttributeValue::Text(value),
                id,
                ..
            } if value == "display:none" => Some(*id),
            _ => None,
        })
        .expect("Original hidden Filter panel");
    for (open, expected) in [
        (true, "display:block"),
        (false, "display:none"),
        (true, "display:block"),
    ] {
        dom.in_runtime(|| {
            document_mut(&mut state.write(), visit)
                .unwrap()
                .activity
                .calendar
                .filter_open = open
        });
        let edits = dom.render_immediate_to_vec();
        assert!(edits.edits.iter().any(|edit| matches!(edit,
            Mutation::SetAttribute{name:"style",value:AttributeValue::Text(value),id,..}
            if *id==panel && value==expected)));
        // A visibility toggle must neither remount genuine native controls nor
        // rewrite their renderer-owned values/selected options.
        assert!(
            edits.edits.iter().all(|edit| matches!(edit,
            Mutation::SetAttribute{name,..} if !matches!(*name,"value"|"selected"))),
            "Visibility unexpectedly replaced controls or their buffers: {:?}",
            edits.edits
        );
    }
}
