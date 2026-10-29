//! Paginated subscriptions, isolated by account/workspace. In-flight pages replay
//! realtime changes so old previews cannot resurrect deleted or edited messages.
use crate::{
    CoreAppState,
    domain::ThreadKey,
    slack::{
        events::RtEvent,
        models::{Message, MessageTs, ThreadViewItem, ThreadsViewPage},
    },
    state::{cmp_ts, merge_message},
};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedRequest {
    pub sequence: u64,
    pub cursor: Option<String>,
}
#[derive(Debug, Clone)]
enum Change {
    Message(String, Message, bool),
    Delete(String, String),
    Unsubscribe(String, String),
}
#[derive(Debug, Clone, Default)]
pub struct ThreadsState {
    pub items: Vec<ThreadViewItem>,
    pub max_ts: Option<MessageTs>,
    pub next_cursor: Option<MessageTs>,
    pub has_more: bool,
    pub load_seq: u64,
    pub loading: bool,
    pub loaded: bool,
    pub failed: bool,
    pub dirty: bool,
    pub vip_only: bool,
    pub selected: Option<(String, MessageTs)>,
    scope: Option<(u64, String)>,
    seen_cursors: HashSet<String>,
    changes: Vec<Change>,
    pending: Option<FeedRequest>,
    retry: Option<FeedRequest>,
}
impl ThreadsState {
    pub fn matches_scope(&self, epoch: u64, team: &str) -> bool {
        self.scope
            .as_ref()
            .is_some_and(|(e, t)| *e == epoch && t == team)
    }
    pub fn ensure_scope(&mut self, epoch: u64, team: &str) {
        if !self.matches_scope(epoch, team) {
            let seq = self.load_seq.wrapping_add(1);
            *self = Self {
                scope: Some((epoch, team.into())),
                load_seq: seq,
                ..Default::default()
            };
        }
    }
    pub fn begin(&mut self, append: bool) -> Option<FeedRequest> {
        if append && (!self.has_more || self.next_cursor.is_none()) {
            return None;
        }
        self.begin_cursor(if append {
            self.next_cursor.clone()
        } else {
            None
        })
    }
    pub fn failed_read(&self) -> Option<&FeedRequest> {
        self.retry.as_ref()
    }
    pub fn begin_retry(&mut self) -> Option<FeedRequest> {
        let cursor = self.retry.as_ref()?.cursor.clone();
        self.begin_cursor(cursor)
    }
    fn begin_cursor(&mut self, cursor: Option<String>) -> Option<FeedRequest> {
        if self.loading {
            return None;
        }
        self.loading = true;
        self.failed = false;
        self.dirty = false;
        self.changes.clear();
        self.retry = None;
        self.load_seq = self.load_seq.wrapping_add(1);
        let request = FeedRequest {
            sequence: self.load_seq,
            cursor,
        };
        self.pending = Some(request.clone());
        Some(request)
    }
    /// Settlement requires the coordinate actually admitted by `begin`/Retry.
    /// The public FeedRequest shape remains constructible; mutating its fields
    /// no longer changes which pending page a caller may settle.
    pub fn accepts(&self, request: &FeedRequest) -> bool {
        self.loading && self.load_seq == request.sequence && self.pending.as_ref() == Some(request)
    }
    pub fn fail(&mut self, request: &FeedRequest) {
        if self.accepts(request) {
            self.loading = false;
            self.failed = true;
            self.retry = Some(request.clone());
            self.pending = None;
            self.changes.clear();
        }
    }
    /// Quiet local release, without fabricating a failed HTTP response.
    /// Native callers must also retain their original shell/slot ownership.
    pub fn cancel(&mut self, request: &FeedRequest) -> bool {
        if !self.accepts(request) {
            return false;
        }
        self.loading = false;
        self.pending = None;
        self.retry = None;
        self.changes.clear();
        true
    }
    pub fn upsert(&mut self, item: ThreadViewItem) {
        let Some((channel, root)) = item.channel().zip(item.root_ts()) else {
            return;
        };
        if let Some(existing) = self
            .items
            .iter_mut()
            .find(|i| i.channel() == Some(channel) && i.root_ts() == Some(root))
        {
            *existing = item;
        } else {
            self.items.push(item);
        }
    }
    fn sort(&mut self) {
        self.items.sort_by(|a, b| {
            a.unread_replies
                .is_empty()
                .cmp(&b.unread_replies.is_empty())
                .then_with(|| {
                    cmp_ts(
                        b.latest_ts().map(String::as_str),
                        a.latest_ts().map(String::as_str),
                    )
                })
        });
    }
    pub fn observe(&mut self, event: &RtEvent) {
        let change = match event {
            RtEvent::Message(m) => m
                .channel
                .clone()
                .map(|c| Change::Message(c, m.clone(), true)),
            RtEvent::MessageChanged {
                channel, message, ..
            } => Some(Change::Message(channel.clone(), message.clone(), false)),
            RtEvent::MessageDeleted {
                channel,
                deleted_ts,
            } => Some(Change::Delete(channel.clone(), deleted_ts.clone())),
            RtEvent::Unknown(raw)
                if raw.kind == "thread_unsubscribed" || raw.kind == "thread_subscribed" =>
            {
                let Some(sub) = raw.rest.get("subscription") else {
                    return;
                };
                let Some((c, r)) = sub
                    .get("channel")
                    .and_then(|v| v.as_str())
                    .zip(sub.get("thread_ts").and_then(|v| v.as_str()))
                else {
                    return;
                };
                if raw.kind == "thread_subscribed" {
                    self.dirty = true;
                    None
                } else {
                    Some(Change::Unsubscribe(c.into(), r.into()))
                }
            }
            _ => None,
        };
        if let Some(change) = change {
            self.apply_change(&change);
            if self.loading {
                self.changes.push(change);
            }
        }
    }
    fn apply_change(&mut self, change: &Change) {
        match change {
            Change::Unsubscribe(channel, root) => self
                .items
                .retain(|i| i.channel() != Some(channel) || i.root_ts() != Some(root)),
            Change::Delete(channel, ts) => {
                self.items
                    .retain(|i| i.channel() != Some(channel) || i.root_ts() != Some(ts));
                for item in self
                    .items
                    .iter_mut()
                    .filter(|i| i.channel() == Some(channel))
                {
                    let present = item.replies().iter().any(|m| m.ts.as_ref() == Some(ts));
                    item.latest_replies.retain(|m| m.ts.as_ref() != Some(ts));
                    item.unread_replies.retain(|m| m.ts.as_ref() != Some(ts));
                    if present {
                        item.root_msg.reply_count =
                            item.root_msg.reply_count.map(|n| n.saturating_sub(1));
                    }
                }
            }
            Change::Message(channel, message, arrival) => {
                let Some(ts) = message.ts.as_ref() else {
                    return;
                };
                for item in self
                    .items
                    .iter_mut()
                    .filter(|i| i.channel() == Some(channel))
                {
                    if item.root_ts() == Some(ts) {
                        merge_message(&mut item.root_msg, message.clone());
                        continue;
                    }
                    let known = item.replies().iter().any(|m| m.ts.as_ref() == Some(ts));
                    let incoming = *arrival && message.thread_ts.as_ref() == item.root_ts();
                    if !known && !incoming {
                        continue;
                    }
                    if incoming && cmp_ts(Some(ts), item.root_msg.latest_reply.as_deref()).is_gt() {
                        item.root_msg.latest_reply = Some(ts.clone());
                        item.root_msg.reply_count =
                            item.root_msg.reply_count.map(|n| n.saturating_add(1));
                    }
                    for replies in [&mut item.latest_replies, &mut item.unread_replies] {
                        if let Some(existing) =
                            replies.iter_mut().find(|m| m.ts.as_ref() == Some(ts))
                        {
                            merge_message(existing, message.clone());
                        }
                    }
                    if incoming && !known {
                        item.latest_replies.push(message.clone());
                        item.latest_replies
                            .sort_by(|a, b| cmp_ts(a.ts.as_deref(), b.ts.as_deref()));
                        if item.latest_replies.len() > 3 {
                            item.latest_replies.drain(..item.latest_replies.len() - 3);
                        }
                        if item
                            .root_msg
                            .extra
                            .get("last_read")
                            .and_then(|v| v.as_str())
                            .is_some_and(|cursor| cmp_ts(Some(ts), Some(cursor)).is_gt())
                        {
                            item.unread_replies.push(message.clone());
                        }
                    }
                }
            }
        }
    }
}
impl CoreAppState {
    pub fn apply_thread_feed_page(
        &mut self,
        team: &str,
        request: &FeedRequest,
        mut page: ThreadsViewPage,
        cursors: &HashMap<ThreadKey, u64>,
    ) -> bool {
        if self.active_team.as_deref() != Some(team)
            || !self.threads_view.matches_scope(self.account_epoch, team)
            || !self.threads_view.accepts(request)
        {
            return false;
        }
        // Slack orders unread threads first, then newest replies. The final
        // thread's reply timestamp is current_ts; response max_ts is not a cursor.
        page.threads.sort_by(|a, b| {
            a.unread_replies
                .is_empty()
                .cmp(&b.unread_replies.is_empty())
                .then_with(|| {
                    cmp_ts(
                        b.latest_ts().map(String::as_str),
                        a.latest_ts().map(String::as_str),
                    )
                })
        });
        let next = page.threads.last().and_then(|i| i.latest_ts()).cloned();
        if request.cursor.is_none() {
            self.threads_view.items.clear();
            self.threads_view.seen_cursors.clear();
        }
        for item in page.threads {
            let key = item
                .channel()
                .cloned()
                .zip(item.root_ts().cloned())
                .map(|(c, r)| (team.into(), c, r));
            let revision = key
                .as_ref()
                .and_then(|k| cursors.get(k))
                .copied()
                .unwrap_or(0);
            self.merge_thread_view_item(team, item, revision);
        }
        let feed = &mut self.threads_view;
        let advancing = next
            .as_ref()
            .is_some_and(|c| Some(c) != request.cursor.as_ref() && !feed.seen_cursors.contains(c));
        feed.has_more = page.has_more && advancing;
        feed.failed = page.has_more && !advancing;
        if let Some(c) = next.as_ref() {
            feed.seen_cursors.insert(c.clone());
        }
        feed.next_cursor = next;
        feed.max_ts = page.max_ts;
        for change in std::mem::take(&mut feed.changes) {
            feed.apply_change(&change);
        }
        feed.sort();
        feed.loaded = true;
        feed.loading = false;
        feed.pending = None;
        feed.retry = None;
        true
    }
}

