//! Lightweight thread geometry; rich rows are projected only near the viewport.
use std::collections::{HashMap, HashSet};
use super_platinum_core::{state::ChannelMessages, thread_history::ThreadHistory};

pub(crate) const LIMIT: usize = 100;
#[derive(Clone, Debug)]
pub(crate) struct Row {
    pub ts: String,
    pub history: bool,
    pub pending: bool,
}
#[derive(Default)]
pub(crate) struct ThreadWindow {
    scope: String,
    pub rows: Vec<Row>,
    pub start: usize,
    pub end: usize,
    heights: HashMap<String, f64>,
    pub selection_pinned: bool,
    pub measured: bool,
    pub input_generation: u64,
}
impl ThreadWindow {
    pub fn user_navigation(&mut self) {
        self.input_generation = self.input_generation.wrapping_add(1);
    }
    pub fn measurement_is_current(&self, scope: &str, input_generation: u64) -> bool {
        self.scope == scope && self.input_generation == input_generation
    }

    pub fn sync(
        &mut self,
        scope: &str,
        bag: &ChannelMessages,
        history: &ThreadHistory,
        root: &str,
        target: Option<&str>,
        bottom: bool,
    ) {
        if self.scope != scope {
            *self = Self {
                scope: scope.into(),
                ..Default::default()
            };
        }
        let held = self
            .rows
            .get(self.start..self.end)
            .unwrap_or_default()
            .iter()
            .map(|r| r.ts.as_str())
            .collect::<HashSet<_>>();
        let rows = bag
            .messages
            .iter()
            .filter_map(|m| {
                let ts = m.ts.as_deref()?;
                let pending = bag.is_pending(ts);
                (history.visible(root, ts) || pending).then(|| Row {
                    ts: ts.into(),
                    history: history.contains(root, ts),
                    pending,
                })
            })
            .collect::<Vec<_>>();
        let positions = rows
            .iter()
            .enumerate()
            .filter_map(|(i, r)| held.contains(r.ts.as_str()).then_some(i))
            .collect::<Vec<_>>();
        let targeted = target.and_then(|ts| rows.iter().position(|r| r.ts == ts));
        let (start, end) = if let Some(index) = targeted {
            let start = index.saturating_sub(24);
            (start, (start + LIMIT).min(rows.len()))
        } else if bottom {
            (
                positions
                    .first()
                    .copied()
                    .unwrap_or(0)
                    .max(rows.len().saturating_sub(LIMIT)),
                rows.len(),
            )
        } else if positions.is_empty() {
            (rows.len().saturating_sub(LIMIT), rows.len())
        } else {
            let start = positions[0];
            (
                start,
                (positions.last().copied().unwrap() + 1).min(start + LIMIT),
            )
        };
        self.rows = rows;
        self.start = start;
        self.end = end;
        self.prune();
    }
    pub fn selected(&self, index: usize, root: &str) -> bool {
        let row = &self.rows[index];
        row.ts == root || row.pending || (self.start..self.end).contains(&index)
    }
    pub fn height(&self, ts: &str) -> f64 {
        self.heights.get(ts).copied().unwrap_or(66.0)
    }
    pub fn estimated(&self, top: f64, height: f64, gap: f64, gap_before: usize) -> (usize, usize) {
        let top = if top.is_finite() { top.max(0.0) } else { 0.0 };
        let bottom = top
            + if height.is_finite() {
                height.max(0.0)
            } else {
                0.0
            };
        let mut offset = 0.0;
        let mut first = None;
        for (i, row) in self.rows.iter().enumerate() {
            if i == gap_before {
                offset += gap;
            }
            offset += self.height(&row.ts);
            if first.is_none() && offset > top {
                first = Some(i);
            }
            if offset >= bottom {
                return (first.unwrap_or(i), i);
            }
        }
        let last = self.rows.len().saturating_sub(1);
        (first.unwrap_or(last), last)
    }
    pub fn would_change(
        &self,
        first: usize,
        last: usize,
        heights: &[(String, f64)],
        target: bool,
    ) -> bool {
        heights
            .iter()
            .any(|(id, h)| h.is_finite() && *h > 0.0 && self.heights.get(id) != Some(h))
            || (!target && !self.selection_pinned && {
                let start = first.saturating_sub(12).min(self.rows.len());
                let end = last
                    .saturating_add(13)
                    .min(self.rows.len())
                    .min(start + LIMIT);
                end > start && (self.start, self.end) != (start, end)
            })
    }
    pub fn measure(
        &mut self,
        first: usize,
        last: usize,
        heights: &[(String, f64)],
        target: bool,
    ) -> bool {
        self.measured = true;
        let mut changed = false;
        for (id, height) in heights {
            if height.is_finite() && *height > 0.0 && self.heights.get(id) != Some(height) {
                self.heights.insert(id.clone(), *height);
                changed = true;
            }
        }
        if !target && !self.selection_pinned {
            let start = first.saturating_sub(12).min(self.rows.len());
            let end = last
                .saturating_add(13)
                .min(self.rows.len())
                .min(start + LIMIT);
            if end > start && (self.start, self.end) != (start, end) {
                self.start = start;
                self.end = end;
                changed = true;
            }
        }
        self.prune();
        changed
    }
    fn prune(&mut self) {
        if self.heights.len() > 600 {
            let keep = self
                .rows
                .iter()
                .skip(self.start.saturating_sub(200))
                .take(400)
                .chain(self.rows.iter().rev().take(100))
                .map(|r| r.ts.as_str())
                .collect::<HashSet<_>>();
            self.heights.retain(|id, _| keep.contains(id.as_str()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn bag(count: usize) -> ChannelMessages {
        let mut bag = ChannelMessages::default();
        bag.messages=(0..count).map(|i|serde_json::from_value(serde_json::json!({"ts":format!("{}.{:06}",100,i),"user":"U1","text":"Synthetic reply"})).unwrap()).collect();
        bag
    }
    #[test]
    fn input_retires_pending_measurement_even_before_next_scroll_event() {
        let mut window = ThreadWindow::default();
        let bag = bag(2000);
        let history = ThreadHistory::default();
        window.sync("visit", &bag, &history, "100.000000", None, true);
        let sampled = window.input_generation;
        assert!(window.measurement_is_current("visit", sampled));
        // A bottom sample can be in IPC while the reader starts dragging to
        // the top. No subsequent scroll event is required to retire it.
        window.user_navigation();
        window.sync("visit", &bag, &history, "100.000000", None, false);
        assert!(!window.measurement_is_current("visit", sampled));
        let fresh = window.input_generation;
        assert!(window.measurement_is_current("visit", fresh));
        window.sync("new visit", &bag, &history, "100.000000", None, true);
        assert!(!window.measurement_is_current("visit", fresh));
    }
    #[test]
    fn target_root_pending_and_bottom_are_bounded() {
        let mut bag = bag(10000);
        bag.pending.push("100.009999".into());
        let mut window = ThreadWindow::default();
        window.sync(
            "visit",
            &bag,
            &ThreadHistory::default(),
            "100.000000",
            Some("100.000250"),
            false,
        );
        let selected = (0..window.rows.len())
            .filter(|i| window.selected(*i, "100.000000"))
            .collect::<Vec<_>>();
        assert!(selected.contains(&0));
        assert!(selected.contains(&250));
        assert!(selected.contains(&9999));
        assert!(selected.len() <= LIMIT + 2);
        window.sync(
            "visit",
            &bag,
            &ThreadHistory::default(),
            "100.000000",
            None,
            true,
        );
        assert_eq!(window.end, 10000);
        assert_eq!(window.start, 9900);
    }
    #[test]
    fn refresh_retains_surviving_rows_and_target_cannot_be_evicted() {
        let mut bag = bag(500);
        let mut window = ThreadWindow::default();
        let history = ThreadHistory::default();
        window.sync(
            "visit",
            &bag,
            &history,
            "100.000000",
            Some("100.000200"),
            false,
        );
        window.measure(0, 4, &[], true);
        assert!(window.start <= 200 && window.end > 200);
        let held = window.rows[window.start].ts.clone();
        bag.messages.remove(2);
        window.sync("visit", &bag, &history, "100.000000", None, false);
        assert_eq!(window.rows[window.start].ts, held);
        window.measure(480, 498, &[], false);
        window.sync("visit", &bag, &history, "100.000000", None, true);
        assert!(window.end - window.start < 100);
    }
    #[test]
    fn scroll_estimates_and_measurement_cache_are_bounded_and_scoped() {
        let bag = bag(2000);
        let mut window = ThreadWindow::default();
        let history = ThreadHistory::default();
        window.sync("visit", &bag, &history, "100.000000", None, true);
        assert_eq!(window.estimated(66000.0, 132.0, 0.0, 2000), (1000, 1001));
        let heights = window
            .rows
            .iter()
            .map(|r| (r.ts.clone(), 70.0))
            .collect::<Vec<_>>();
        window.measure(1000, 1005, &heights, false);
        assert!(window.heights.len() <= 500);
        window.selection_pinned = true;
        let held = (window.start, window.end);
        window.measure(1, 10, &[], false);
        assert_eq!((window.start, window.end), held);
        window.sync("new visit", &bag, &history, "100.000000", None, true);
        assert!(window.heights.is_empty());
        assert!(!window.selection_pinned);
    }
}
