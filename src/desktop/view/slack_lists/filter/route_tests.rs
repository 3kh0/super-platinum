//! Invoke the actual shared panel's generated onclick, not the confirm helper.
use super::*;
use dioxus::core::{AttributeValue, DynamicNode, TemplateAttribute, TemplateNode, VNode};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
type Click = Rc<RefCell<Option<AttributeValue>>>;
fn listener(v: &VNode) -> Option<AttributeValue> {
    v.template.roots.iter().find_map(|n| at(v, n))
}
fn at(v: &VNode, n: &TemplateNode) -> Option<AttributeValue> {
    match n {
        TemplateNode::Element {
            attrs, children, ..
        } => {
            let mut confirm = false;
            let mut click = None;
            for a in attrs.iter() {
                match a {
                    TemplateAttribute::Static {
                        name: "id",
                        value: "slack-list-rename-confirm",
                        ..
                    } => confirm = true,
                    TemplateAttribute::Dynamic { id } => {
                        for a in v.dynamic_attrs[*id].iter() {
                            if a.name == "id"
                                && matches!(&a.value,AttributeValue::Text(t) if t=="slack-list-rename-confirm")
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
            children.iter().find_map(|n| at(v, n))
        }
        TemplateNode::Dynamic { id } => match &v.dynamic_nodes[*id] {
            DynamicNode::Fragment(v) => v.iter().find_map(listener),
            _ => None,
        },
        _ => None,
    }
}
fn harness(name: &'static str) -> (VirtualDom, Signal<ShellState>, AttributeValue) {
    let slot = Slot::default();
    let click = Click::default();
    let mut dom = VirtualDom::new_with_props(
        |(name, slot, click): (&'static str, Slot, Click)| {
            let state = use_signal(move || {
                ShellState::fixture_variant(crate::media::MediaRegistry::default(), name)
            });
            use_hook(move || {
                if name == "slack-list-number-precision" {
                    let p = rename::presentation(&state.peek()).unwrap();
                    let visit = crate::slack_lists::generation(&state.peek()).unwrap();
                    rename::precision::input(state, p.serial, visit, 2);
                }
            });
            slot.set(Some(state));
            let rendered = super::super::rename::panel(state);
            *click.borrow_mut() = listener(rendered.as_ref().unwrap());
            rendered
        },
        (name, slot.clone(), click.clone()),
    );
    dom.rebuild_to_vec();
    assert!(
        rename::presentation(&slot.get().unwrap().peek())
            .unwrap()
            .can_confirm
    );
    let callback = click
        .borrow_mut()
        .take()
        .expect("actual shared confirm onclick");
    (dom, slot.get().unwrap(), callback)
}
fn invoke(dom: &VirtualDom, callback: AttributeValue) {
    static EVENTS: std::sync::Once = std::sync::Once::new();
    EVENTS.call_once(|| {
        dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter))
    });
    let AttributeValue::Listener(callback) = callback else {
        panic!("not a listener")
    };
    let data = dioxus::html::EventData::Mouse(dioxus::html::SerializedMouseData::default());
    dom.in_runtime(|| callback.call(dioxus::core::Event::new(data.into_any(), true)));
}
#[tokio::test(flavor = "current_thread")]
async fn shared_panel_create_and_precision_execute_non_filter_fallback() {
    for name in [
        "slack-list-text-field-create",
        "slack-list-number-precision",
    ] {
        let (mut dom, state, callback) = harness(name);
        let before = rename::presentation(&state.peek()).unwrap();
        assert!(before.filter.is_none());
        invoke(&dom, callback);
        let after = rename::presentation(&state.peek()).unwrap();
        assert_eq!(after.serial, before.serial);
        assert_eq!(after.stage, rename::Stage::Preparing);
        assert!(state.peek().core.session.is_none() && state.peek().core.transport.is_none());
        // The disconnected fixture refuses preflight; allow its actual task to settle
        // so normal Close releases this exact registry entry. No HTTP capability exists.
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while rename::presentation(&state.peek()).unwrap().stage == rename::Stage::Preparing {
                dom.wait_for_work().await;
                dom.render_immediate_to_vec();
            }
        })
        .await
        .unwrap();
        assert_eq!(
            rename::presentation(&state.peek()).unwrap().stage,
            rename::Stage::NotRequested
        );
        dom.in_runtime(|| rename::close(state, before.serial));
        assert!(rename::presentation(&state.peek()).is_none());
    }
}
#[test]
fn shared_panel_filter_reserves_complete_capture_without_generic_prepare() {
    let (dom, state, callback) = harness("slack-list-compound-filter");
    let before = rename::presentation(&state.peek()).unwrap();
    let p = before.filter.unwrap();
    let visit = crate::slack_lists::generation(&state.peek()).unwrap();
    invoke(&dom, callback);
    assert_eq!(
        rename::presentation(&state.peek()).unwrap().stage,
        rename::Stage::Editing
    );
    assert!(
        filter::reserve(&state.peek(), before.serial, visit, p.choice).is_none(),
        "exact capture already reserved synchronously"
    );
    dom.in_runtime(|| rename::confirm(state, before.serial));
    assert_eq!(
        rename::presentation(&state.peek()).unwrap().stage,
        rename::Stage::Editing,
        "generic confirm cannot bypass Filter snapshot"
    );
    dom.in_runtime(|| rename::close(state, before.serial));
    assert!(rename::presentation(&state.peek()).is_none());
}
