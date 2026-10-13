//! Preserve the difference between a notification about one reply and a bundle
//! about a whole thread when entering the shared thread-loading path.
use super_platinum_core::slack::models::ActivityItem;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ThreadDestination {
    pub root: String,
    pub target: Option<String>,
}

pub(crate) fn thread_destination(item: &ActivityItem) -> Option<ThreadDestination> {
    let root = item.thread_ts()?;
    // Bundled thread entries expose their root as ts(), even if latest_ts and
    // min_unread_ts are present. Preserve their existing latest-thread behavior.
    // A mention/reaction referencing an actual reply must load that exact reply;
    // target=None would jump to latest and eagerly mark newer replies read.
    let target = item.ts().filter(|ts| *ts != root).map(str::to_owned);
    Some(ThreadDestination {
        root: root.into(),
        target,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{MainView, ShellState};
    use serde_json::json;
    use super_platinum_core::thread_history::ThreadHistory;

    fn item(kind: &str, ts: &str, root: Option<&str>) -> ActivityItem {
        serde_json::from_value(json!({
            "key":"notification", "is_unread":true,
            "item":{"type":kind,"message":{"channel":"C2","ts":ts,"thread_ts":root}}
        }))
        .unwrap()
    }

    #[test]
    fn older_reply_notifications_keep_exact_target_in_activity_and_initial_request() {
        for kind in ["mention", "reaction", "thread_reply"] {
            let notification = item(kind, "150.000002", Some("100.000001"));
            let destination = thread_destination(&notification).unwrap();
            let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
            shell.main_view = MainView::Activity;
            // The click path selects the notification's conversation first,
            // then passes this same destination to open_thread_at/begin_thread.
            assert!(shell.select_activity_item(
                notification.key.clone(),
                notification.channel(),
                notification.ts(),
                Some(&destination.root),
            ));
            shell.begin_thread(destination.root.clone(), destination.target.clone());
            assert_eq!(shell.main_view, MainView::Activity);
            assert!(shell.detail_open());
            assert_eq!(shell.thread_root.as_deref(), Some("100.000001"));
            assert_eq!(shell.thread_target.as_deref(), Some("150.000002"));
            assert!(!shell.thread_at_bottom);
            // A target much older than the latest page is requested inclusively,
            // not just sought in the newest 200 replies after they arrive.
            let request = ThreadHistory::initial(destination.target.as_deref(), 2000)
                .request("C2".into(), destination.root);
            assert_eq!(request.oldest.as_deref(), Some("150.000002"));
            assert!(request.latest.is_none());
            assert!(request.inclusive);
            assert_eq!(request.limit, Some(200));
        }
    }

    #[test]
    fn bundled_thread_keeps_latest_behavior_despite_latest_and_unread_timestamps() {
        let notification: ActivityItem = serde_json::from_value(json!({
            "key":"bundle", "item":{"type":"thread_v2","bundle_info":{"payload":{
                "thread_entry":{"channel_id":"C2","thread_ts":"100.000001",
                    "latest_ts":"900.000009","min_unread_ts":"150.000002","unread_msg_count":4}
            }}}
        }))
        .unwrap();
        let destination = thread_destination(&notification).unwrap();
        assert_eq!(destination.root, "100.000001");
        assert_eq!(destination.target, None);
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        shell.main_view = MainView::Activity;
        assert!(shell.select_activity_item(
            notification.key.clone(),
            notification.channel(),
            notification.ts(),
            Some(&destination.root)
        ));
        shell.begin_thread(destination.root.clone(), destination.target.clone());
        assert!(shell.thread_at_bottom);
        assert!(shell.thread_target.is_none());
        let request = ThreadHistory::initial(destination.target.as_deref(), 2000)
            .request("C2".into(), destination.root);
        assert!(request.oldest.is_none());
        assert_eq!(request.latest.as_deref(), Some("2000.999999"));
    }

    #[test]
    fn root_notification_and_nonthread_items_do_not_invent_reply_targets() {
        assert_eq!(
            thread_destination(&item("reaction", "100.000001", Some("100.000001")))
                .unwrap()
                .target,
            None
        );
        assert!(thread_destination(&item("mention", "150.000002", None)).is_none());
        assert!(thread_destination(&ActivityItem::default()).is_none());
    }
}
