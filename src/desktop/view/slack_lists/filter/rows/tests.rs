//! Pinned renderer regression: structural edits must preserve surviving native inputs.
use super::*;
use dioxus::core::{AttributeValue, ElementId, Mutation, Mutations, ScopeId};
use std::{cell::Cell, rc::Rc};
type Draft = Signal<filter::Presentation>;
type Slot = Rc<Cell<Option<Draft>>>;
fn value(id: &str, text: &str) -> ViewFilterChoice {
    ViewFilterChoice::Number {
        column_id: id.into(),
        operator: Op::Equal,
        value: Some(text.into()),
    }
}
fn initial() -> filter::Presentation {
    filter::Presentation {
        original: ViewFilterChoice::None,
        current: Some(ViewFilterChoice::None),
        desired: ViewFilterChoice::All(vec![value("A", "12.50"), value("B", "2e1")]),
        choices: vec![("A".into(), "A".into()), ("B".into(), "B".into())],
        numbers: vec!["A".into(), "B".into()],
        choice: 30,
        rows: vec![
            filter::Row { id: 1, epoch: 11 },
            filter::Row { id: 2, epoch: 12 },
        ],
    }
}
fn input(edits: &Mutations, text: &str) -> ElementId {
    edits
        .edits
        .iter()
        .find_map(|m| match m {
            Mutation::SetAttribute {
                id,
                name: "initial_value",
                value: AttributeValue::Text(v),
                ..
            } if *v == text => Some(*id),
            _ => None,
        })
        .expect("actual native Number input")
}
#[test]
fn two_number_nodes_survive_value_render_and_removing_another_stable_row() {
    let slot = Slot::default();
    let copy = slot.clone();
    let mut dom = VirtualDom::new_with_props(
        |slot: Slot| {
            let state = use_signal(|| ShellState::fixture(crate::media::MediaRegistry::default()));
            let draft = use_signal(initial);
            slot.set(Some(draft));
            let p = draft.read().clone();
            rsx! {Conditions{state,serial:7,visit:1,p,editable:true}}
        },
        copy,
    );
    let first = dom.rebuild_to_vec();
    let a = input(&first, "12.50");
    let b = input(&first, "2e1");
    assert_ne!(a, b);
    let mut draft = slot.get().unwrap();
    dom.in_runtime(|| {
        let mut p = draft.write();
        p.desired = ViewFilterChoice::All(vec![value("A", "12.50"), value("B", "25")]);
    });
    dom.mark_dirty(ScopeId::ROOT);
    let changed = dom.render_immediate_to_vec();
    assert_eq!(input(&changed, "25"), b);
    dom.in_runtime(|| {
        let mut p = draft.write();
        p.desired = value("B", "26");
        p.rows.remove(0);
        p.choice += 1;
    });
    dom.mark_dirty(ScopeId::ROOT);
    let removed = dom.render_immediate_to_vec();
    assert_eq!(input(&removed, "26"), b);
    dom.in_runtime(|| {
        let mut p = draft.write();
        p.rows[0].epoch += 1;
        p.choice += 1;
        p.desired = ViewFilterChoice::Number {
            column_id: "B".into(),
            operator: Op::LessThan,
            value: Some("26".into()),
        };
    });
    dom.mark_dirty(ScopeId::ROOT);
    let epoch = dom.render_immediate_to_vec();
    assert!(
        epoch.edits.iter().any(|m| matches!(
            m,
            Mutation::CreatePlaceholder { .. } | Mutation::LoadTemplate { .. }
        )),
        "field/operator epoch remounts native comparison input"
    );
}

#[test]
fn actual_over_cap_input_text_is_carried_to_operator_epoch_without_old_value_revival() {
    let slot = Slot::default();
    let copy = slot.clone();
    let mut dom = VirtualDom::new_with_props(
        |slot: Slot| {
            let state = use_signal(|| ShellState::fixture(crate::media::MediaRegistry::default()));
            let draft = use_signal(initial);
            slot.set(Some(draft));
            let p = draft.read().clone();
            rsx! {Conditions{state,serial:7,visit:1,p,editable:true}}
        },
        copy,
    );
    let first = dom.rebuild_to_vec();
    let before = input(&first, "12.50");
    let long = "9".repeat(129);
    let mut draft = slot.get().unwrap();
    dom.in_runtime(|| {
        draft.write().desired = ViewFilterChoice::All(vec![value("A", &long), value("B", "2e1")])
    });
    dom.mark_dirty(ScopeId::ROOT);
    let text = dom.render_immediate_to_vec();
    assert_eq!(input(&text, &long), before);
    dom.in_runtime(|| {
        let mut p = draft.write();
        p.rows[0].epoch += 20;
        p.choice += 1;
        p.desired = ViewFilterChoice::All(vec![
            ViewFilterChoice::Number {
                column_id: "A".into(),
                operator: Op::LessThan,
                value: Some(long.clone()),
            },
            value("B", "2e1"),
        ]);
    });
    dom.mark_dirty(ScopeId::ROOT);
    let changed = dom.render_immediate_to_vec();
    input(&changed, &long);
    assert!(!changed.edits.iter().any(|m|matches!(m,Mutation::SetAttribute{name:"initial_value",value:AttributeValue::Text(v),..} if v=="12.50")));
    assert!(changed.edits.iter().any(|m| matches!(
        m,
        Mutation::SetAttribute {
            name: "aria-invalid",
            value: AttributeValue::Bool(true),
            ..
        }
    )));
}
