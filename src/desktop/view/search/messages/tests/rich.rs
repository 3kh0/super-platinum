use super::*;
use crate::resource_reads::render_test::{button, text};
use dioxus::core::{DynamicNode, TemplateAttribute, TemplateNode, VNode};

fn inspect(v: &VNode) -> (Vec<String>, Vec<String>) {
    fn at(
        v: &VNode,
        node: &TemplateNode,
        inside: bool,
        tags: &mut Vec<String>,
        classes: &mut Vec<String>,
    ) {
        match node {
            TemplateNode::Element {
                tag,
                attrs,
                children,
                ..
            } => {
                let mut names = Vec::new();
                let mut listeners = 0;
                for attr in attrs.iter() {
                    match attr {
                        TemplateAttribute::Static {
                            name: "class",
                            value,
                            ..
                        } => names.push(value.to_string()),
                        TemplateAttribute::Dynamic { id } => {
                            for a in v.dynamic_attrs[*id].iter() {
                                if a.name == "class" {
                                    if let AttributeValue::Text(s) = &a.value {
                                        names.push(s.clone());
                                    }
                                }
                                listeners +=
                                    usize::from(matches!(a.value, AttributeValue::Listener(_)));
                            }
                        }
                        _ => {}
                    }
                }
                let inside = inside || names.iter().any(|s| s.contains("search-message-body"));
                if inside {
                    assert_eq!(listeners, 0, "inert body listener on {tag}");
                    assert!(
                        !["a", "button", "img", "audio", "video", "iframe", "object"].contains(tag)
                    );
                    tags.push(tag.to_string());
                    classes.extend(names);
                }
                for child in children.iter() {
                    at(v, child, inside, tags, classes);
                }
            }
            TemplateNode::Dynamic { id } => {
                if let DynamicNode::Fragment(nodes) = &v.dynamic_nodes[*id] {
                    for child in nodes {
                        for root in child.template.roots.iter() {
                            at(child, root, inside, tags, classes);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let mut tags = Vec::new();
    let mut classes = Vec::new();
    for root in v.template.roots.iter() {
        at(v, root, false, &mut tags, &mut classes);
    }
    (tags, classes)
}

#[tokio::test]
async fn actual_owned_search_rich_reading_is_inert_across_page_cache_and_confirmed_deletion() {
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
    let before = {
        let s = state.peek();
        (
            s.core.active_channel.clone(),
            s.channel_generation,
            s.thread_generation,
            s.core.composer.text.clone(),
        )
    };
    {
        let c = captured.borrow();
        let v = c.as_ref().unwrap();
        let (tags, classes) = inspect(v);
        for tag in ["p", "ol", "code", "blockquote"] {
            assert!(tags.iter().any(|t| t == tag), "{tag}");
        }
        assert!(classes.iter().any(|c| c.contains("bold")));
        let shown = text(v);
        for s in [
            "Indexed release",
            "Second paragraph",
            "Read notes",
            "Search diagram",
            "Review action",
        ] {
            assert!(shown.contains(s), "{s}");
        }
        for s in [
            "Fallback must not duplicate",
            "fixture.invalid",
            "\u{e000}",
            "\u{e001}",
        ] {
            assert!(!shown.contains(s), "{s}");
        }
        assert!(!button(v, "message-search-hit-1-0").unwrap().disabled);
    }
    let reply = io.expect(2);
    let source = pages::Control::capture(&state.peek()).unwrap();
    dom.runtime()
        .in_scope(ScopeId::ROOT, || pages::choose(state, source, 2));
    pump(&mut dom, &mut attributes, || io.pages.borrow().len() == 1).await;
    let mut wire = pages::tests::rich::rich_wire(state, 2);
    wire["items"][0]["messages"][0]["blocks"] = json!([{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Second-page reading","style":{"italic":true}}]}]}]);
    reply.send(Ok(wire)).unwrap();
    pump(&mut dom, &mut attributes, || {
        !state.peek().search_pager.loading
    })
    .await;
    assert!(text(captured.borrow().as_ref().unwrap()).contains("Second-page reading"));
    assert!(
        inspect(captured.borrow().as_ref().unwrap())
            .1
            .iter()
            .any(|c| c.contains("italic"))
    );
    let source = pages::Control::capture(&state.peek()).unwrap();
    dom.runtime()
        .in_scope(ScopeId::ROOT, || pages::choose(state, source, 1));
    record(&mut attributes, dom.render_immediate_to_vec().edits);
    assert!(text(captured.borrow().as_ref().unwrap()).contains("Indexed release"));
    assert!(
        inspect(captured.borrow().as_ref().unwrap())
            .1
            .iter()
            .any(|c| c.contains("bold"))
    );
    assert_eq!(&*io.pages.borrow(), &[2]);
    let (channel, ts, generation) = {
        let s = state.peek();
        let row = &pages::rows(&s)[0];
        (
            row.channel_id.clone(),
            row.ts.clone(),
            s.core.workspaces["T1"].rt_generation,
        )
    };
    let event = super_platinum_core::slack::realtime::parse_event(
        &json!({"type":"message","subtype":"message_deleted","channel":channel,"deleted_ts":ts})
            .to_string(),
    )
    .unwrap();
    crate::realtime::apply_in_view(&mut state.write().core, "T1", generation, event, false);
    record(&mut attributes, dom.render_immediate_to_vec().edits);
    assert!(text(captured.borrow().as_ref().unwrap()).contains("This message was deleted."));
    assert!(!text(captured.borrow().as_ref().unwrap()).contains("Indexed release"));
    assert!(inspect(captured.borrow().as_ref().unwrap()).0.is_empty());
    assert!(
        button(
            captured.borrow().as_ref().unwrap(),
            "message-search-hit-1-0"
        )
        .unwrap()
        .disabled
    );
    assert!(pages::RowSource::capture(&state.peek(), 0).is_none());
    let s = state.peek();
    assert_eq!(
        (
            s.core.active_channel.clone(),
            s.channel_generation,
            s.thread_generation,
            s.core.composer.text.clone()
        ),
        before
    );
    assert_eq!(&*io.pages.borrow(), &[2]);
}
