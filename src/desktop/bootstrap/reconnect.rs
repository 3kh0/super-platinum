//! Re-drive data for the current visit without turning reconnect into navigation.
use crate::state::{ChannelTab, MainView, ShellState};
use dioxus::prelude::*;
use std::sync::Arc;
use super_platinum_core::slack::Transport;

#[cfg(test)]
mod admitted_tests;
pub(crate) mod check;
pub(crate) mod fixture;
pub(super) mod history_gate;

#[derive(Clone)]
pub(crate) struct Visit {
    account: Option<String>,
    epoch: u64,
    team: Option<String>,
    transport: Option<Arc<Transport>>,
    view: MainView,
    channel: Option<String>,
    channel_generation: u64,
    root: Option<String>,
    thread_generation: u64,
    shortcut: u64,
    request: u64,
    overlay: Option<crate::state::Overlay>,
    tab: ChannelTab,
}
impl Visit {
    pub(super) fn begin(s: &mut ShellState) -> Self {
        s.reconnect_generation = s.reconnect_generation.wrapping_add(1);
        // Retire the old serial's busy flags synchronously. Cached content,
        // paging bounds, error, target and the thread visit remain untouched.
        s.thread_loading = false;
        s.thread_history.loading = None;
        Self::capture(s)
    }
    fn capture(s: &ShellState) -> Self {
        Self {
            account: s.core.active_account.clone(),
            epoch: s.core.account_epoch,
            team: s.core.active_team.clone(),
            transport: s.core.transport.clone(),
            view: s.main_view,
            channel: s.core.active_channel.clone(),
            channel_generation: s.channel_generation,
            root: s.thread_root.clone(),
            thread_generation: s.thread_generation,
            shortcut: s.shortcut_generation,
            request: s.reconnect_generation,
            overlay: s.overlay.clone(),
            tab: s.channel_tab,
        }
    }
    /// An admitted REST request owns its own serial. A later restore does not
    /// retire it, while every original account/surface/overlay field still does.
    /// Queued admission must continue to use `current`, including its serial.
    pub(crate) fn admitted_current(&self, s: &ShellState) -> bool {
        let mut admitted = self.clone();
        admitted.request = s.reconnect_generation;
        admitted.current(s)
    }
    pub(crate) fn current(&self, s: &ShellState) -> bool {
        self.account == s.core.active_account
            && self.epoch == s.core.account_epoch
            && self.team == s.core.active_team
            && self.view == s.main_view
            && self.channel == s.core.active_channel
            && self.channel_generation == s.channel_generation
            && self.root == s.thread_root
            && self.thread_generation == s.thread_generation
            && self.request == s.reconnect_generation
            && self.shortcut == s.shortcut_generation
            && self.overlay == s.overlay
            && self.tab == s.channel_tab
            && match (&self.transport, &s.core.transport) {
                (None, None) => true,
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                _ => false,
            }
    }
}

#[cfg(test)]
pub(crate) fn resource_test_visit(shell: &mut ShellState) -> Visit {
    Visit::begin(shell)
}

pub async fn reload_after_outage(mut state: Signal<ShellState>) {
    let visit = {
        let mut s = state.write();
        Visit::begin(&mut s)
    };
    super::search::recovery::schedule(state, visit.clone());
    super::search::message_pages::schedule_after_outage(state, visit.clone());
    super::search::file_pages::schedule_after_outage(state, visit.clone());
    super::search_people::numbered::schedule_after_outage(state, visit.clone());
    super::profile_search::schedule_after_outage(state, visit.clone());
    super::discovery::profile_recovery::schedule_after_outage(state, visit.clone());
    let run = fixture::begin(&state.peek());
    crate::slack_lists::invalidate_visible_for_reconnect(&mut state.write());
    crate::slack_lists::refresh(state);
    let resource_detail = {
        let shell = state.peek();
        shell.conversation_visible() && visit.tab != ChannelTab::Messages
    };
    if resource_detail {
        if visit.view == MainView::Activity {
            crate::activity_feed::schedule_failed_after_outage(state, visit.clone());
        }
        crate::channel_resources::refresh_after_outage(state, visit.clone()).await;
        refresh_visible_thread(state, visit).await;
        return;
    }
    super::discovery::refresh_main_view(state, visit.view, Some(&visit)).await;
    let phase = fixture::boundary(state, run, "surface").await;
    let current = visit.current(&state.peek());
    fixture::finish_boundary(phase, &state.peek(), current);
    if !current {
        return;
    }
    super::history::refresh_selected_channel_owned(state, &visit).await;
    let phase = fixture::boundary(state, run, "history").await;
    let current = visit.current(&state.peek());
    fixture::finish_boundary(phase, &state.peek(), current);
    if !current {
        return;
    }
    refresh_visible_thread(state, visit).await;
}

