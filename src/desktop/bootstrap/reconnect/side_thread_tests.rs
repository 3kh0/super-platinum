//! Real parent recovery with a closed transport; no native context or HTTP client.
use super::*;
use crate::bootstrap::test_support::harness;

#[tokio::test(flavor = "current_thread")]
async fn resource_reconnect_retries_visible_retained_thread_without_reopening_or_reading_it() {
    for open in [true, false] {
        let mut h = harness();
        let mut state = h.state;
        let (team, channel, root) = {
            let mut s = state.write();
            s.overlay = None;
            s.main_view = MainView::Home;
            s.channel_tab = ChannelTab::Files;
            let team = s.core.active_team.clone().unwrap();
            let channel = s.core.active_channel.clone().unwrap();
            let root = "1750000900.000001".to_owned();
            if open {
                s.begin_thread(root.clone(), None);
            }
            let bag = s
                .core
                .threads
                .entry((team.clone(), channel.clone(), root.clone()))
                .or_default();
            for raw in [
                serde_json::json!({"ts":root,"thread_ts":root,"text":"Retained root"}),
                serde_json::json!({"ts":"1750000900.000002","thread_ts":root,"text":"Retained reply"}),
            ] {
                bag.upsert(serde_json::from_value(raw).unwrap());
            }
            bag.loaded = true;
            s.thread_error = Some("previous failed replies sentinel".into());
            s.core.composer.text = "Keep channel draft".into();
            s.core.thread_composer.text = "Keep thread draft".into();
            assert_eq!(s.thread_visible(), open);
            (team, channel, root)
        };
        let key = (team.clone(), channel.clone(), root);
        let (scope, target, cached, revision, channel_cursor, drafts, generations) = {
            let s = state.peek();
            (
                s.thread_scope(),
                s.thread_target.clone(),
                format!("{:?}", s.core.threads.get(&key)),
                s.core.thread_read_revision(&key),
                s.core.workspaces[&team]
                    .read_cursor(&channel)
                    .map(str::to_owned),
                (
                    s.core.composer.text.clone(),
                    s.core.thread_composer.text.clone(),
                ),
                (s.channel_generation, s.thread_generation),
            )
        };
        let finished = std::rc::Rc::new(std::cell::Cell::new(false));
        let done = finished.clone();
        h._dom.in_runtime(|| {
            h._dom.in_scope(dioxus::core::ScopeId::ROOT, || {
                spawn(async move {
                    reload_after_outage(state).await;
                    done.set(true);
                })
            })
        });
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            while !finished.get() {
                h._dom.render_immediate_to_vec();
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("real resource reconnect must finish");
        let s = state.peek();
        assert_eq!(
            s.thread_error.as_deref(),
            Some(if open {
                "Replies could not load. Please try again."
            } else {
                "previous failed replies sentinel"
            })
        );
        assert!(!s.thread_loading);
        assert_eq!(s.thread_scope(), scope);
        assert_eq!(s.thread_target, target);
        assert_eq!(s.core.active_channel.as_deref(), Some(channel.as_str()));
        assert_eq!(
            s.thread_root.as_deref(),
            if open { Some(key.2.as_str()) } else { None }
        );
        assert_eq!(format!("{:?}", s.core.threads.get(&key)), cached);
        assert_eq!(s.core.thread_read_revision(&key), revision);
        assert_eq!(
            s.core.workspaces[&team].read_cursor(&channel),
            channel_cursor.as_deref()
        );
        assert_eq!(
            (
                s.core.composer.text.clone(),
                s.core.thread_composer.text.clone()
            ),
            drafts
        );
        assert_eq!((s.channel_generation, s.thread_generation), generations);
        assert_eq!(s.main_view, MainView::Home);
        assert_eq!(s.channel_tab, ChannelTab::Files);
    }
}
