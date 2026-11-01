use super::*;
use dioxus::core::{
    AttributeValue, DynamicNode, ElementId, ScopeId, TemplateAttribute, TemplateNode, VNode,
    internal::generational_box,
};
use dioxus::document::{Document, Eval, EvalError, Evaluator};
use std::task::{Context, Poll};
struct BookmarkReply(Option<Value>);
impl Evaluator for BookmarkReply {
    fn send(&self, _: Value) -> Result<(), EvalError> {
        Err(EvalError::Unsupported)
    }
    fn poll_recv(&mut self, _: &mut Context<'_>) -> Poll<Result<Value, EvalError>> {
        Poll::Ready(Err(EvalError::Unsupported))
    }
    fn poll_join(&mut self, _: &mut Context<'_>) -> Poll<Result<Value, EvalError>> {
        Poll::Ready(self.0.take().ok_or(EvalError::Unsupported))
    }
}
#[derive(Default)]
struct BookmarkDocument {
    owner: generational_box::Owner,
}
impl Document for BookmarkDocument {
    fn eval(&self, js: String) -> Eval {
        assert!(
            js.contains("document.getElementById('message-timeline')"),
            "only original timeline bookmark evaluator"
        );
        let evaluator: Box<dyn Evaluator> = Box::new(BookmarkReply(Some(Value::Null)));
        Eval::new(self.owner.insert(evaluator))
    }
}
pub(super) struct Ui {
    pub dom: VirtualDom,
    pub state: Signal<ShellState>,
    pub io: Rc<io::Io>,
    _original: Harness,
}
pub(super) fn mount(mut h: Harness) -> Ui {
    static EVENTS: std::sync::Once = std::sync::Once::new();
    EVENTS.call_once(|| {
        dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter))
    });
    let shell = std::mem::replace(
        &mut *h.state.write(),
        ShellState::fixture(crate::media::MediaRegistry::default()),
    );
    type Input = Rc<RefCell<Option<ShellState>>>;
    type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
    let slot = Slot::default();
    let mut dom = VirtualDom::new_with_props(
        |(input, slot): (Input, Slot)| {
            let state = use_signal(|| input.borrow_mut().take().unwrap());
            slot.set(Some(state));
            crate::view::channel_thread_test_surface(state)
        },
        (Rc::new(RefCell::new(Some(shell))), slot.clone()),
    );
    dom.rebuild_to_vec();
    let state = slot.get().unwrap();
    let io = Rc::new(io::Io::default());
    dom.in_scope(ScopeId::ROOT, || {
        provide_context(io.clone());
        let document: Rc<dyn Document> = Rc::new(BookmarkDocument::default());
        provide_context(document);
    });
    Ui {
        dom,
        state,
        io,
        _original: h,
    }
}
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
impl Ui {
    pub fn click(&self, name: &str, value: &str) {
        self.event("click", name, value);
    }
    pub fn double_click(&self, name: &str, value: &str) {
        self.event("dblclick", name, value);
    }
    fn event(&self, event: &str, name: &str, value: &str) {
        let id = mounted_element(&self.dom, name, value);
        let data = dioxus::html::EventData::Mouse(dioxus::html::SerializedMouseData::default());
        self.dom.in_scope(ScopeId::ROOT, || {
            self.dom.runtime().handle_event(
                event,
                dioxus::core::Event::new(data.into_any(), true),
                id,
            )
        });
    }
    pub async fn pump(&mut self, ready: impl Fn() -> bool) {
        tokio::time::timeout(std::time::Duration::from_secs(4), async {
            loop {
                self.dom.process_events();
                self.dom.render_immediate_to_vec();
                if ready() {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("original offline channel Threads action must settle");
    }
    pub fn click_listener_count(&self) -> usize {
        fn walk(dom: &VirtualDom, v: &VNode) -> usize {
            let here = v
                .dynamic_attrs
                .iter()
                .flatten()
                .filter(|a| {
                    matches!(&a.value, AttributeValue::Listener(_))
                        && matches!(a.name, "onclick" | "click")
                })
                .count();
            here + v
                .dynamic_nodes
                .iter()
                .enumerate()
                .map(|(id, n)| match n {
                    DynamicNode::Fragment(nodes) => nodes.iter().map(|v| walk(dom, v)).sum(),
                    DynamicNode::Component(c) => c
                        .mounted_scope(id, v, dom)
                        .map_or(0, |s| walk(dom, s.root_node())),
                    _ => 0,
                })
                .sum::<usize>()
        }
        walk(&self.dom, self.dom.base_scope().root_node())
    }
}

// Actual generated row identities, independent of the projection's return value.
impl Ui {
    pub fn message_ids(&self) -> Vec<String> {
        fn vnode(dom: &VirtualDom, v: &VNode, out: &mut Vec<String>) {
            for n in v.template.roots {
                node(dom, v, n, out);
            }
        }
        fn node(dom: &VirtualDom, v: &VNode, n: &TemplateNode, out: &mut Vec<String>) {
            match n {
                TemplateNode::Element {
                    attrs, children, ..
                } => {
                    for a in *attrs {
                        match a {
                            TemplateAttribute::Static {
                                name: "data-message-id",
                                value,
                                ..
                            } => out.push((*value).into()),
                            TemplateAttribute::Dynamic { id } => {
                                for a in v.dynamic_attrs[*id].iter() {
                                    if a.name == "data-message-id" {
                                        if let AttributeValue::Text(s) = &a.value {
                                            out.push(s.clone());
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    for n in *children {
                        node(dom, v, n, out);
                    }
                }
                TemplateNode::Dynamic { id } => match &v.dynamic_nodes[*id] {
                    DynamicNode::Fragment(nodes) => {
                        for v in nodes {
                            vnode(dom, v, out);
                        }
                    }
                    DynamicNode::Component(c) => {
                        if let Some(scope) = c.mounted_scope(*id, v, dom) {
                            vnode(dom, scope.root_node(), out);
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        let mut out = Vec::new();
        vnode(&self.dom, self.dom.base_scope().root_node(), &mut out);
        out
    }
}
