//! Reactive regression tests for the actual paint-acknowledgement commit.
//! No Desktop/WebView, session, transport, environment mutation or wall sleeps.
use super::*;
use dioxus::prelude::*;
use std::{cell::Cell, rc::Rc, time::Duration};

type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
type Renders = Rc<Cell<usize>>;

struct Harness {
    dom: VirtualDom,
    state: Signal<ShellState>,
    renders: Renders,
}

impl Harness {
    fn new() -> Self {
        let slot = Slot::default();
        let renders = Renders::default();
        let mut dom = VirtualDom::new_with_props(
            |(slot, renders): (Slot, Renders)| {
                let state =
                    use_signal(|| ShellState::opening(crate::media::MediaRegistry::default()));
                slot.set(Some(state));
                renders.set(renders.get() + 1);
                // A real UI field subscribes this scope to the full shell signal.
                let loading = state.read().loading;
                rsx! { div { "{loading}" } }
            },
            (slot.clone(), renders.clone()),
        );
        dom.rebuild_in_place();
        let state = slot.get().unwrap();
        assert!(state.peek().core.session.is_none());
        assert!(state.peek().core.transport.is_none());
        assert!(state.peek().core.cache.is_none());
        Self {
            dom,
            state,
            renders,
        }
    }

    fn flush(&mut self) {
        self.dom.render_immediate_to_vec();
    }

    fn acknowledge(&mut self, now: Instant) {
        self.dom
            .in_runtime(|| record_paint_latency(self.state, now));
        self.flush();
    }
}

#[test]
fn empty_paint_acknowledgements_do_not_render_but_normal_writes_still_do() {
    let mut h = Harness::new();
    let initial = h.renders.get();
    for _ in 0..3 {
        h.acknowledge(Instant::now());
    }
    assert_eq!(h.renders.get(), initial);
    // A positive control: the former unconditional write schedules the scope
    // even without a changed field. This verifies the subscriber is observable.
    h.dom.in_runtime(|| drop(h.state.write()));
    h.flush();
    assert_eq!(h.renders.get(), initial + 1);
    // Ordinary visible writes retain their normal notification semantics.
    h.dom.in_runtime(|| h.state.write().loading = false);
    h.flush();
    assert_eq!(h.renders.get(), initial + 2);
    assert!(!h.state.peek().loading);
}

#[test]
fn real_paint_samples_keep_exact_elapsed_values_and_notify_once() {
    let mut h = Harness::new();
    let now = Instant::now();
    h.dom.in_runtime(|| {
        let mut shell = h.state.write();
        shell.channel_switch_started = Some(now - Duration::from_millis(100));
        shell.realtime_insert_started = Some(now - Duration::from_millis(25));
    });
    h.flush();
    let initial = h.renders.get();
    h.acknowledge(now);
    assert_eq!(h.renders.get(), initial + 1);
    {
        let shell = h.state.peek();
        assert_eq!(shell.performance.channel_switch_ms, Some(100.0));
        assert_eq!(shell.performance.realtime_insert_ms, Some(25.0));
        assert!(shell.channel_switch_started.is_none());
        assert!(shell.realtime_insert_started.is_none());
    }
    h.acknowledge(now + Duration::from_secs(1));
    assert_eq!(h.renders.get(), initial + 1);
    assert_eq!(h.state.peek().performance.channel_switch_ms, Some(100.0));
    assert_eq!(h.state.peek().performance.realtime_insert_ms, Some(25.0));
}

#[test]
fn a_single_pending_metric_preserves_the_other_recorded_metric() {
    for channel in [false, true] {
        let mut h = Harness::new();
        let now = Instant::now();
        h.dom.in_runtime(|| {
            let mut shell = h.state.write();
            shell.performance.channel_switch_ms = Some(8.0);
            shell.performance.realtime_insert_ms = Some(9.0);
            if channel {
                shell.channel_switch_started = Some(now - Duration::from_millis(40));
            } else {
                shell.realtime_insert_started = Some(now - Duration::from_millis(40));
            }
        });
        h.flush();
        h.acknowledge(now);
        let shell = h.state.peek();
        assert_eq!(
            shell.performance.channel_switch_ms,
            Some(if channel { 40.0 } else { 8.0 })
        );
        assert_eq!(
            shell.performance.realtime_insert_ms,
            Some(if channel { 9.0 } else { 40.0 })
        );
        assert!(shell.channel_switch_started.is_none() && shell.realtime_insert_started.is_none());
    }
}
