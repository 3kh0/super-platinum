use super::*;
use crate::bootstrap::reconnect::resource_test_visit;
use crate::channel_tabs::selection;
fn ts(i: usize) -> String {
    format!("{:010}.000001", 1_710_000_000 + i)
}
fn page(text: &str) -> Value {
    let rows = (1..=150)
        .map(
            |i| json!({"ts":ts(i),"user":"UINIT0001","text":format!("Owned recovery context {i}")}),
        )
        .collect::<Vec<_>>();
    json!({"ok":true,"threads":[{"root_msg":{"channel":C,"ts":ROOT,"thread_ts":ROOT,"user":"UINIT0001","subscribed":true,"reply_count":150,"last_read":ts(150),"text":text},"latest_replies":rows}],"has_more":false})
}
fn recover(u: &ui::Ui) -> (Rc<Cell<bool>>, Rc<Cell<bool>>) {
    let mut state = u.state;
    let visit = {
        let mut s = state.write();
        resource_test_visit(&mut s)
    };
    let started = Rc::new(Cell::new(false));
    let done = Rc::new(Cell::new(false));
    let began = started.clone();
    let ended = done.clone();
    u.dom.in_scope(dioxus::core::ScopeId::ROOT, || {
        dioxus::core::spawn_forever(async move {
            began.set(true);
            crate::channel_resources::refresh_after_outage(state, visit).await;
            ended.set(true);
        })
    });
    (started, done)
}
async fn seeded() -> ui::Ui {
    let mut u = ui::mount(setup(true).await);
    let first = u.io.expect();
    u.click("data-channel-tab-id", "threads");
    let state = u.state;
    u.pump(|| u_state_loading(state)).await;
    first.send(Ok(page("Initial recovery cache"))).unwrap();
    let state = u.state;
    u.pump(|| visible(&state.peek()).is_some_and(|v| v.loaded() && !v.loading()))
        .await;
    u
}
fn u_state_loading(s: Signal<ShellState>) -> bool {
    visible(&s.peek()).is_some_and(|v| v.loading())
}
async fn reinitialize(u: &mut ui::Ui, on: bool) {
    let mut state = u.state;
    support::provide(&u.dom, Rc::new(support::Renderer::default()));
    support::connect(&mut state.write()).unwrap();
    let initial = support::assignments(&mut state.write(), &flags(on)).unwrap();
    let raw = boot(state);
    support::run_pumped(
        &mut u.dom,
        initial.complete_channels(state, raw, channels()),
    )
    .await
    .unwrap();
}
fn context_len(state: Signal<ShellState>) -> usize {
    loaded(&state.peek())[0].replies.len()
}
fn root_text(state: Signal<ShellState>) -> String {
    loaded(&state.peek())[0].root.text.clone().unwrap()
}
#[tokio::test]
async fn generated_outage_reconfirms_retired_gate_keeps_expansion_history_and_thread_drafts() {
    let mut u = seeded().await;
    let mut state = u.state;
    let io = u.io.clone();
    u.click("data-previous-thread", ROOT);
    u.pump(|| context_len(state) == 102).await;
    let old_action = super::super::previous::capture(&state.peek(), ROOT).unwrap();
    {
        let mut s = state.write();
        s.begin_thread(ROOT.into(), Some(ts(1)));
        s.core.composer = super_platinum_core::ComposerState::with_text("Channel draft stays");
        s.core.thread_composer =
            super_platinum_core::ComposerState::with_text("Thread draft stays");
    }
    let before = (
        state.peek().channel_generation,
        state.peek().shortcut_generation,
        state.peek().thread_generation,
        state.peek().thread_target.clone(),
    );
    let old_source = Source::capture(&state.peek()).unwrap();
    let back = selection::step(&state.peek(), false).unwrap();
    assert!(selection::step(&state.peek(), true).is_none());
    reinitialize(&mut u, true).await;
    assert!(!old_source.route.current(&state.peek()));
    assert!(Source::capture(&state.peek()).is_none());
    let held = io.expect();
    let (_, done) = recover(&u);
    u.pump(|| io.calls.get() == 2).await;
    assert_eq!(context_len(state), 102);
    assert_eq!(u.message_ids().len(), 103);
    assert_eq!(
        (
            state.peek().channel_generation,
            state.peek().shortcut_generation,
            state.peek().thread_generation,
            state.peek().thread_target.clone()
        ),
        before
    );
    assert!(!selection::accepts(&state.peek(), &back));
    assert_eq!(
        selection::step(&state.peek(), false).unwrap().route,
        back.route
    );
    assert!(selection::step(&state.peek(), true).is_none());
    old_source.start(state);
    old_action.reveal(state);
    assert_eq!(io.calls.get(), 2);
    assert_eq!(context_len(state), 102);
    held.send(Err(Error::Offline("original synthetic outage".into())))
        .unwrap();
    u.pump(|| {
        done.get() && visible(&state.peek()).is_some_and(|v| !v.loading() && !v.notice.is_empty())
    })
    .await;
    assert_eq!(context_len(state), 102);
    assert_eq!(root_text(state), "Initial recovery cache");
    assert_eq!(state.peek().core.composer.text, "Channel draft stays");
    assert_eq!(state.peek().core.thread_composer.text, "Thread draft stays");
    assert_eq!(state.peek().thread_root.as_deref(), Some(ROOT));
    let success = io.expect();
    let (_, done) = recover(&u);
    u.pump(|| io.calls.get() == 3).await;
    assert_eq!(context_len(state), 102);
    success.send(Ok(page("Recovered current data"))).unwrap();
    u.pump(|| done.get() && root_text(state) == "Recovered current data")
        .await;
    assert_eq!(context_len(state), 2);
    assert_eq!(io.calls.get(), 3);
    assert!(io.opens.borrow().is_empty());
    assert_eq!(
        (
            state.peek().channel_generation,
            state.peek().shortcut_generation,
            state.peek().thread_generation,
            state.peek().thread_target.clone()
        ),
        before
    );
}
#[tokio::test]
async fn generated_restore_serial_supersedes_queued_read_but_waits_for_admitted_read_without_duplicate()
 {
    for admitted in [false, true] {
        let mut u = seeded().await;
        let state = u.state;
        let io = u.io.clone();
        let gate = if admitted {
            None
        } else {
            Some(io.hold_dispatch())
        };
        let a_response = io.expect();
        let (_, a_done) = recover(&u);
        u.pump(|| {
            if admitted {
                io.calls.get() == 2
            } else {
                io.gated.get() == 1
            }
        })
        .await;
        let b_response = if admitted { None } else { Some(io.expect()) };
        let (b_started, b_done) = recover(&u);
        u.pump(|| b_started.get() && (admitted || io.calls.get() == 2))
            .await;
        if admitted {
            assert!(
                !b_done.get(),
                "coalesced restore must wait before its outer thread phase"
            );
            assert_eq!(io.calls.get(), 2);
            a_response
                .send(Ok(page("Admitted original survives later restore")))
                .unwrap();
            u.pump(|| a_done.get() && b_done.get() && !u_state_loading(state))
                .await;
            assert_eq!(root_text(state), "Admitted original survives later restore");
            assert_eq!(io.calls.get(), 2);
        } else {
            gate.unwrap().send(()).unwrap();
            u.pump(|| a_done.get()).await;
            assert!(
                a_response
                    .send(Ok(page("Never sent queued predecessor")))
                    .is_err()
            );
            assert!(
                u_state_loading(state),
                "obsolete cleanup cannot clear replacement pending state"
            );
            b_response
                .unwrap()
                .send(Ok(page("Queued replacement owns result")))
                .unwrap();
            u.pump(|| b_done.get() && !u_state_loading(state)).await;
            assert_eq!(root_text(state), "Queued replacement owns result");
            assert_eq!(io.calls.get(), 2);
        }
    }
}
#[tokio::test]
async fn generated_rebased_restore_keeps_retired_429_budget_and_off_assignment_never_recovers() {
    let mut u = seeded().await;
    let state = u.state;
    let io = u.io.clone();
    let retired = io.expect();
    let (_, a_done) = recover(&u);
    u.pump(|| io.calls.get() == 2).await;
    reinitialize(&mut u, true).await;
    let gate = io.hold_dispatch();
    let never = io.expect();
    let (_, b_done) = recover(&u);
    u.pump(|| io.gated.get() == 1).await;
    retired
        .send(Err(Error::RateLimited {
            retry_after_secs: Some(1),
        }))
        .unwrap();
    u.pump(|| a_done.get() && limit_status(&state.peek()).cooling())
        .await;
    assert!(u_state_loading(state));
    gate.send(()).unwrap();
    u.pump(|| b_done.get() && !u_state_loading(state)).await;
    assert!(never.send(Ok(page("Forbidden peer dispatch"))).is_err());
    assert_eq!(io.calls.get(), 2);
    assert_eq!(root_text(state), "Initial recovery cache");
    u.pump(|| !limit_status(&state.peek()).cooling()).await;
    assert_eq!(io.calls.get(), 2);
    let success = io.expect();
    let (_, done) = recover(&u);
    u.pump(|| io.calls.get() == 3).await;
    success.send(Ok(page("Explicit later restore"))).unwrap();
    u.pump(|| done.get() && !u_state_loading(state)).await;
    assert_eq!(root_text(state), "Explicit later restore");
    reinitialize(&mut u, false).await;
    let (_, done) = recover(&u);
    u.pump(|| done.get()).await;
    assert_eq!(io.calls.get(), 3);
    assert!(Source::capture(&state.peek()).is_none());
}