#[cfg(test)]
mod recovery;

#[cfg(test)]
mod tests {
    use super::*;
    fn core() -> CoreAppState {
        let mut core = CoreAppState::new(Default::default());
        core.active_team = Some("T".into());
        core.threads_view.ensure_scope(core.account_epoch, "T");
        core
    }
    fn item(root: &str, latest: &str, unread: bool) -> ThreadViewItem {
        serde_json::from_value(serde_json::json!({"root_msg":{"channel":"C","ts":root,"text":"original","reply_count":147,"latest_reply":latest,"last_read":if unread {root}else{latest}},"latest_replies":[{"ts":latest,"thread_ts":root,"text":"reply"}],"unread_replies":if unread {serde_json::json!([{"ts":latest,"thread_ts":root,"text":"reply"}])}else{serde_json::json!([])}})).unwrap()
    }
    fn page(items: Vec<ThreadViewItem>, more: bool) -> ThreadsViewPage {
        ThreadsViewPage {
            threads: items,
            has_more: more,
            max_ts: Some("9999.0".into()),
            ..Default::default()
        }
    }
    #[test]
    fn pagination_uses_last_reply_not_response_max_ts_and_deduplicates_boundaries() {
        let mut c = core();
        let req = c.threads_view.begin(false).unwrap();
        assert!(c.apply_thread_feed_page(
            "T",
            &req,
            page(
                vec![item("10.0", "100.0", true), item("20.0", "200.0", true)],
                true
            ),
            &HashMap::new()
        ));
        assert_eq!(c.threads_view.next_cursor.as_deref(), Some("100.0"));
        assert_eq!(
            c.threads_view.items[0].root_ts().map(String::as_str),
            Some("20.0")
        );
        let req = c.threads_view.begin(true).unwrap();
        assert_eq!(req.cursor.as_deref(), Some("100.0"));
        c.apply_thread_feed_page(
            "T",
            &req,
            page(
                vec![item("10.0", "100.0", true), item("5.0", "50.0", false)],
                true,
            ),
            &HashMap::new(),
        );
        assert_eq!(c.threads_view.items.len(), 3);
        assert_eq!(c.threads_view.next_cursor.as_deref(), Some("50.0"));
        assert_eq!(c.threads_view.items[0].root_msg.reply_count, Some(147));
        let req = c.threads_view.begin(true).unwrap();
        c.apply_thread_feed_page(
            "T",
            &req,
            page(vec![item("5.0", "50.0", false)], true),
            &HashMap::new(),
        );
        assert!(!c.threads_view.has_more);
        assert!(c.threads_view.failed);
        assert!(c.threads_view.begin(true).is_none());
    }
    #[test]
    fn failure_keeps_items_and_cursor_for_retry_and_scope_change_rejects_old_page() {
        let mut c = core();
        let req = c.threads_view.begin(false).unwrap();
        c.apply_thread_feed_page(
            "T",
            &req,
            page(vec![item("10.0", "100.0", false)], true),
            &HashMap::new(),
        );
        let req = c.threads_view.begin(true).unwrap();
        c.threads_view.fail(&req);
        assert_eq!(c.threads_view.items.len(), 1);
        assert_eq!(c.threads_view.begin(true).unwrap().cursor, req.cursor);
        c.threads_view.ensure_scope(c.account_epoch, "OTHER");
        c.threads_view.ensure_scope(c.account_epoch, "T");
        assert!(!c.apply_thread_feed_page(
            "T",
            &req,
            page(vec![item("20.0", "200.0", false)], false),
            &HashMap::new()
        ));
        assert!(c.threads_view.items.is_empty());
        assert!(!c.threads_view.loading);
    }
    #[test]
    fn incoming_replies_and_edits_survive_a_late_page_without_inflating_counts() {
        let mut c = core();
        let req = c.threads_view.begin(false).unwrap();
        let original = item("10.0", "100.0", false);
        c.apply_thread_feed_page(
            "T",
            &req,
            page(vec![original.clone()], false),
            &HashMap::new(),
        );
        let req = c.threads_view.begin(false).unwrap();
        let reply: Message = serde_json::from_value(
            serde_json::json!({"channel":"C","thread_ts":"10.0","ts":"101.0","text":"new"}),
        )
        .unwrap();
        c.threads_view.observe(&RtEvent::Message(reply.clone()));
        c.threads_view.observe(&RtEvent::Message(reply));
        c.threads_view.observe(&RtEvent::MessageChanged {
            original_update: None,
            channel: "C".into(),
            message: Message {
                ts: Some("10.0".into()),
                text: Some("edited".into()),
                ..Default::default()
            },
        });
        c.apply_thread_feed_page("T", &req, page(vec![original], false), &HashMap::new());
        let i = &c.threads_view.items[0];
        assert_eq!(i.root_msg.text.as_deref(), Some("edited"));
        assert_eq!(i.root_msg.reply_count, Some(148));
        assert_eq!(i.unread_replies.len(), 1);
        assert_eq!(i.replies().len(), 2);
    }
    #[test]
    fn deletion_and_unsubscribe_during_refresh_are_not_resurrected() {
        let mut c = core();
        let req = c.threads_view.begin(false).unwrap();
        c.threads_view.observe(&RtEvent::MessageDeleted {
            channel: "C".into(),
            deleted_ts: "100.0".into(),
        });
        let unsubscribe = crate::slack::realtime::parse_event(
            r#"{"type":"thread_unsubscribed","subscription":{"channel":"C","thread_ts":"20.0"}}"#,
        )
        .unwrap();
        c.threads_view.observe(&unsubscribe);
        c.apply_thread_feed_page(
            "T",
            &req,
            page(
                vec![item("10.0", "100.0", true), item("20.0", "200.0", true)],
                false,
            ),
            &HashMap::new(),
        );
        assert_eq!(c.threads_view.items.len(), 1);
        assert!(c.threads_view.items[0].unread_replies.is_empty());
        assert_eq!(c.threads_view.items[0].root_msg.reply_count, Some(146));
    }
    #[test]
    fn read_mark_while_fetching_remains_authoritative() {
        let mut c = core();
        let req = c.threads_view.begin(false).unwrap();
        c.apply_thread_mark(&("T".into(), "C".into(), "10.0".into()), "100.0");
        c.apply_thread_feed_page(
            "T",
            &req,
            page(vec![item("10.0", "100.0", true)], false),
            &HashMap::new(),
        );
        assert!(c.threads_view.items[0].unread_replies.is_empty());
    }
}
