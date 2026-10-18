//! The actual keyed element iterator must retain surviving select DOM identities.
use super::*;
use dioxus::core::{AttributeValue, ElementId, Mutation, Mutations, ScopeId};
use std::{cell::Cell, rc::Rc};
type Slot = Rc<Cell<Option<Signal<sort::Presentation>>>>;
fn rule(id: &str) -> ViewSortChoice {
    ViewSortChoice::Field {
        column_id: id.into(),
        ascending: true,
    }
}
fn initial() -> sort::Presentation {
    sort::Presentation {
        original: ViewSortChoice::None,
        current: Some(ViewSortChoice::None),
        desired: ViewSortChoice::from_rules(vec![rule("A"), rule("B")]),
        choices: vec![
            ("A".into(), "A".into()),
            ("B".into(), "B".into()),
            ("C".into(), "C".into()),
        ],
        choice: 30,
        rows: vec![1, 2],
    }
}
fn field(edits: &Mutations, dom_id: &str) -> ElementId {
    edits
        .edits
        .iter()
        .find_map(|m| match m {
            Mutation::SetAttribute {
                id,
                name: "id",
                value: AttributeValue::Text(v),
                ..
            } if v == dom_id => Some(*id),
            _ => None,
        })
        .expect("actual select identified by its exact DOM id, not option mutation order")
}
fn changed_value(edits: &Mutations, id: ElementId, text: &str) {
    assert!(edits.edits.iter().any(|m|matches!(m,Mutation::SetAttribute{id:changed,name:"value",value:AttributeValue::Text(v),..} if *changed==id && v==text)));
}
#[test]
fn keyed_sort_row_survives_priority_reorder_and_removing_a_different_row() {
    let slot = Slot::default();
    let copy = slot.clone();
    let mut dom = VirtualDom::new_with_props(
        |slot: Slot| {
            let state = use_signal(|| ShellState::fixture(crate::media::MediaRegistry::default()));
            let draft = use_signal(initial);
            slot.set(Some(draft));
            let p = draft.read().clone();
            rsx! {Rules{state,serial:7,visit:1,p,editable:true,local:false}}
        },
        copy,
    );
    let first = dom.rebuild_to_vec();
    let a = field(&first, "sort-field-1");
    let b = field(&first, "sort-field-2");
    assert_ne!(a, b);
    let mut draft = slot.get().unwrap();
    dom.in_runtime(|| {
        let mut p = draft.write();
        p.rows.swap(0, 1);
        p.desired = ViewSortChoice::from_rules(vec![rule("B"), rule("A")]);
        p.choice += 1;
    });
    dom.mark_dirty(ScopeId::ROOT);
    let reordered = dom.render_immediate_to_vec();
    assert!(
        !reordered
            .edits
            .iter()
            .any(|m| matches!(m,Mutation::Remove{id} if *id==a||*id==b))
    );
    // Changing the surviving row's value exposes its actual ElementId after reorder.
    dom.in_runtime(|| {
        let mut p = draft.write();
        p.desired = ViewSortChoice::from_rules(vec![rule("C"), rule("A")]);
        p.choice += 1;
    });
    dom.mark_dirty(ScopeId::ROOT);
    let changed = dom.render_immediate_to_vec();
    changed_value(&changed, b, "C");
    dom.in_runtime(|| {
        let mut p = draft.write();
        p.rows.pop();
        p.desired = rule("B");
        p.choice += 1;
    });
    dom.mark_dirty(ScopeId::ROOT);
    let removed = dom.render_immediate_to_vec();
    changed_value(&removed, b, "B");
}
