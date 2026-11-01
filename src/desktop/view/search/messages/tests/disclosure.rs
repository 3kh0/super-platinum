use super::*;
use crate::resource_reads::render_test::{button, click as click_control, text};
use dioxus::core::{DynamicNode, VNode};

fn expanded(v: &VNode) -> bool {
    v.dynamic_attrs.iter().any(|attrs| attrs.iter().any(|a|
        a.name == "class" && matches!(&a.value, AttributeValue::Text(s) if s.contains("search-message-body expanded"))
    )) || v.dynamic_nodes.iter().any(|n| match n {
        DynamicNode::Fragment(nodes) => nodes.iter().any(expanded), _ => false,
    })
}

#[tokio::test]
async fn actual_show_more_callback_reveals_local_context_without_opening_and_refuses_cached_reentry()
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
    let state = slot.get().unwrap();
    let io = Rc::new(pages::test_io::Io::default());
    dom.in_scope(ScopeId::ROOT, || provide_context(io.clone()));
    let before = {
        let s = state.peek();
        (
            s.core.active_channel.clone(),
            s.channel_generation,
            s.thread_generation,
            s.core.composer.text.clone(),
            s.search_query.clone(),
            pages::status(&s),
        )
    };
    let original = captured.borrow().as_ref().unwrap().clone();
    assert!(!expanded(&original));
    let stale = button(&original, "message-search-expand-1-0").unwrap();
    let current = button(&original, "message-search-expand-1-0").unwrap();
    assert!(!current.disabled);
    click_control(&dom, current);
    record(&mut attributes, dom.render_immediate_to_vec().edits);
    assert!(expanded(captured.borrow().as_ref().unwrap()));
    assert!(
        button(
            captured.borrow().as_ref().unwrap(),
            "message-search-expand-1-0"
        )
        .is_none()
    );
    assert!(
        !button(
            captured.borrow().as_ref().unwrap(),
            "message-search-hit-1-0"
        )
        .unwrap()
        .disabled
    );
    assert!(text(captured.borrow().as_ref().unwrap()).contains("Keep indexed context"));
    assert!(io.pages.borrow().is_empty());
    {
        let s = state.peek();
        assert_eq!(
            (
                s.core.active_channel.clone(),
                s.channel_generation,
                s.thread_generation,
                s.core.composer.text.clone(),
                s.search_query.clone(),
                pages::status(&s)
            ),
            before
        );
        assert!(s.search_opening.is_none());
    }
    let reply = io.expect(2);
    let source = pages::Control::capture(&state.peek()).unwrap();
    dom.runtime()
        .in_scope(ScopeId::ROOT, || pages::choose(state, source, 2));
    pump(&mut dom, &mut attributes, || io.pages.borrow().len() == 1).await;
    let mut wire = pages::tests::rich::rich_wire(state, 2);
    wire["items"][0]["messages"][0]["blocks"][0]["elements"][0]["elements"][0]["text"] =
        json!("Different long page");
    reply.send(Ok(wire)).unwrap();
    pump(&mut dom, &mut attributes, || {
        !state.peek().search_pager.loading
    })
    .await;
    assert!(pages::can_expand(&state.peek(), 0));
    click_control(&dom, stale);
    record(&mut attributes, dom.render_immediate_to_vec().edits);
    assert!(!expanded(captured.borrow().as_ref().unwrap()));
    assert!(text(captured.borrow().as_ref().unwrap()).contains("Different long page"));
    let source = pages::Control::capture(&state.peek()).unwrap();
    dom.runtime()
        .in_scope(ScopeId::ROOT, || pages::choose(state, source, 1));
    record(&mut attributes, dom.render_immediate_to_vec().edits);
    assert!(expanded(captured.borrow().as_ref().unwrap()));
    assert_eq!(&*io.pages.borrow(), &[2]);
    // Credential-free presentation remains disabled even if its listener is
    // deliberately invoked directly; it never mints private Book ownership.
    let fixture_slot = Slot::default();
    let fixture = Captured::default();
    let mut fixture_dom = VirtualDom::new_with_props(
        |(slot, captured): (Slot, Captured)| {
            let state = use_signal(|| {
                let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
                pages::fixture(&mut shell, "search-messages-rich");
                shell
            });
            slot.set(Some(state));
            let rendered = results(state, &state.read());
            *captured.borrow_mut() = Some(rendered.as_ref().unwrap().clone());
            rendered
        },
        (fixture_slot.clone(), fixture.clone()),
    );
    fixture_dom.rebuild_to_vec();
    let fixture_state = fixture_slot.get().unwrap();
    let disabled = button(
        fixture.borrow().as_ref().unwrap(),
        "message-search-expand-1-0",
    )
    .unwrap();
    assert!(disabled.disabled);
    click_control(&fixture_dom, disabled);
    assert!(!pages::has_scope(&fixture_state.peek()));
    assert!(!expanded(fixture.borrow().as_ref().unwrap()));
}
