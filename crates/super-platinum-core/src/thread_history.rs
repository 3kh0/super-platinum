//! Bidirectional thread paging. Bounds exclude the parent, which Slack repeats
//! on every reply page, and never infer continuity from cached message count.
use crate::slack::{api::RepliesArgs, models::HistoryPage};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Direction {
    Earlier,
    Newer,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Page {
    pub oldest: Option<String>,
    pub latest: Option<String>,
    pub cursor: Option<String>,
    pub inclusive: bool,
}
impl Page {
    pub fn request(&self, channel: String, root: String) -> RepliesArgs {
        RepliesArgs {
            channel,
            ts: root,
            oldest: self.oldest.clone(),
            latest: self.latest.clone(),
            cursor: self.cursor.clone(),
            inclusive: self.inclusive,
            limit: Some(200),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ThreadHistory {
    pub earlier: Option<Page>,
    pub newer: Option<Page>,
    pub loading: Option<Direction>,
    pub failed: Option<Direction>,
    first: Option<String>,
    last: Option<String>,
    initialized: bool,
    live_after: Option<String>,
    cursors: HashSet<(Direction, String)>,
}
impl ThreadHistory {
    pub fn watch_live(&mut self, now: i64) {
        self.live_after = Some(format!("{now}.000000"));
    }
    pub fn initial(target: Option<&str>, now: i64) -> Page {
        Page {
            oldest: target.map(str::to_owned),
            latest: target.is_none().then(|| format!("{now}.999999")),
            inclusive: true,
            ..Default::default()
        }
    }
    pub fn start(&mut self, request: &Page, root: &str, page: &HistoryPage) {
        let live_after = self.live_after.take();
        *self = Self::default();
        self.live_after = live_after;
        let direction = if let Some(target) = request.oldest.as_ref() {
            if target != root {
                self.earlier = Some(Page {
                    latest: Some(target.clone()),
                    ..Default::default()
                });
            }
            Direction::Newer
        } else {
            Direction::Earlier
        };
        self.apply(direction, request, root, page);
    }
    pub fn next(&self, direction: Direction) -> Option<&Page> {
        match direction {
            Direction::Earlier => self.earlier.as_ref(),
            Direction::Newer => self.newer.as_ref(),
        }
    }
    pub fn begin(&mut self, direction: Direction) -> Option<Page> {
        if self.loading.is_some() {
            return None;
        }
        let request = self.next(direction)?.clone();
        self.loading = Some(direction);
        self.failed = None;
        Some(request)
    }
    pub fn fail(&mut self, direction: Direction) {
        self.loading = None;
        self.failed = Some(direction);
    }
    pub fn apply(&mut self, direction: Direction, request: &Page, root: &str, page: &HistoryPage) {
        self.loading = None;
        self.failed = None;
        self.initialized = true;
        let mut ids = page
            .messages
            .iter()
            .filter_map(|m| m.ts.as_ref())
            .filter(|ts| ts.as_str() != root)
            .collect::<Vec<_>>();
        ids.sort();
        if let Some(first) = ids.first() {
            if self.first.as_ref().is_none_or(|old| *first < old) {
                self.first = Some((*first).clone());
            }
        }
        if let Some(last) = ids.last() {
            if self.last.as_ref().is_none_or(|old| *last > old) {
                self.last = Some((*last).clone());
            }
        }
        let cursor = page
            .response_metadata
            .as_ref()
            .and_then(|m| m.next_cursor.as_ref())
            .filter(|c| !c.trim().is_empty());
        let next = if direction == Direction::Newer && (page.has_more || cursor.is_some()) {
            // Slack's web-session forward cursor combined with `oldest` skips
            // the first reply of each continuation page (verified live with a
            // three-reply page). Continue exclusively after the last reply we
            // actually received. The repeated parent is never a boundary.
            match ids.last() {
                Some(last)
                    if request
                        .oldest
                        .as_ref()
                        .is_none_or(|old| *last > old || (request.inclusive && *last == old)) =>
                {
                    Some(Page {
                        oldest: Some((*last).clone()),
                        latest: request.latest.clone(),
                        ..Default::default()
                    })
                }
                _ => {
                    self.failed = Some(direction);
                    Some(request.clone())
                }
            }
        } else if let Some(cursor) = cursor {
            if request.cursor.as_ref() == Some(cursor)
                || !self.cursors.insert((direction, cursor.clone()))
            {
                // Keep the request retryable; never spin or silently declare a
                // repeated cursor to be the end of a conversation.
                self.failed = Some(direction);
                Some(request.clone())
            } else {
                Some(Page {
                    cursor: Some(cursor.clone()),
                    ..request.clone()
                })
            }
        } else if page.has_more {
            let boundary = match direction {
                Direction::Earlier => ids.first(),
                Direction::Newer => ids.last(),
            };
            match boundary {
                Some(boundary)
                    if match direction {
                        Direction::Earlier => {
                            request.latest.as_ref().is_none_or(|old| *boundary < old)
                        }
                        Direction::Newer => {
                            request.oldest.as_ref().is_none_or(|old| *boundary > old)
                        }
                    } =>
                {
                    Some(match direction {
                        Direction::Earlier => Page {
                            latest: Some((*boundary).clone()),
                            oldest: request.oldest.clone(),
                            ..Default::default()
                        },
                        Direction::Newer => Page {
                            oldest: Some((*boundary).clone()),
                            latest: request.latest.clone(),
                            ..Default::default()
                        },
                    })
                }
                _ => {
                    self.failed = Some(direction);
                    Some(request.clone())
                }
            }
        } else {
            None
        };
        match direction {
            Direction::Earlier => self.earlier = next,
            Direction::Newer => self.newer = next,
        }
    }
    /// Cached replies beyond an unfilled page gap must not look contiguous.
    pub fn contains(&self, root: &str, ts: &str) -> bool {
        if !self.initialized || ts == root {
            return true;
        }
        self.first
            .as_ref()
            .is_some_and(|first| ts >= first.as_str())
            && (self.newer.is_none() || self.last.as_ref().is_some_and(|last| ts <= last.as_str()))
    }
    /// Replies arriving during this visit, including optimistic sends, remain
    /// visible below the newer-page control while the intervening gap is open.
    pub fn visible(&self, root: &str, ts: &str) -> bool {
        self.contains(root, ts)
            || self
                .live_after
                .as_ref()
                .is_some_and(|after| ts >= after.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn page(ids: &[&str], cursor: Option<&str>, more: bool) -> HistoryPage {
        serde_json::from_value(serde_json::json!({"messages":ids.iter().map(|ts|serde_json::json!({"ts":ts})).collect::<Vec<_>>(),"has_more":more,"response_metadata":{"next_cursor":cursor}})).unwrap()
    }
    #[test]
    fn parent_only_page_hides_stale_cached_replies_but_keeps_new_arrivals() {
        let mut h = ThreadHistory::default();
        h.watch_live(200);
        h.start(
            &ThreadHistory::initial(None, 200),
            "100.0",
            &page(&["100.0"], None, false),
        );
        assert!(h.visible("100.0", "100.0"));
        assert!(!h.visible("100.0", "110.0"));
        assert!(h.visible("100.0", "200.000001"));
    }
    #[test]
    fn forward_pages_use_received_boundary_and_keep_live_replies_below_gap() {
        let mut h = ThreadHistory::default();
        h.watch_live(200);
        let request = ThreadHistory::initial(Some("105.0"), 200);
        h.start(
            &request,
            "100.0",
            &page(&["100.0", "105.0", "107.0"], Some("skips-next"), true),
        );
        let next = h.begin(Direction::Newer).unwrap();
        assert_eq!(next.oldest.as_deref(), Some("107.0"));
        assert!(next.cursor.is_none());
        assert!(!next.inclusive);
        assert!(!h.visible("100.0", "110.0"));
        assert!(h.visible("100.0", "200.000000"));
        assert!(!h.contains("100.0", "200.000000"));
        h.apply(
            Direction::Newer,
            &next,
            "100.0",
            &page(&["100.0", "108.0", "110.0"], None, false),
        );
        assert!(h.contains("100.0", "200.000000"));
    }
    #[test]
    fn tail_cursor_walks_backward_and_ignores_repeated_parent() {
        let mut history = ThreadHistory::default();
        let request = ThreadHistory::initial(None, 200);
        history.start(
            &request,
            "100.0",
            &page(&["100.0", "110.0", "111.0"], Some("older"), true),
        );
        assert!(history.newer.is_none());
        assert!(!history.contains("100.0", "105.0"));
        assert!(history.contains("100.0", "112.0"));
        let next = history.begin(Direction::Earlier).unwrap();
        assert_eq!(next.latest, request.latest);
        assert_eq!(next.cursor.as_deref(), Some("older"));
        assert!(history.begin(Direction::Earlier).is_none());
        history.apply(
            Direction::Earlier,
            &next,
            "100.0",
            &page(&["100.0", "105.0", "109.0"], None, false),
        );
        assert!(history.earlier.is_none());
        assert!(history.contains("100.0", "105.0"));
    }
    #[test]
    fn middle_target_has_two_independent_edges_and_hides_cached_tail_gap() {
        let mut h = ThreadHistory::default();
        let request = ThreadHistory::initial(Some("105.0"), 200);
        h.start(
            &request,
            "100.0",
            &page(&["100.0", "105.0", "106.0"], Some("newer"), true),
        );
        assert_eq!(h.earlier.as_ref().unwrap().latest.as_deref(), Some("105.0"));
        assert!(!h.earlier.as_ref().unwrap().inclusive);
        assert!(h.contains("100.0", "100.0"));
        assert!(!h.contains("100.0", "103.0"));
        assert!(!h.contains("100.0", "110.0"));
        let newer = h.begin(Direction::Newer).unwrap();
        h.apply(
            Direction::Newer,
            &newer,
            "100.0",
            &page(&["100.0", "107.0", "110.0"], None, false),
        );
        assert!(h.contains("100.0", "111.0"));
        assert!(h.earlier.is_some());
        let older = h.begin(Direction::Earlier).unwrap();
        h.apply(
            Direction::Earlier,
            &older,
            "100.0",
            &page(&["100.0", "103.0", "104.0"], Some("older"), true),
        );
        assert!(h.contains("100.0", "103.0"));
        assert!(h.newer.is_none());
    }
    #[test]
    fn missing_cursor_advances_without_including_parent_or_boundary_again() {
        let mut h = ThreadHistory::default();
        let request = ThreadHistory::initial(None, 200);
        h.start(
            &request,
            "100.0",
            &page(&["100.0", "108.0", "109.0"], None, true),
        );
        assert_eq!(h.earlier.as_ref().unwrap().latest.as_deref(), Some("108.0"));
        assert!(!h.earlier.as_ref().unwrap().inclusive);
        let next = h.begin(Direction::Earlier).unwrap();
        h.fail(Direction::Earlier);
        assert_eq!(h.begin(Direction::Earlier), Some(next));
    }
    #[test]
    fn repeated_or_cyclic_cursor_stays_retryable_and_parent_only_page_ends() {
        let mut h = ThreadHistory::default();
        let request = ThreadHistory::initial(None, 200);
        h.start(
            &request,
            "100.0",
            &page(&["100.0", "109.0"], Some("a"), true),
        );
        let a = h.begin(Direction::Earlier).unwrap();
        h.apply(
            Direction::Earlier,
            &a,
            "100.0",
            &page(&["100.0", "108.0"], Some("b"), true),
        );
        let b = h.begin(Direction::Earlier).unwrap();
        h.apply(
            Direction::Earlier,
            &b,
            "100.0",
            &page(&["100.0", "107.0"], Some("a"), true),
        );
        assert_eq!(h.failed, Some(Direction::Earlier));
        assert_eq!(h.earlier, Some(b));
        let retry = h.begin(Direction::Earlier).unwrap();
        h.apply(
            Direction::Earlier,
            &retry,
            "100.0",
            &page(&["100.0"], None, false),
        );
        assert!(h.earlier.is_none());
        assert!(h.failed.is_none());
    }
}
