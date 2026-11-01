use super::*;
use crate::resource_reads::render_test::{button, click as click_control};
use dioxus::core::VNode;

type Input = Rc<RefCell<Option<ShellState>>>;
type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
type Captured = Rc<RefCell<Option<VNode>>>;
fn mount(shell: ShellState) -> (VirtualDom, Signal<ShellState>, Captured) {
    let slot = Slot::default();
    let captured = Captured::default();
    let mut dom = VirtualDom::new_with_props(
        |(input, slot, captured): (Input, Slot, Captured)| {
            let state = use_signal(|| input.borrow_mut().take().unwrap());
            slot.set(Some(state));
            let rendered = {
                let s = state.read();
                if s.overlay == Some(crate::state::Overlay::Search) {
                    rsx! {div{{results(state,&s)}{controls(state,&s)}}}
                } else {
                    rsx! {div{}}
                }
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
    dom.rebuild_in_place();
    (dom, slot.get().unwrap(), captured)
}
async fn settle(dom: &mut VirtualDom, ready: impl Fn() -> bool) {
    tokio::time::timeout(std::time::Duration::from_secs(4), async {
        loop {
            dom.render_immediate_to_vec();
            if ready() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("actual root navigation must settle");
}

// Ancillary subscription/hydration work must also remain disconnected. The
// current desktop gate already explicitly enables this transport protection.
#[cfg(feature = "protected-native-fixture")]
#[tokio::test]
async fn search_root_thread_mounted_callback_reads_exact_root_failure_retry_and_retires_held_work()
{
    for alias in [
        "search-messages-root-thread",
        "search-messages-root-thread-narrow",
    ] {
        let mut fixture = VirtualDom::new_with_props(
            |name: &'static str| {
                let state = use_signal(|| {
                    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
                    pages::fixture(&mut s, name);
                    s
                });
                let rendered = results(state, &state.read());
                assert!(
                    button(rendered.as_ref().unwrap(), "message-search-thread-1-0")
                        .unwrap()
                        .disabled,
                    "fixture grants no root navigation"
                );
                rendered
            },
            alias,
        );
        fixture.rebuild_in_place();
    }
    for case in ["failure_retry", "history_retired", "reply_retired"] {
        let h = pages::tests::thread::root_harness().await;
        let mut old = h.state;
        let shell = std::mem::replace(
            &mut *old.write(),
            ShellState::fixture(crate::media::MediaRegistry::default()),
        );
        let (mut dom, mut state, captured) = mount(shell);
        let row = pages::rows(&state.peek())[0].clone();
        let before = (
            state.peek().core.composer.text.clone(),
            state.peek().search_query.clone(),
            pages::status(&state.peek()),
        );
        let history = pages::tests::thread::history_io();
        let replies = pages::tests::thread::replies_io();
        pages::tests::thread::provide_io(&dom, history.clone(), replies.clone());
        let history_done = history.expect(&row.channel_id, &row.ts);
        let reply = if case != "history_retired" {
            Some(replies.expect(&row.channel_id, &row.ts))
        } else {
            None
        };
        let rendered = captured.borrow().as_ref().unwrap().clone();
        let control = button(&rendered, "message-search-thread-1-0").unwrap();
        assert!(!control.disabled);
        click_control(&dom, control);
        settle(&mut dom, || history.reads.borrow().len() == 1).await;
        assert!(
            state.peek().thread_root.is_none(),
            "history completes before the thread starts"
        );
        assert_eq!(replies.reads.borrow().len(), 0);
        if case == "history_retired" {
            state.write().open_search(false);
            state
                .write()
                .set_search_query("replacement search during history".into());
            let generation = state.peek().thread_generation;
            history_done.send(()).unwrap();
            for _ in 0..20 {
                dom.render_immediate_to_vec();
                tokio::task::yield_now().await;
            }
            assert!(state.peek().thread_root.is_none());
            assert_eq!(state.peek().thread_generation, generation);
            assert!(replies.reads.borrow().is_empty());
            assert_eq!(
                state.peek().search_query,
                "replacement search during history"
            );
            continue;
        }
        history_done.send(()).unwrap();
        settle(&mut dom, || replies.reads.borrow().len() == 1).await;
        assert_eq!(state.peek().thread_root.as_deref(), Some(row.ts.as_str()));
        assert_eq!(state.peek().thread_target.as_deref(), Some(row.ts.as_str()));
        assert!(state.peek().thread_loading);
        let reply = reply.unwrap();
        if case == "reply_retired" {
            let epoch = state.peek().core.account_epoch;
            state.write().core.account_epoch = epoch.wrapping_add(1);
            state.write().core.thread_composer.text = "Replacement reply draft".into();
            let before = format!("{:?}", state.peek().core.threads);
            reply
                .send(Ok(pages::tests::thread::reply_wire(
                    &row.channel_id,
                    &row.ts,
                )))
                .unwrap();
            settle(&mut dom, || replies.completed.get() == 1).await;
            assert_eq!(format!("{:?}", state.peek().core.threads), before);
            assert_eq!(
                state.peek().core.thread_composer.text,
                "Replacement reply draft"
            );
            continue;
        }
        reply
            .send(Err(super_platinum_core::slack::Error::Offline(
                "synthetic outage".into(),
            )))
            .unwrap();
        settle(&mut dom, || !state.peek().thread_loading).await;
        assert!(state.peek().thread_error.is_some());
        assert_eq!(
            pages::rows(&state.peek())[0],
            row,
            "failed replies preserve indexed result"
        );
        assert_eq!(state.peek().core.composer.text, before.0);
        let retry = replies.expect(&row.channel_id, &row.ts);
        let channel = row.channel_id.clone();
        let root = row.ts.clone();
        dom.in_scope(ScopeId::ROOT, || {
            spawn(crate::bootstrap::open_thread_at(
                state,
                channel,
                root.clone(),
                Some(root),
            ));
        });
        settle(&mut dom, || replies.reads.borrow().len() == 2).await;
        retry
            .send(Ok(pages::tests::thread::reply_wire(
                &row.channel_id,
                &row.ts,
            )))
            .unwrap();
        settle(&mut dom, || !state.peek().thread_loading).await;
        let s = state.peek();
        assert!(s.thread_error.is_none());
        assert_eq!(s.main_view, crate::state::MainView::Home);
        assert_eq!(s.thread_root.as_deref(), Some(row.ts.as_str()));
        let key = ("T1".into(), row.channel_id.clone(), row.ts.clone());
        assert!(s.core.threads[&key].loaded);
        assert_eq!(s.core.threads[&key].messages.len(), 4);
        assert!(s.thread_messages.iter().any(|m| m.id == "120.000004"));
        assert_eq!(
            (
                s.core.composer.text.clone(),
                s.search_query.clone(),
                pages::status(&s)
            ),
            before
        );
        assert_eq!(history.reads.borrow().len(), 1);
        assert_eq!(replies.completed.get(), 2);
    }
}
