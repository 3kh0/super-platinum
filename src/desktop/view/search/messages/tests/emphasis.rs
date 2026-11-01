use super::*;
use crate::resource_reads::render_test::{button, text};
use dioxus::core::{DynamicNode, TemplateAttribute, TemplateNode, VNode};

fn matches(v: &VNode) -> Vec<String> {
    fn letters(v: &VNode, node: &TemplateNode, out: &mut String) {
        match node {
            TemplateNode::Text { text } => out.push_str(text),
            TemplateNode::Element { children, .. } => {
                for n in children.iter() {
                    letters(v, n, out);
                }
            }
            TemplateNode::Dynamic { id } => match &v.dynamic_nodes[*id] {
                DynamicNode::Text(t) => out.push_str(&t.value),
                DynamicNode::Fragment(nodes) => {
                    for child in nodes {
                        out.push_str(&text(child));
                    }
                }
                _ => {}
            },
        }
    }
    fn at(v: &VNode, node: &TemplateNode, inside: bool, out: &mut Vec<String>) {
        match node {
            TemplateNode::Element {
                tag,
                attrs,
                children,
                ..
            } => {
                let mut inside = inside;
                let mut listeners = 0;
                for attr in attrs.iter() {
                    match attr {
                        TemplateAttribute::Static {
                            name: "class",
                            value,
                            ..
                        } => inside |= value.contains("search-message-body"),
                        TemplateAttribute::Dynamic { id } => {
                            for a in v.dynamic_attrs[*id].iter() {
                                if a.name == "class" {
                                    if let AttributeValue::Text(s) = &a.value {
                                        inside |= s.contains("search-message-body");
                                    }
                                }
                                listeners +=
                                    usize::from(matches!(a.value, AttributeValue::Listener(_)));
                            }
                        }
                        _ => {}
                    }
                }
                if inside {
                    assert_eq!(listeners, 0);
                    assert!(
                        ![
                            "a", "button", "img", "script", "iframe", "audio", "video", "object"
                        ]
                        .contains(tag)
                    );
                    if *tag == "mark" {
                        let mut value = String::new();
                        for n in children.iter() {
                            letters(v, n, &mut value);
                        }
                        out.push(value);
                    }
                }
                for n in children.iter() {
                    at(v, n, inside, out);
                }
            }
            TemplateNode::Dynamic { id } => {
                if let DynamicNode::Fragment(nodes) = &v.dynamic_nodes[*id] {
                    for child in nodes {
                        for root in child.template.roots.iter() {
                            at(child, root, inside, out);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let mut result = Vec::new();
    for root in v.template.roots.iter() {
        at(v, root, false, &mut result);
    }
    result
}

#[tokio::test]
async fn supplied_search_emphasis_preserves_indexed_text_cache_and_deletion_without_query_inference()
 {
    let h = pages::tests::rich::rich_harness().await;
    let mut old = h.state;
    let shell = std::mem::replace(
        &mut *old.write(),
        ShellState::fixture(crate::media::MediaRegistry::default()),
    );
    type Input = Rc<RefCell<Option<ShellState>>>;
    type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
    type Captured = Rc<RefCell<Option<VNode>>>;
    let slot = Slot::default();
    let captured = Captured::default();
    let mut dom = VirtualDom::new_with_props(
        |(input, slot, captured): (Input, Slot, Captured)| {
            let state = use_signal(|| input.borrow_mut().take().unwrap());
            slot.set(Some(state));
            let rendered = {
                let s = state.read();
                rsx! {div{{results(state,&s)}{controls(state,&s)}}}
            };
            *captured.borrow_mut() = Some(rendered.as_ref().unwrap().clone());
            rendered
        },
        (
            Rc::new(RefCell::new(Some(shell))),
            slot.clone(),
            captured.clone(),
        ),
    );
    let mut attributes = Attributes::new();
    record(&mut attributes, dom.rebuild_to_vec().edits);
    let mut state = slot.get().unwrap();
    let io = Rc::new(pages::test_io::Io::default());
    dom.in_scope(ScopeId::ROOT, || provide_context(io.clone()));
    let original = pages::rows(&state.peek())[0].clone();
    let original_body = pages::body(&state.peek(), 0).to_vec();
    let before = {
        let s = state.peek();
        (
            s.core.active_channel.clone(),
            s.channel_generation,
            s.thread_generation,
            s.core.composer.text.clone(),
            s.search_query.clone(),
        )
    };
    assert_eq!(
        matches(captured.borrow().as_ref().unwrap()),
        ["Indexed release"]
    );
    assert!(
        !button(
            captured.borrow().as_ref().unwrap(),
            "message-search-hit-1-0"
        )
        .unwrap()
        .disabled
    );
    assert!(!text(captured.borrow().as_ref().unwrap()).contains(['\u{e000}', '\u{e001}']));
    assert!(format!("{original_body:?}").contains("bold: true"));
    let reply = io.expect(2);
    let source = pages::Control::capture(&state.peek()).unwrap();
    dom.runtime()
        .in_scope(ScopeId::ROOT, || pages::choose(state, source, 2));
    pump(&mut dom, &mut attributes, || io.pages.borrow().len() == 1).await;
    let mut wire = pages::tests::rich::rich_wire(state, 2);
    wire["items"][0]["messages"][0]["blocks"][0]["elements"][0]["elements"][0]["text"] =
        json!("release \u{e000}Different token\u{e001}");
    reply.send(Ok(wire)).unwrap();
    pump(&mut dom, &mut attributes, || {
        !state.peek().search_pager.loading
    })
    .await;
    assert_eq!(
        matches(captured.borrow().as_ref().unwrap()),
        ["Different token"]
    );
    assert_eq!(state.peek().search_query, "release");
    let source = pages::Control::capture(&state.peek()).unwrap();
    dom.runtime()
        .in_scope(ScopeId::ROOT, || pages::choose(state, source, 1));
    record(&mut attributes, dom.render_immediate_to_vec().edits);
    assert_eq!(
        matches(captured.borrow().as_ref().unwrap()),
        ["Indexed release"]
    );
    assert_eq!(pages::rows(&state.peek())[0], original);
    assert_eq!(pages::body(&state.peek(), 0), original_body.as_slice());
    assert_eq!(&*io.pages.borrow(), &[2]);
    let generation = state.peek().core.workspaces["T1"].rt_generation;
    let event=super_platinum_core::slack::realtime::parse_event(&json!({"type":"message","subtype":"message_deleted","channel":original.channel_id,"deleted_ts":original.ts}).to_string()).unwrap();
    crate::realtime::apply_in_view(&mut state.write().core, "T1", generation, event, false);
    record(&mut attributes, dom.render_immediate_to_vec().edits);
    assert!(matches(captured.borrow().as_ref().unwrap()).is_empty());
    assert!(text(captured.borrow().as_ref().unwrap()).contains("This message was deleted."));
    assert!(
        button(
            captured.borrow().as_ref().unwrap(),
            "message-search-hit-1-0"
        )
        .unwrap()
        .disabled
    );
    let s = state.peek();
    assert_eq!(
        (
            s.core.active_channel.clone(),
            s.channel_generation,
            s.thread_generation,
            s.core.composer.text.clone(),
            s.search_query.clone()
        ),
        before
    );
    assert_eq!(&*io.pages.borrow(), &[2]);
}
