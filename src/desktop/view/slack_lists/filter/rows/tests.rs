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
        selects: vec![],
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
            rsx! {Conditions{state,serial:7,visit:1,p,editable:true,local:false}}
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
            rsx! {Conditions{state,serial:7,visit:1,p,editable:true,local:false}}
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

#[test]
fn actual_select_checkbox_nodes_survive_epoch_toggle_and_removal_of_number_neighbor() {
    use super_platinum_core::slack_lists::SelectFilterOperator;
    let slot = Slot::default();
    let copy = slot.clone();
    let mut dom = VirtualDom::new_with_props(
        |slot: Slot| {
            let state = use_signal(|| ShellState::fixture(crate::media::MediaRegistry::default()));
            let draft = use_signal(|| {
                let mut p = initial();
                p.numbers = vec!["A".into()];
                p.selects = vec![(
                    "B".into(),
                    vec![("a".into(), "Same".into()), ("é".into(), "Same".into())],
                )];
                p.desired = ViewFilterChoice::All(vec![
                    value("A", "12.50"),
                    ViewFilterChoice::Select {
                        column_id: "B".into(),
                        operator: SelectFilterOperator::Contains,
                        values: vec!["é".into()],
                    },
                ]);
                p
            });
            slot.set(Some(draft));
            let p = draft.read().clone();
            rsx! {Conditions{state,serial:7,visit:1,p,editable:true,local:false}}
        },
        copy,
    );
    let first = dom.rebuild_to_vec();
    let option = |edits: &Mutations, want: &str| {
        edits
            .edits
            .iter()
            .find_map(|m| match m {
                Mutation::SetAttribute {
                    id,
                    name: "data-filter-choice",
                    value: AttributeValue::Text(v),
                    ..
                } if *v == want => Some(*id),
                _ => None,
            })
            .expect("actual choice attribute joins exact node")
    };
    let a = option(&first, "a");
    let accent = option(&first, "é");
    assert_ne!(a, accent);
    let mut draft = slot.get().unwrap();
    dom.in_runtime(|| {
        let mut p = draft.write();
        p.rows[1].epoch += 1;
        p.choice += 1;
        p.desired = ViewFilterChoice::All(vec![
            value("A", "12.50"),
            ViewFilterChoice::Select {
                column_id: "B".into(),
                operator: SelectFilterOperator::Contains,
                values: vec!["é".into(), "a".into()],
            },
        ]);
    });
    dom.mark_dirty(ScopeId::ROOT);
    let toggle = dom.render_immediate_to_vec();
    assert!(toggle.edits.iter().any(|m|matches!(m,Mutation::SetAttribute{id,name:"checked",value:AttributeValue::Bool(true),..} if *id==a)));
    assert!(!toggle.edits.iter().any(
        |m| matches!(m,Mutation::Remove{id}|Mutation::ReplaceWith{id,..} if *id==a||*id==accent)
    ));
    dom.in_runtime(|| {
        let mut p = draft.write();
        p.rows.remove(0);
        p.choice += 1;
        p.desired = ViewFilterChoice::Select {
            column_id: "B".into(),
            operator: SelectFilterOperator::Contains,
            values: vec!["a".into()],
        };
    });
    dom.mark_dirty(ScopeId::ROOT);
    let removed = dom.render_immediate_to_vec();
    assert!(removed.edits.iter().any(|m|matches!(m,Mutation::SetAttribute{id,name:"checked",value:AttributeValue::Bool(false),..} if *id==accent)));
    assert!(!removed.edits.iter().any(
        |m| matches!(m,Mutation::Remove{id}|Mutation::ReplaceWith{id,..} if *id==a||*id==accent)
    ));
}

#[test]
fn calendar_node_survives_sibling_removal_and_preset_removes_only_calendar_control() {
    use super_platinum_core::slack_lists::DateFilterOperator;
    let slot = Slot::default();
    let copy = slot.clone();
    let date = |text: &str| ViewFilterChoice::Date {
        column_id: "B".into(),
        operator: DateFilterOperator::Equal,
        value: Some(text.into()),
    };
    let mut dom = VirtualDom::new_with_props(
        |slot: Slot| {
            let state = use_signal(|| ShellState::fixture(crate::media::MediaRegistry::default()));
            let draft = use_signal(|| {
                let mut p = initial();
                p.numbers = vec!["A".into()];
                p.desired = ViewFilterChoice::All(vec![
                    value("A", "12.50"),
                    ViewFilterChoice::Date {
                        column_id: "B".into(),
                        operator: DateFilterOperator::Equal,
                        value: Some("2024-02-29".into()),
                    },
                ]);
                p
            });
            slot.set(Some(draft));
            let p = draft.read().clone();
            rsx! {Conditions{state,serial:7,visit:1,p,editable:true,local:false}}
        },
        copy,
    );
    let first = dom.rebuild_to_vec();
    let calendar = input(&first, "2024-02-29");
    let mut draft = slot.get().unwrap();
    dom.in_runtime(|| {
        let mut p = draft.write();
        p.desired = date("2024-03-01");
        p.rows.remove(0);
        p.choice += 1;
    });
    dom.mark_dirty(ScopeId::ROOT);
    let removed = dom.render_immediate_to_vec();
    assert_eq!(input(&removed, "2024-03-01"), calendar);
    dom.in_runtime(|| {
        let mut p = draft.write();
        p.desired = date("today");
        p.rows[0].epoch += 1;
        p.choice += 1;
    });
    dom.mark_dirty(ScopeId::ROOT);
    let preset = dom.render_immediate_to_vec();
    assert!(!preset.edits.iter().any(|m|matches!(m,Mutation::SetAttribute{name:"initial_value",value:AttributeValue::Text(v),..} if v=="today")));
}
