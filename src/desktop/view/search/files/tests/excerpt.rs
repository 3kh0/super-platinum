//! Actual original Files request, typed decode, Book, article and pager callbacks.
use super::*;
use crate::resource_reads::render_test::{self, Captured};
use serde_json::{Value, json};

fn response(page: u32, label: &str) -> Value {
    let mut raw = pages::tests::response(page, false);
    for (index, item) in raw["items"].as_array_mut().unwrap().iter_mut().enumerate() {
        let object = item.as_object_mut().unwrap();
        object.insert("mode".into(), json!("snippet"));
        object.insert("filetype".into(), json!("text"));
        object.insert("mimetype".into(), json!("text/plain"));
        object.insert("created".into(), json!(1710000000));
        object.insert("file_access".into(), json!("visible"));
        object.insert(
            "contents_extracts".into(),
            json!([format!(
                "\u{feff}\t  {label} occurrence {index} <b>literal</b> \u{e000}match\u{e001} tail"
            )]),
        );
    }
    raw
}
/// Inspect only the new subtree: all excerpt content must be literal text or
/// inert mark nodes, with no URL/resource/event-bearing attribute or component.
fn excerpts(captured: &Captured) -> Vec<String> {
    fn plain(v: &VNode, n: &TemplateNode, out: &mut String) {
        match n {
            TemplateNode::Text { text } => out.push_str(text),
            TemplateNode::Element { children, .. } => {
                for c in children.iter() {
                    plain(v, c, out)
                }
            }
            TemplateNode::Dynamic { id } => match &v.dynamic_nodes[*id] {
                DynamicNode::Text(t) => out.push_str(&t.value),
                DynamicNode::Fragment(nodes) => {
                    for child in nodes.iter() {
                        for root in child.template.roots.iter() {
                            plain(child, root, out)
                        }
                    }
                }
                DynamicNode::Placeholder(_) => {}
                _ => panic!("excerpt contains no component, placeholder, or delegated renderer"),
            },
        }
    }
    fn inspect(v: &VNode, n: &TemplateNode) -> usize {
        match n {
            TemplateNode::Element {
                tag,
                attrs,
                children,
                ..
            } => {
                assert!(matches!(*tag, "span" | "mark"));
                for a in attrs.iter() {
                    match a {
                        TemplateAttribute::Static { name, .. } => {
                            assert!(matches!(*name, "class" | "data-file-search-excerpt"))
                        }
                        TemplateAttribute::Dynamic { id } => {
                            for a in v.dynamic_attrs[*id].iter() {
                                assert!(matches!(a.name, "class" | "data-file-search-excerpt"));
                                assert!(!matches!(&a.value, AttributeValue::Listener(_)));
                            }
                        }
                    }
                }
                usize::from(*tag == "mark") + children.iter().map(|c| inspect(v, c)).sum::<usize>()
            }
            TemplateNode::Dynamic { id } => match &v.dynamic_nodes[*id] {
                DynamicNode::Text(_) | DynamicNode::Placeholder(_) => 0,
                DynamicNode::Fragment(nodes) => nodes
                    .iter()
                    .map(|child| {
                        child
                            .template
                            .roots
                            .iter()
                            .map(|root| inspect(child, root))
                            .sum::<usize>()
                    })
                    .sum(),
                _ => panic!("no component/resource/control in supplied excerpt"),
            },
            TemplateNode::Text { .. } => 0,
        }
    }
    fn at(v: &VNode, n: &TemplateNode, out: &mut Vec<String>) {
        match n {
            TemplateNode::Element {
                attrs, children, ..
            } => {
                let marker = attrs.iter().any(|a| match a {
                    TemplateAttribute::Static { name, value, .. } => {
                        *name == "data-file-search-excerpt" && *value == "true"
                    }
                    TemplateAttribute::Dynamic { id } => v.dynamic_attrs[*id].iter().any(|a| {
                        a.name == "data-file-search-excerpt"
                            && matches!(&a.value,AttributeValue::Text(s)if s=="true")
                    }),
                });
                if marker {
                    assert_eq!(
                        inspect(v, n),
                        1,
                        "each original excerpt preserves its paired highlight"
                    );
                    let mut text = String::new();
                    plain(v, n, &mut text);
                    out.push(text)
                } else {
                    for c in children.iter() {
                        at(v, c, out)
                    }
                }
            }
            TemplateNode::Dynamic { id } => {
                if let DynamicNode::Fragment(nodes) = &v.dynamic_nodes[*id] {
                    for child in nodes.iter() {
                        for root in child.template.roots.iter() {
                            at(child, root, out)
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let mut result = Vec::new();
    let value = captured.borrow();
    let v = value.as_ref().unwrap();
    for root in v.template.roots.iter() {
        at(v, root, &mut result)
    }
    result
}
fn expected(label: &str) -> Vec<String> {
    (0..3)
        .map(|i| format!("{label} occurrence {i} <b>literal</b> match tail"))
        .collect()
}

#[tokio::test]
async fn files_excerpt_original_rows_duplicate_occurrences_cached_paging_outage_retry_and_deletion_keep_inert_reading()
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
    let captured = Captured::default();
    let mut dom = VirtualDom::new_with_props(
        |(input, slot, captured): (Input, Slot, Captured)| {
            let state = use_signal(|| input.borrow_mut().take().unwrap());
            slot.set(Some(state));
            let rendered = rsx! {div{{super::super::super::controls(state,&state.read())}{results(state,&state.read())}{controls(state,&state.read())}}};
            *captured.borrow_mut() = Some(rendered.as_ref().unwrap().clone());
            rendered
        },
        (
            Rc::new(RefCell::new(Some(shell))),
            slot.clone(),
            captured.clone(),
        ),
    );
    let mut attrs = Attributes::new();
    record(&mut attrs, dom.rebuild_to_vec().edits);
    let mut state = slot.get().unwrap();
    let before_files = serde_json::to_value(&state.peek().core.files.items).unwrap();
    let io = Rc::new(pages::test_io::Io::default());
    dom.in_scope(ScopeId::ROOT, || provide_context(io.clone()));
    let reply = io.expect(1);
    change(&dom, mounted_element(&dom, "id", SORT_ID), "created");
    pump(&mut dom, &mut attrs, || io.pages.borrow().len() == 1).await;
    reply.send(Ok(response(1, "First"))).unwrap();
    pump(&mut dom, &mut attrs, || {
        pages::status(&state.peek()).is_some_and(|v| !v.4)
    })
    .await;
    assert_eq!(excerpts(&captured), expected("First"));
    assert_eq!(
        pages::rows(&state.peek())
            .iter()
            .map(|h| h.id())
            .collect::<Vec<_>>(),
        ["FZ", "FA", "FZ"]
    );
    assert!(render_test::text(captured.borrow().as_ref().unwrap()).contains("<b>literal</b>"));
    assert_eq!(
        serde_json::to_value(&state.peek().core.files.items).unwrap(),
        before_files,
        "display projection cannot seed a canonical File"
    );
    assert_eq!(
        state
            .peek()
            .media
            .registered_source_count("https://untrusted.invalid"),
        0
    );

    let reply = io.expect(2);
    click(&dom, mounted_element(&dom, "id", NEXT_ID));
    pump(&mut dom, &mut attrs, || io.pages.borrow().len() == 2).await;
    assert!(
        excerpts(&captured).is_empty(),
        "pending retains data but does not borrow the previous page's article"
    );
    reply.send(Ok(response(2, "Second"))).unwrap();
    pump(&mut dom, &mut attrs, || {
        pages::status(&state.peek()).is_some_and(|v| v.0 == 2 && !v.4)
    })
    .await;
    assert_eq!(excerpts(&captured), expected("Second"));
    click(&dom, mounted_element(&dom, "id", PREVIOUS_ID));
    record(&mut attrs, dom.render_immediate_to_vec().edits);
    assert_eq!(excerpts(&captured), expected("First"));
    assert_eq!(&*io.pages.borrow(), &[1, 2]);

    let reply = io.expect(4);
    click(&dom, mounted_element(&dom, "aria-label", "File page 4"));
    pump(&mut dom, &mut attrs, || io.pages.borrow().len() == 3).await;
    reply
        .send(Err(super_platinum_core::slack::Error::Offline(
            "synthetic excerpt outage".into(),
        )))
        .unwrap();
    pump(&mut dom, &mut attrs, || {
        pages::status(&state.peek()).is_some_and(|v| v.5)
    })
    .await;
    assert_eq!(
        excerpts(&captured),
        expected("First"),
        "failed target preserves loaded original occurrences"
    );
    let reply = io.expect(4);
    click(&dom, mounted_element(&dom, "id", RETRY_ID));
    pump(&mut dom, &mut attrs, || io.pages.borrow().len() == 4).await;
    reply.send(Ok(response(4, "Recovered"))).unwrap();
    pump(&mut dom, &mut attrs, || {
        pages::status(&state.peek()).is_some_and(|v| v.0 == 4 && !v.4)
    })
    .await;
    assert_eq!(excerpts(&captured), expected("Recovered"));
    assert!(state.peek().viewer.is_none());
    assert_eq!(
        serde_json::to_value(&state.peek().core.files.items).unwrap(),
        before_files
    );
    assert_eq!(
        state
            .peek()
            .media
            .registered_source_count("https://untrusted.invalid"),
        0
    );
    let team = state.peek().core.active_team.clone().unwrap();
    let generation = state.peek().core.workspaces[&team].rt_generation;
    let event = super_platinum_core::slack::realtime::parse_event(
        r#"{"type":"file_deleted","file_id":"FZ"}"#,
    )
    .unwrap();
    crate::realtime::apply_in_view_with_context(
        &mut state.write(),
        &team,
        generation,
        event,
        false,
        true,
    );
    record(&mut attrs, dom.render_immediate_to_vec().edits);
    assert_eq!(excerpts(&captured), vec![expected("Recovered")[1].clone()]);
    assert_eq!(
        pages::rows(&state.peek()).len(),
        3,
        "visibility veto does not rewrite archived original page data"
    );
    assert_eq!(&*io.pages.borrow(), &[1, 2, 4, 4]);
}