async fn refresh_visible_thread(state: Signal<ShellState>, visit: Visit) {
    // Refresh the current pane without reopening it or restoring a consumed target.
    let (channel, root, target, scope) = {
        let s = state.peek();
        if !visit.current(&s) || !s.thread_visible() {
            return;
        }
        let Some((channel, root)) = s.core.active_channel.clone().zip(s.thread_root.clone()) else {
            return;
        };
        (channel, root, s.thread_target.clone(), s.thread_scope())
    };
    super::thread::load_started_thread_owned(state, channel, root, target, scope, Some(visit))
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn queued_continuation_after_navigation_preserves_each_thread_draft_and_new_visit() {
        use std::{
            cell::{Cell, RefCell},
            rc::Rc,
        };
        type Props = (
            Rc<RefCell<Option<ShellState>>>,
            Rc<Cell<Option<Signal<ShellState>>>>,
        );
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        shell.sync_composer_drafts();
        let channel_a = shell.active_channel;
        let channel_b = if channel_a == 0 { 1 } else { 0 };
        shell.begin_thread("100.000001".into(), Some("100.000002".into()));
        shell.core.thread_composer =
            super_platinum_core::ComposerState::with_text("Thread A draft \u{4e2d}\u{1f642}");
        let owner = Visit::begin(&mut shell);
        let scope = shell.thread_scope();
        let channel = shell.core.active_channel.clone().unwrap();
        let slot = Rc::new(Cell::new(None));
        let mut dom = VirtualDom::new_with_props(
            |(input, slot): Props| {
                let state = use_signal(|| input.borrow_mut().take().unwrap());
                slot.set(Some(state));
                VNode::empty()
            },
            (Rc::new(RefCell::new(Some(shell))), slot.clone()),
        );
        dom.rebuild_to_vec();
        let mut state = slot.get().unwrap();
        let (release, held) = tokio::sync::oneshot::channel();
        let retired_owner = owner.clone();
        let retired_scope = scope.clone();
        let retired_channel = channel.clone();
        let queued = async move {
            held.await.unwrap();
            super::super::thread::load_started_thread_owned(
                state,
                retired_channel,
                "100.000001".into(),
                Some("100.000002".into()),
                retired_scope,
                Some(retired_owner),
            )
            .await;
        };
        {
            let mut shell = state.write();
            shell.select_channel(channel_b, crate::state::ChannelOpen::Global);
            shell.begin_thread("200.000001".into(), Some("200.000002".into()));
            shell.core.thread_composer =
                super_platinum_core::ComposerState::with_text("Thread B draft");
            shell.thread_error = Some("Current B read is pending".into());
            shell.thread_window.input_generation += 1;
        }
        let before_scope = state.peek().thread_scope();
        let before_input = state.peek().thread_window.input_generation;
        assert!(!owner.current(&state.peek()));
        release.send(()).unwrap();
        queued.await;
        {
            let shell = state.peek();
            assert_eq!(shell.active_channel, channel_b);
            assert_eq!(shell.thread_scope(), before_scope);
            assert_eq!(shell.thread_root.as_deref(), Some("200.000001"));
            assert_eq!(shell.thread_target.as_deref(), Some("200.000002"));
            assert_eq!(shell.core.thread_composer.text, "Thread B draft");
            assert_eq!(
                shell.thread_error.as_deref(),
                Some("Current B read is pending")
            );
            assert!(shell.thread_loading, "A must not clear B's busy state");
            assert_eq!(shell.thread_window.input_generation, before_input);
        }
        {
            let mut shell = state.write();
            shell.select_channel(channel_a, crate::state::ChannelOpen::Global);
            shell.begin_thread("100.000001".into(), Some("100.000003".into()));
            shell.thread_error = Some("Current A read is pending".into());
            shell.thread_window.input_generation += 1;
        }
        let before_scope = state.peek().thread_scope();
        let before_input = state.peek().thread_window.input_generation;
        let before_error = state.peek().thread_error.clone();
        let before_loading = state.peek().thread_loading;
        assert!(!owner.current(&state.peek()));
        super::super::thread::load_started_thread_owned(
            state,
            channel,
            "100.000001".into(),
            Some("100.000002".into()),
            scope,
            Some(owner),
        )
        .await;
        let shell = state.peek();
        assert_eq!(shell.thread_scope(), before_scope);
        assert_eq!(shell.thread_window.input_generation, before_input);
        assert_eq!(shell.thread_error, before_error);
        assert_eq!(shell.thread_loading, before_loading);
        assert_eq!(shell.thread_target.as_deref(), Some("100.000003"));
        assert_eq!(
            shell.core.thread_composer.text,
            "Thread A draft \u{4e2d}\u{1f642}"
        );
        assert!(shell.thread_loading);
        assert!(shell.core.session.is_none() && shell.core.transport.is_none());
    }

    #[test]
    fn retiring_old_read_releases_busy_state_without_resetting_thread() {
        use super_platinum_core::thread_history::{Direction, Page};
        let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
        s.begin_thread("1.0".into(), Some("1.1".into()));
        s.thread_error = Some("cached failure".into());
        s.thread_history.earlier = Some(Page {
            latest: Some("1.1".into()),
            ..Default::default()
        });
        s.thread_history.newer = Some(Page {
            oldest: Some("1.1".into()),
            ..Default::default()
        });
        s.thread_history.loading = Some(Direction::Earlier);
        s.thread_history.failed = Some(Direction::Newer);
        let before = Visit::capture(&s);
        let earlier = s.thread_history.earlier.clone();
        let newer = s.thread_history.newer.clone();
        let key = ("T1".to_owned(), "C2".to_owned(), "1.0".to_owned());
        s.core.threads.insert(
            key.clone(),
            super_platinum_core::state::ChannelMessages {
                messages: vec![super_platinum_core::slack::models::Message {
                    ts: Some("1.1".into()),
                    text: Some("cached body".into()),
                    ..Default::default()
                }],
                ..Default::default()
            },
        );
        let input = s.thread_window.input_generation;
        let current = Visit::begin(&mut s);
        assert!(!before.current(&s));
        assert!(current.current(&s));
        assert!(!s.thread_loading);
        assert!(s.thread_history.loading.is_none());
        assert_eq!(s.thread_history.earlier, earlier);
        assert_eq!(s.thread_history.newer, newer);
        assert_eq!(s.thread_history.failed, Some(Direction::Newer));
        assert_eq!(s.thread_root.as_deref(), Some("1.0"));
        assert_eq!(s.thread_target.as_deref(), Some("1.1"));
        assert_eq!(s.thread_error.as_deref(), Some("cached failure"));
        assert_eq!(s.thread_generation, before.thread_generation);
        assert_eq!(
            s.core.threads[&key].messages[0].text.as_deref(),
            Some("cached body")
        );
        assert_eq!(s.thread_window.input_generation, input);
    }
    #[test]
    fn reconnect_preserves_consumed_target_but_rejects_replaced_owners() {
        let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
        s.begin_thread("1.0".into(), Some("1.1".into()));
        let v = Visit::capture(&s);
        s.thread_target = None;
        assert!(v.current(&s));
        s.reconnect_generation += 1;
        assert!(!v.current(&s));
        s.reconnect_generation -= 1;
        s.thread_generation += 1;
        assert!(!v.current(&s));
        s.thread_generation -= 1;
        s.channel_generation += 1;
        assert!(!v.current(&s));
        s.channel_generation -= 1;
        s.shortcut_generation += 1;
        assert!(!v.current(&s));
        s.shortcut_generation -= 1;
        s.core.account_epoch += 1;
        assert!(!v.current(&s));
        s.core.account_epoch -= 1;
        s.channel_tab = ChannelTab::Files;
        assert!(!v.current(&s));
        s.channel_tab = v.tab;
        s.overlay = Some(crate::state::Overlay::CanvasCreate);
        assert!(!v.current(&s));
        s.overlay = v.overlay.clone();
        s.main_view = MainView::Later;
        assert!(!v.current(&s));
        s.main_view = v.view;
        assert!(v.current(&s));
        s.close_thread();
        assert!(!v.current(&s));
    }
}

#[cfg(all(test, feature = "protected-native-fixture"))]
mod visible_feed_tests;

#[cfg(test)]
mod side_thread_tests;
