use super::*;
use crate::bootstrap::test_support::{Harness, harness, users};
use std::{cell::Cell, future::Future, rc::Rc};

const SENTINEL: &str = "previous profile failure sentinel";
const FAILED: &str = "Try opening this profile again.";

fn prepare(mut state: Signal<ShellState>) -> String {
    let mut shell = state.write();
    assert!(
        shell.core.cache.is_none(),
        "closed fixture must not persist a real workspace"
    );
    shell.overlay = None;
    shell.main_view = crate::state::MainView::Home;
    shell.channel_tab = crate::state::ChannelTab::Files;
    let workspace = &shell.core.workspaces["T1"];
    assert!(!matches!(
        &workspace.rt,
        super_platinum_core::state::RealtimeStatus::Connected(_)
    ));
    let user = workspace
        .users
        .keys()
        .find(|id| id.as_str() != workspace.self_user_id.as_str())
        .or_else(|| workspace.users.keys().next())
        .unwrap()
        .clone();
    super::super::show_profile(&mut shell, user.clone());
    let pane = shell.core.profile_pane.as_mut().unwrap();
    pane.loading = false;
    pane.error = Some(SENTINEL.into());
    user
}

async fn drive<F: Future<Output = ()> + 'static>(
    h: &mut Harness,
    work: F,
    ready: impl Fn(&ShellState) -> bool,
) {
    let finished = Rc::new(Cell::new(false));
    let done = finished.clone();
    h._dom.in_runtime(|| {
        h._dom.in_scope(dioxus::core::ScopeId::ROOT, || {
            spawn(async move {
                work.await;
                done.set(true);
            })
        })
    });
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            h._dom.render_immediate_to_vec();
            if finished.get() && ready(&h.state.peek()) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("actual profile recovery task must settle");
}

#[tokio::test(flavor = "current_thread")]
async fn real_reconnect_retries_known_failed_profile_beside_files_preserving_cached_details_and_visit()
 {
    let mut h = harness();
    let state = h.state;
    let user = prepare(state);
    let cached = users(state);
    let (generation, channel, drafts) = {
        let s = state.peek();
        (
            s.profile_generation,
            s.core.active_channel.clone(),
            (
                s.core.composer.text.clone(),
                s.core.thread_composer.text.clone(),
            ),
        )
    };
    drive(&mut h, crate::bootstrap::reload_after_outage(state), |s| {
        s.core
            .profile_pane
            .as_ref()
            .is_some_and(|p| !p.loading && p.error.as_deref() == Some(FAILED))
    })
    .await;
    let s = state.peek();
    assert_eq!(s.profile_user.as_deref(), Some(user.as_str()));
    let pane = s.core.profile_pane.as_ref().unwrap();
    assert_eq!(pane.user, user);
    assert!(!pane.loading);
    assert_eq!(pane.error.as_deref(), Some(FAILED));
    assert_eq!(s.profile_generation, generation);
    assert_eq!(s.core.active_channel, channel);
    assert_eq!(
        (
            s.core.composer.text.clone(),
            s.core.thread_composer.text.clone()
        ),
        drafts
    );
    assert_eq!(s.main_view, crate::state::MainView::Home);
    assert_eq!(s.channel_tab, crate::state::ChannelTab::Files);
    assert!(s.profile_search.is_none());
    drop(s);
    assert_eq!(users(state), cached);
}

#[tokio::test(flavor = "current_thread")]
async fn queued_known_profile_reads_bind_original_pane_and_failed_idle_reconnect_admission() {
    for change in 0..6 {
        let mut h = harness();
        let mut state = h.state;
        let user = prepare(state);
        let cached = users(state);
        if change >= 4 {
            // Exercise the production synchronous constructor before queueing.
            if change == 5 {
                let mut s = state.write();
                let pane = s.core.profile_pane.as_mut().unwrap();
                pane.loading = true;
                pane.error = None;
            }
            let old = super::super::refresh_open_profile(state, user.clone());
            {
                let mut s = state.write();
                if change == 4 {
                    super::super::show_profile(&mut s, user.clone());
                    s.core.profile_pane.as_mut().unwrap().error =
                        Some("replacement pane sentinel".into());
                } else {
                    s.core
                        .session
                        .as_mut()
                        .unwrap()
                        .workspaces
                        .get_mut("T1")
                        .unwrap()
                        .token
                        .push_str("-rotated");
                    s.core.transport = Some(crate::bootstrap::test_support::transport("synthetic"));
                }
            }
            let generation = state.peek().profile_generation;
            drive(&mut h, old, |_| true).await;
            let s = state.peek();
            assert_eq!(s.profile_generation, generation);
            assert_eq!(s.core.profile_pane.as_ref().unwrap().loading, change == 4);
            assert_eq!(
                s.core.profile_pane.as_ref().unwrap().error.as_deref(),
                Some(if change == 4 {
                    "replacement pane sentinel"
                } else {
                    "Open this profile again to refresh its details."
                })
            );
        } else {
            let reconnect = crate::bootstrap::reconnect::resource_test_visit(&mut state.write());
            let source = Source::capture(&state.peek(), reconnect).unwrap();
            match change {
                1 => {
                    let mut s = state.write();
                    super::super::show_profile(&mut s, user.clone());
                    s.core.profile_pane.as_mut().unwrap().error = Some(SENTINEL.into());
                }
                2 => state.write().core.profile_pane.as_mut().unwrap().loading = true,
                3 => {
                    crate::bootstrap::reconnect::resource_test_visit(&mut state.write());
                }
                _ => {}
            }
            let generation = state.peek().profile_generation;
            drive(&mut h, run(state, source), |_| true).await;
            let s = state.peek();
            let pane = s.core.profile_pane.as_ref().unwrap();
            assert_eq!(
                pane.error.as_deref(),
                Some(if change == 0 { FAILED } else { SENTINEL }),
                "admission case {change}"
            );
            assert_eq!(pane.loading, matches!(change, 1 | 2));
            assert_eq!(s.profile_generation, generation);
        }
        assert_eq!(state.peek().profile_user.as_deref(), Some(user.as_str()));
        assert_eq!(users(state), cached);
    }
}
