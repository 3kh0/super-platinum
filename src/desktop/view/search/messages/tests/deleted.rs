use super::*;
use crate::resource_reads::render_test::{button, text};
use dioxus::core::VNode;

#[tokio::test]
async fn owned_search_deleted_cue_survives_held_index_refresh_and_cached_page_without_rewriting_results()
 {
    let h = pages::tests::loaded_harness().await;
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
                rsx! { div { {results(state, &s)} {controls(state, &s)} } }
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
    assert!(text(captured.borrow().as_ref().unwrap()).contains("Page 1 release"));
    assert!(
        !button(
            captured.borrow().as_ref().unwrap(),
            "message-search-hit-1-0"
        )
        .unwrap()
        .disabled
    );
    let (channel, ts, generation) = {
        let s = state.peek();
        let hit = &pages::rows(&s)[0];
        (
            hit.channel_id.clone(),
            hit.ts.clone(),
            s.core.workspaces["T1"].rt_generation,
        )
    };
    let before = {
        let s = state.peek();
        (
            s.core.active_channel.clone(),
            s.channel_generation,
            s.thread_generation,
            s.core.composer.text.clone(),
            s.core.account_epoch,
        )
    };
    let reply = io.expect(1);
    dom.runtime().in_scope(ScopeId::ROOT, || {
        dioxus::core::spawn_forever(async move {
            assert!(pages::refresh_current(state).await);
        })
    });
    pump(&mut dom, &mut attributes, || io.pages.borrow().len() == 1).await;
    let event = super_platinum_core::slack::realtime::parse_event(
        &json!({"type":"message","subtype":"message_deleted","channel":channel,"deleted_ts":ts})
            .to_string(),
    )
    .unwrap();
    crate::realtime::apply_in_view(&mut state.write().core, "T1", generation, event, false);
    record(&mut attributes, dom.render_immediate_to_vec().edits);
    assert!(text(captured.borrow().as_ref().unwrap()).contains("This message was deleted."));
    assert!(!text(captured.borrow().as_ref().unwrap()).contains("Page 1 release"));
    reply.send(Ok(json!({"ok":true,"items":[{"channel":{"id":channel,"name":"ship"},"messages":[{"user":"U_PAGE_AUTHOR","ts":ts,"text":"Page 1 release"}]}],"pagination":{"page":1,"page_count":7,"total_count":121}}))).unwrap();
    pump(&mut dom, &mut attributes, || {
        !state.peek().search_pager.loading
    })
    .await;
    assert!(text(captured.borrow().as_ref().unwrap()).contains("This message was deleted."));
    assert!(
        button(
            captured.borrow().as_ref().unwrap(),
            "message-search-hit-1-0"
        )
        .unwrap()
        .disabled
    );
    assert_eq!(pages::rows(&state.peek())[0].text, "Page 1 release");
    assert_eq!(
        pages::status(&state.peek()),
        Some((1, 1, 7, 121, false, false))
    );
    let reply = io.expect(2);
    let source = pages::Control::capture(&state.peek()).unwrap();
    dom.runtime()
        .in_scope(ScopeId::ROOT, || pages::choose(state, source, 2));
    pump(&mut dom, &mut attributes, || io.pages.borrow().len() == 2).await;
    reply.send(Ok(json!({"ok":true,"items":[{"channel":{"id":channel,"name":"ship"},"messages":[{"user":"U_PAGE_AUTHOR","ts":"120.000002","text":"Page 2 release"}]}],"pagination":{"page":2,"page_count":7,"total_count":121}}))).unwrap();
    pump(&mut dom, &mut attributes, || {
        !state.peek().search_pager.loading
    })
    .await;
    assert!(text(captured.borrow().as_ref().unwrap()).contains("Page 2 release"));
    assert!(!text(captured.borrow().as_ref().unwrap()).contains("This message was deleted."));
    let source = pages::Control::capture(&state.peek()).unwrap();
    dom.runtime()
        .in_scope(ScopeId::ROOT, || pages::choose(state, source, 1));
    record(&mut attributes, dom.render_immediate_to_vec().edits);
    assert_eq!(&*io.pages.borrow(), &[1, 2]);
    assert!(text(captured.borrow().as_ref().unwrap()).contains("This message was deleted."));
    assert_eq!(pages::rows(&state.peek())[0].text, "Page 1 release");
    {
        let s = state.peek();
        assert_eq!(
            (
                s.core.active_channel.clone(),
                s.channel_generation,
                s.thread_generation,
                s.core.composer.text.clone(),
                s.core.account_epoch
            ),
            before
        );
        assert_eq!(pages::status(&s), Some((1, 1, 7, 121, false, false)));
    }
    state.write().set_search_query("newer query".into());
    record(&mut attributes, dom.render_immediate_to_vec().edits);
    assert!(!text(captured.borrow().as_ref().unwrap()).contains("This message was deleted."));
    assert_eq!(&*io.pages.borrow(), &[1, 2]);
}
