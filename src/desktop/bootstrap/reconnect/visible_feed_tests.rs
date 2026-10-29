//! Actual parent dispatch with the feature-only transport that has no HTTP client.
use super::*;
use crate::bootstrap::test_support::harness;

#[tokio::test(flavor = "current_thread")]
async fn real_reconnect_recovers_activity_beside_files_and_threads_after_a_resource_tab_without_page_one_replay()
 {
    for activity in [true, false] {
        let mut h = harness();
        let mut state = h.state;
        {
            let mut shell = state.write();
            shell.overlay = None;
            shell.main_view = if activity {
                MainView::Activity
            } else {
                MainView::Threads
            };
            if activity {
                shell.select_channel(0, crate::state::ChannelOpen::InSurface);
            }
            shell.channel_tab = ChannelTab::Files;
            let epoch = shell.core.account_epoch;
            let team = shell.core.active_team.clone().unwrap();
            if activity {
                shell.core.activity = Default::default();
                shell.core.activity.ensure_scope(epoch, &team);
                let first = shell.core.activity.begin(false).unwrap();
                shell.core.activity.apply_page(&first,serde_json::from_value(serde_json::json!({
                    "items":[{"key":"retained-visible-feed","feed_ts":"1.0","is_unread":true,"item":{"type":"at_user","message":{"channel":"C1","ts":"1.0"}}}],
                    "response_metadata":{"next_cursor":"opaque/+?=&older"}
                })).unwrap());
                let failed = shell.core.activity.begin(true).unwrap();
                shell.core.activity.fail(&failed);
                assert!(shell.conversation_visible());
            } else {
                shell.core.threads_view = Default::default();
                shell.core.threads_view.ensure_scope(epoch, &team);
                let first = shell.core.threads_view.begin(false).unwrap();
                shell.core.apply_thread_feed_page(&team,&first,serde_json::from_value(serde_json::json!({
                    "threads":[{"root_msg":{"channel":"C1","ts":"1.0","text":"Retained thread","latest_reply":"10.0","last_read":"10.0"},"latest_replies":[{"ts":"10.0","thread_ts":"1.0"}]}],
                    "has_more":true,"max_ts":"999999.0"
                })).unwrap(),&Default::default());
                let failed = shell.core.threads_view.begin(true).unwrap();
                shell.core.threads_view.fail(&failed);
                assert!(shell.detail_open());
                assert!(!shell.conversation_visible());
            }
        }
        let draft_text = state.peek().core.composer.text.clone();
        let channel = state
            .peek()
            .core
            .active_channel
            .clone()
            .expect("retained conversation from resource navigation");
        let team = state.peek().core.active_team.clone().unwrap();
        let read_cursor = state.peek().core.workspaces[&team]
            .read_cursor(&channel)
            .map(str::to_owned);
        let before = if activity {
            state.peek().core.activity.load_seq
        } else {
            state.peek().core.threads_view.load_seq
        };
        let parent_finished = std::rc::Rc::new(std::cell::Cell::new(false));
        let finished = parent_finished.clone();
        h._dom.in_runtime(|| {
            h._dom.in_scope(dioxus::core::ScopeId::ROOT, || {
                spawn(async move {
                    crate::bootstrap::reload_after_outage(state).await;
                    finished.set(true);
                })
            })
        });
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            loop {
                h._dom.render_immediate_to_vec();
                let done = {
                    let shell = state.peek();
                    if activity {
                        shell.core.activity.load_seq > before && !shell.core.activity.loading
                    } else {
                        shell.core.threads_view.load_seq > before
                            && !shell.core.threads_view.loading
                    }
                };
                if done && parent_finished.get() {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("actual reconnect must admit the exact visible failed feed");
        let shell = state.peek();
        if activity {
            assert_eq!(
                shell.core.activity.load_seq,
                before + 1,
                "one admission, no fallthrough page-one refresh"
            );
            assert_eq!(
                shell.core.activity.failed_read().unwrap().cursor.as_deref(),
                Some("opaque/+?=&older")
            );
            assert_eq!(shell.core.activity.items.len(), 1);
            assert!(shell.core.activity.failed);
            assert_eq!(shell.main_view, MainView::Activity);
        } else {
            assert_eq!(shell.core.threads_view.load_seq, before + 1);
            assert_eq!(
                shell
                    .core
                    .threads_view
                    .failed_read()
                    .unwrap()
                    .cursor
                    .as_deref(),
                Some("10.0")
            );
            assert_eq!(shell.core.threads_view.items.len(), 1);
            assert!(shell.core.threads_view.failed);
            assert_eq!(shell.main_view, MainView::Threads);
        }
        assert_eq!(shell.channel_tab, ChannelTab::Files);
        assert_eq!(shell.core.composer.text, draft_text);
        assert_eq!(shell.core.active_channel.as_deref(), Some(channel.as_str()));
        assert_eq!(
            shell.core.workspaces[&team].read_cursor(&channel),
            read_cursor.as_deref(),
            "hidden channel must not be marked read by visible-feed recovery"
        );
    }
}
