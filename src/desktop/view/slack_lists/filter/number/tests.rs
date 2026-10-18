//! Exercise the actual product RSX with pinned Dioxus, without WebView or IO.
use super::*;
use dioxus::core::{AttributeValue, ElementId, Mutation, Mutations, ScopeId};
use std::{cell::Cell, rc::Rc};
type Draft = Signal<(u64, ViewFilterChoice)>;
type Slot = Rc<Cell<Option<Draft>>>;
fn number(id: &str, operator: Op, value: &str) -> ViewFilterChoice {
    ViewFilterChoice::Number {
        column_id: id.into(),
        operator,
        value: Some(value.into()),
    }
}
fn input_id(edits: &Mutations, expected: &str) -> ElementId {
    edits
        .edits
        .iter()
        .find_map(|edit| match edit {
            Mutation::SetAttribute {
                id,
                name: "initial_value",
                value: AttributeValue::Text(value),
                ..
            } if value == expected => Some(*id),
            _ => None,
        })
        .expect("Actual uncontrolled Number input initial_value mutation")
}
fn removes(edits: &Mutations, input: ElementId) -> bool {
    edits
        .edits
        .iter()
        .any(|edit| matches!(edit,Mutation::Remove{id}|Mutation::ReplaceWith{id,..} if *id==input))
}
fn harness() -> (VirtualDom, Draft, Mutations) {
    let slot = Slot::default();
    let copy = slot.clone();
    let mut dom = VirtualDom::new_with_props(
        |slot: Slot| {
            let state = use_signal(|| ShellState::fixture(crate::media::MediaRegistry::default()));
            let draft = use_signal(|| (31, number("A", Op::Equal, "12.50")));
            slot.set(Some(draft));
            let (choice, desired) = draft.read().clone();
            let p = filter::Presentation {
                original: ViewFilterChoice::None,
                desired,
                current: Some(ViewFilterChoice::None),
                choices: vec![("A".into(), "A".into()), ("B".into(), "B".into())],
                numbers: vec!["A".into(), "B".into()],
                selects: vec![],
                choice,
                rows: vec![filter::Row {
                    id: 17,
                    epoch: choice,
                }],
            };
            super::super::fields(state, 7, 1, p, true)
        },
        copy,
    );
    let initial = dom.rebuild_to_vec();
    (dom, slot.get().unwrap(), initial)
}
fn render(
    dom: &mut VirtualDom,
    mut draft: Draft,
    epoch: u64,
    id: &str,
    operator: Op,
    value: &str,
) -> Mutations {
    dom.in_runtime(|| draft.set((epoch, number(id, operator, value))));
    dom.mark_dirty(ScopeId::ROOT);
    dom.render_immediate_to_vec()
}
#[test]
fn same_epoch_keeps_actual_input_node_and_only_updates_default_value() {
    let (mut dom, draft, initial) = harness();
    let id = input_id(&initial, "12.50");
    for value in ["1", "12", "1e", ""] {
        let edits = render(&mut dom, draft, 31, "A", Op::Equal, value);
        assert_eq!(input_id(&edits, value), id);
        assert!(!removes(&edits, id));
        assert!(!edits.edits.iter().any(
            |edit| matches!(edit,Mutation::SetAttribute{id:other,name:"value",..} if *other==id)
        ));
    }
}
#[test]
fn field_operator_and_aba_epochs_replace_actual_native_input_node() {
    let (mut dom, draft, initial) = harness();
    let mut old = input_id(&initial, "12.50");
    for (epoch, id, op, value) in [
        (32, "B", Op::Equal, "2"),
        (33, "A", Op::Equal, "3"),
        (34, "A", Op::NotEqual, "4"),
        (35, "A", Op::Equal, "5"),
    ] {
        let edits = render(&mut dom, draft, epoch, id, op, value);
        let new = input_id(&edits, value);
        assert!(
            removes(&edits, old),
            "Old uncontrolled Number input must be removed, not receive new owner attributes"
        );
        assert!(
            edits
                .edits
                .iter()
                .any(|edit| matches!(edit, Mutation::LoadTemplate{id,..} if *id==new)),
            "Replacement has an actual freshly loaded input root"
        );
        old = new;
    }
}
