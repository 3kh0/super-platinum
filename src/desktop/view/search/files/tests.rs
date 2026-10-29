use super::*;
use dioxus::core::{
    AttributeValue, DynamicNode, ElementId, Mutation, ScopeId, TemplateAttribute, TemplateNode,
    VNode,
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};
type Attributes = BTreeMap<(ElementId, String), String>;
fn record(a: &mut Attributes, edits: Vec<Mutation>) {
    for e in edits {
        if let Mutation::SetAttribute {
            id, name, value, ..
        } = e
        {
            let v = match value {
                AttributeValue::Text(v) => v,
                AttributeValue::Bool(v) => v.to_string(),
                _ => continue,
            };
            a.insert((id, name.into()), v);
        }
    }
}
/// Literal attributes live in pinned Dioxus templates, not SetAttribute edits.
/// Resolve the actual mounted event-bearing element without changing product
/// markup or substituting direct calls for the real shared button callbacks.
fn mounted_element(dom: &VirtualDom, name: &str, value: &str) -> ElementId {
    fn find(dom: &VirtualDom, v: &VNode, name: &str, value: &str) -> Option<ElementId> {
        v.template
            .roots
            .iter()
            .find_map(|n| node(dom, v, n, name, value))
    }
    fn node(
        dom: &VirtualDom,
        v: &VNode,
        n: &TemplateNode,
        name: &str,
        value: &str,
    ) -> Option<ElementId> {
        match n {
            TemplateNode::Element {
                attrs, children, ..
            } => {
                let matched = attrs.iter().any(|a| match a {
                    TemplateAttribute::Static {
                        name: k, value: v, ..
                    } => *k == name && *v == value,
                    TemplateAttribute::Dynamic { id } => v.dynamic_attrs[*id].iter().any(|a| {
                        a.name == name && matches!(&a.value,AttributeValue::Text(s)if s==value)
                    }),
                });
                if matched {
                    if let Some(id) = attrs.iter().find_map(|a| match a {
                        TemplateAttribute::Dynamic { id } => v.mounted_dynamic_attribute(*id, dom),
                        _ => None,
                    }) {
                        return Some(id);
                    }
                }
                children.iter().find_map(|n| node(dom, v, n, name, value))
            }
            TemplateNode::Dynamic { id } => match &v.dynamic_nodes[*id] {
                DynamicNode::Fragment(nodes) => {
                    nodes.iter().find_map(|v| find(dom, v, name, value))
                }
                DynamicNode::Component(c) => c
                    .mounted_scope(*id, v, dom)
                    .and_then(|scope| find(dom, scope.root_node(), name, value)),
                _ => None,
            },
            TemplateNode::Text { .. } => None,
        }
    }
    find(dom, dom.base_scope().root_node(), name, value)
        .unwrap_or_else(|| panic!("missing mounted template attribute {name}={value}"))
}
fn click(dom: &VirtualDom, id: ElementId) {
    let data = dioxus::html::EventData::Mouse(dioxus::html::SerializedMouseData::default());
    dom.in_scope(ScopeId::ROOT, || {
        dom.runtime()
            .handle_event("click", dioxus::core::Event::new(data.into_any(), true), id)
    });
}
fn change(dom: &VirtualDom, id: ElementId, value: &str) {
    let data = dioxus::html::EventData::Form(dioxus::html::SerializedFormData::new(
        value.into(),
        Vec::new(),
    ));
    dom.in_scope(ScopeId::ROOT, || {
        dom.runtime().handle_event(
            "change",
            dioxus::core::Event::new(data.into_any(), true),
            id,
        )
    });
}
#[track_caller]
fn pump<'a>(
    dom: &'a mut VirtualDom,
    a: &'a mut Attributes,
    ready: impl Fn() -> bool + 'a,
) -> impl std::future::Future<Output = ()> + 'a {
    let caller = std::panic::Location::caller();
    async move {
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            loop {
                // Public pinned API polls notified tasks even if they rightly
                // produce no dirty UI scope (a retired response is such a task).
                dom.process_events();
                record(a, dom.render_immediate_to_vec().edits);
                if ready() {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap_or_else(|error| {
            panic!("owned offline Files control from {caller} must settle: {error}")
        });
    }
}
#[tokio::test]
async fn actual_generated_sort_page_cached_empty_and_exact_retry_callbacks_preserve_returned_occurrences_without_open_actions()
 {
    static EVENTS: std::sync::Once = std::sync::Once::new();
    EVENTS.call_once(|| {
        dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter))
    });
    let h = pages::tests::setup(true).await;
    let mut old = h.h.state;
    let shell = std::mem::replace(
        &mut *old.write(),
        ShellState::fixture(crate::media::MediaRegistry::default()),
    );
    type Input = Rc<RefCell<Option<ShellState>>>;
    type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
    let slot = Slot::default();
    let mut dom = VirtualDom::new_with_props(
        |(input, slot): (Input, Slot)| {
            let state = use_signal(|| input.borrow_mut().take().unwrap());
            slot.set(Some(state));
            rsx! {{super::super::controls(state,&state.read())}{results(state,&state.read())}{controls(state,&state.read())}}
        },
        (Rc::new(RefCell::new(Some(shell))), slot.clone()),
    );
    let mut attributes = Attributes::new();
    record(&mut attributes, dom.rebuild_to_vec().edits);
    let state = slot.get().unwrap();
    let io = Rc::new(pages::test_io::Io::default());
    dom.in_scope(ScopeId::ROOT, || provide_context(io.clone()));
    let sort = mounted_element(&dom, "id", SORT_ID);
    assert_eq!(attributes[&(sort, "disabled".into())], "false");
    let reply = io.expect(1);
    change(&dom, sort, "created");
    pump(&mut dom, &mut attributes, || io.pages.borrow().len() == 1).await;
    reply.send(Ok(pages::tests::response(1, false))).unwrap();
    pump(&mut dom, &mut attributes, || {
        pages::status(&state.peek()).is_some_and(|v| !v.4)
    })
    .await;
    assert_eq!(
        pages::rows(&state.peek())
            .iter()
            .map(|h| h.id())
            .collect::<Vec<_>>(),
        ["FZ", "FA", "FZ"]
    );
    let next = mounted_element(&dom, "id", NEXT_ID);
    let reply = io.expect(2);
    click(&dom, next);
    click(&dom, next);
    assert!(pages::rows(&state.peek()).is_empty());
    pump(&mut dom, &mut attributes, || io.pages.borrow().len() == 2).await;
    reply.send(Ok(pages::tests::response(2, true))).unwrap();
    pump(&mut dom, &mut attributes, || {
        pages::status(&state.peek()).is_some_and(|v| v.0 == 2 && !v.4)
    })
    .await;
    click(&dom, mounted_element(&dom, "id", PREVIOUS_ID));
    record(&mut attributes, dom.render_immediate_to_vec().edits);
    assert_eq!(pages::status(&state.peek()).unwrap().0, 1);
    assert_eq!(&*io.pages.borrow(), &[1, 2]);
    let fourth = mounted_element(&dom, "aria-label", "File page 4");
    let reply = io.expect(4);
    click(&dom, fourth);
    pump(&mut dom, &mut attributes, || io.pages.borrow().len() == 3).await;
    reply
        .send(Err(super_platinum_core::slack::Error::Offline(
            "synthetic outage".into(),
        )))
        .unwrap();
    pump(&mut dom, &mut attributes, || {
        pages::status(&state.peek()).is_some_and(|v| v.5)
    })
    .await;
    assert_eq!(pages::status(&state.peek()).unwrap().0, 1);
    assert_eq!(pages::rows(&state.peek()).len(), 3);
    let retry = mounted_element(&dom, "id", RETRY_ID);
    let reply = io.expect(4);
    click(&dom, retry);
    pump(&mut dom, &mut attributes, || io.pages.borrow().len() == 4).await;
    reply.send(Ok(pages::tests::response(4, true))).unwrap();
    pump(&mut dom, &mut attributes, || {
        pages::status(&state.peek()).is_some_and(|v| v.0 == 4 && !v.4)
    })
    .await;
    let reply = io.expect(1);
    let sort = mounted_element(&dom, "id", SORT_ID);
    change(&dom, sort, "name_desc");
    pump(&mut dom, &mut attributes, || io.pages.borrow().len() == 5).await;
    reply.send(Ok(pages::tests::response(1, false))).unwrap();
    pump(&mut dom, &mut attributes, || {
        pages::status(&state.peek()).is_some_and(|v| v.0 == 1 && !v.4)
    })
    .await;
    assert_eq!(
        pages::status(&state.peek()).unwrap().6,
        Sort::NameDescending
    );
    assert_eq!(&*io.pages.borrow(), &[1, 2, 4, 4, 1]);
    for panel in ["history", "filters"] {
        let expected = io.pages.borrow().len() + 1;
        let next = mounted_element(&dom, "id", NEXT_ID);
        let stale = io.expect(2);
        click(&dom, next);
        assert!(
            pages::status(&state.peek()).is_some_and(|v| v.4 && v.1 == 2),
            "{panel}: actual current Next must admit uncached target2"
        );
        pump(&mut dom, &mut attributes, || {
            io.pages.borrow().len() == expected
        })
        .await;
        let (attribute, value) = if panel == "history" {
            ("aria-label", "Recent searches")
        } else {
            ("aria-controls", "search-filter-panel")
        };
        let generation = state.peek().search_open_generation;
        click(&dom, mounted_element(&dom, attribute, value));
        record(&mut attributes, dom.render_immediate_to_vec().edits);
        assert!(state.peek().search_open_generation != generation);
        click(&dom, mounted_element(&dom, attribute, value));
        record(&mut attributes, dom.render_immediate_to_vec().edits);
        assert!(
            !pages::active(&state.peek()),
            "panel ABA must not revive the old frame"
        );
        let fresh = io.expect(1);
        let sort = mounted_element(&dom, "id", SORT_ID);
        change(&dom, sort, "created");
        pump(&mut dom, &mut attributes, || {
            io.pages.borrow().len() == expected + 1
        })
        .await;
        let completed = io.completed.get();
        stale.send(Ok(pages::tests::response(2, false))).unwrap();
        pump(&mut dom, &mut attributes, || {
            io.completed.get() == completed + 1
        })
        .await;
        assert!(
            pages::status(&state.peek()).is_some_and(|v| v.4 && v.1 == 1),
            "retired page2 cannot release successor page1"
        );
        fresh.send(Ok(pages::tests::response(1, false))).unwrap();
        pump(&mut dom, &mut attributes, || {
            pages::status(&state.peek()).is_some_and(|v| v.0 == 1 && !v.4)
        })
        .await;
    }
    assert_eq!(&*io.pages.borrow(), &[1, 2, 4, 4, 1, 2, 1, 2, 1]);

    assert!(state.peek().viewer.is_none());
    let mut rows_dom = VirtualDom::new_with_props(
        |input: Input| {
            let state = use_signal(|| input.borrow_mut().take().unwrap());
            results(state, &state.read())
        },
        Rc::new(RefCell::new(Some(std::mem::replace(
            &mut *slot.get().unwrap().write(),
            ShellState::fixture(crate::media::MediaRegistry::default()),
        )))),
    );
    let edits = rows_dom.rebuild_to_vec();
    assert!(
        !edits
            .edits
            .iter()
            .any(|m| matches!(m, Mutation::NewEventListener { .. }))
    );
    assert!(!format!("{edits:?}").contains("untrusted.invalid"));
}

mod saved;

mod file_types;

mod message_scopes;
