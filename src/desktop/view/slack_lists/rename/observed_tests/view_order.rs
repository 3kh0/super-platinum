//! Actual generated modal structure; native focus is covered by the real JS helper.
use super::*;
#[test]
fn real_view_order_fixture_renders_fixed_before_after_and_shared_save() {
    for alias in ["slack-list-view-order", "slack-list-view-order-narrow"] {
        let result = Rc::new(RefCell::new(None));
        let mut dom = VirtualDom::new_with_props(
            |(result, alias): (Rc<RefCell<Option<Node>>>, &'static str)| {
                let state = use_signal(|| {
                    ShellState::fixture_variant(crate::media::MediaRegistry::default(), alias)
                });
                let rendered = panel(state);
                *result.borrow_mut() = Some(Node {
                    children: expand(rendered.as_ref().unwrap()),
                    ..Default::default()
                });
                let serial = rename::presentation(&state.peek()).unwrap().serial;
                rename::close(state, serial);
                rendered
            },
            (result.clone(), alias),
        );
        dom.rebuild_to_vec();
        let tree = result.borrow_mut().take().unwrap();
        let panel = tree.all("id", "slack-list-rename")[0];
        assert_eq!(panel.attr("data-reorder-direction"), "up");
        assert_eq!(panel.attr("data-rename-view"), "V_ORDER_SELECTED");
        assert_eq!(
            panel.all("id", "slack-list-rename-title")[0].text(),
            "Move view"
        );
        assert_eq!(
            panel.all("id", "slack-list-view-order-before")[0].text(),
            "PlanningDeliveryArchive"
        );
        assert_eq!(
            panel.all("id", "slack-list-view-order-after")[0].text(),
            "DeliveryPlanningArchive"
        );
        assert!(panel.all("id", "slack-list-rename-name").is_empty());
        let save = panel.all("id", "slack-list-rename-confirm")[0];
        assert_eq!(save.text(), "Save for everyone");
        assert_eq!(save.attr("disabled"), "false");
        assert_eq!(
            panel.all("id", "slack-list-rename-close")[0].text(),
            "Cancel"
        );
    }
}

#[test]
fn generated_cancel_removes_real_receipt_and_retired_save_cannot_start_another_move() {
    use dioxus::core::{AttributeValue, ScopeId};
    use std::cell::Cell;
    dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
    type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
    let slot = Slot::default();
    let save = Rc::new(RefCell::new(None));
    let cancel = Rc::new(RefCell::new(None));
    let mut dom = VirtualDom::new_with_props(
        |(slot, save, cancel): (
            Slot,
            Rc<RefCell<Option<AttributeValue>>>,
            Rc<RefCell<Option<AttributeValue>>>,
        )| {
            let state = use_signal(|| {
                ShellState::fixture_variant(
                    crate::media::MediaRegistry::default(),
                    "slack-list-view-order",
                )
            });
            slot.set(Some(state));
            let rendered = panel(state);
            if save.borrow().is_none() {
                *save.borrow_mut() =
                    listener(rendered.as_ref().unwrap(), "slack-list-rename-confirm");
            }
            if cancel.borrow().is_none() {
                *cancel.borrow_mut() =
                    listener(rendered.as_ref().unwrap(), "slack-list-rename-close");
            }
            rendered
        },
        (slot.clone(), save.clone(), cancel.clone()),
    );
    dom.rebuild_to_vec();
    let state = slot.get().unwrap();
    let old = rename::presentation(&state.peek()).unwrap();
    // Close through the actual host action, then create a new immutable opposite move.
    let AttributeValue::Listener(cancel) = cancel.borrow_mut().take().unwrap() else {
        panic!()
    };
    let data = dioxus::html::EventData::Mouse(dioxus::html::SerializedMouseData::default());
    dom.in_runtime(|| cancel.call(dioxus::core::Event::new(data.into_any(), true)));
    assert!(rename::presentation(&state.peek()).is_none());
    dom.in_runtime(|| {
        let visit = crate::slack_lists::generation(&state.peek()).unwrap();
        let selection = rename::selection(&state.peek()).unwrap();
        rename::view_order::open(
            state,
            visit,
            selection,
            "V_ORDER_SELECTED".into(),
            rename::view_order::Direction::Down,
        );
    });
    let fresh = rename::presentation(&state.peek()).unwrap();
    assert_ne!(old.serial, fresh.serial);
    let AttributeValue::Listener(callback) = save.borrow_mut().take().unwrap() else {
        panic!()
    };
    let data = dioxus::html::EventData::Mouse(dioxus::html::SerializedMouseData::default());
    dom.in_runtime(|| callback.call(dioxus::core::Event::new(data.into_any(), true)));
    assert_eq!(
        rename::presentation(&state.peek()).unwrap().stage,
        Stage::Editing
    );
    assert_eq!(
        rename::presentation(&state.peek())
            .unwrap()
            .reorder
            .unwrap()
            .direction,
        "down"
    );
    dom.mark_dirty(ScopeId::ROOT);
    dom.render_immediate_to_vec();
    dom.in_runtime(|| rename::close(state, fresh.serial));
}

fn listener(v: &VNode, wanted: &str) -> Option<AttributeValue> {
    v.template.roots.iter().find_map(|n| at(v, n, wanted))
}
fn at(v: &VNode, n: &TemplateNode, wanted: &str) -> Option<AttributeValue> {
    match n {
        TemplateNode::Element {
            attrs, children, ..
        } => {
            let mut confirm = false;
            let mut click = None;
            for a in attrs.iter() {
                match a {
                    TemplateAttribute::Static {
                        name: "id", value, ..
                    } => confirm = *value == wanted,
                    TemplateAttribute::Dynamic { id } => {
                        for a in v.dynamic_attrs[*id].iter() {
                            if a.name == "id"
                                && matches!(&a.value,AttributeValue::Text(t) if t==wanted)
                            {
                                confirm = true;
                            }
                            if a.name == "onclick" {
                                click = Some(a.value.clone());
                            }
                        }
                    }
                    _ => {}
                }
            }
            if confirm {
                return click;
            }
            children.iter().find_map(|n| at(v, n, wanted))
        }
        TemplateNode::Dynamic { id } => match &v.dynamic_nodes[*id] {
            DynamicNode::Fragment(v) => v.iter().find_map(|v| listener(v, wanted)),
            _ => None,
        },
        _ => None,
    }
}
