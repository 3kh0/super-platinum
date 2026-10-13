//! Lightweight thread geometry; rich rows are projected only near the viewport.
use std::collections::{HashMap, HashSet};
use super_platinum_core::{state::ChannelMessages, thread_history::ThreadHistory};

pub(crate) const LIMIT: usize = 100;
#[derive(Clone, Debug)]
pub(crate) struct Row {
    pub ts: String,
    pub history: bool,
    pub pending: bool,
    pub source_index: usize,
    pub previous_source_index: Option<usize>,
    history_index: usize,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum PaintPiece {
    Spacer { index: usize, height: f64 },
    Row { index: usize, projected: usize },
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
    pub projection_generation: u64,
    source_len: usize,
    pending_snapshot: Vec<String>,
    selected_indices: Vec<usize>,
    always_selected: Vec<usize>,
    history_rows: [Vec<usize>; 2],
    history_offsets: [Vec<f64>; 2],
    pieces: [Vec<PaintPiece>; 2],
}
impl ThreadWindow {
    pub fn user_navigation(&mut self) {
        self.input_generation = self.input_generation.wrapping_add(1);
    }
    pub fn measurement_is_current(&self, scope: &str, input_generation: u64) -> bool {
        self.scope == scope && self.input_generation == input_generation
    }

    pub fn projection_receipt_is_current(&self, sampled: u64, dom: Option<&str>) -> bool {
        self.projection_generation == sampled
            && dom.and_then(|generation| generation.parse::<u64>().ok()) == Some(sampled)
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
        // Canonical updates may change geometry without changing any row IDs.
        // Keep this separate from viewport-only measurements and user input.
        let projection_generation = self.projection_generation.wrapping_add(1);
        if self.scope != scope {
            *self = Self {
                scope: scope.into(),
                ..Default::default()
            };
        }
        self.projection_generation = projection_generation;
        let held = self
            .rows
            .get(self.start..self.end)
            .unwrap_or_default()
            .iter()
            .map(|r| r.ts.as_str())
            .collect::<HashSet<_>>();
        let mut previous_source_index = None;
        let mut history_rows = [Vec::new(), Vec::new()];
        let mut always_selected = Vec::new();
        let mut row_index = 0;
        let rows = bag
            .messages
            .iter()
            .enumerate()
            .filter_map(|(source_index, m)| {
                let ts = m.ts.as_deref()?;
                let predecessor = previous_source_index.replace(source_index);
                let pending = bag.is_pending(ts);
                if !history.visible(root, ts) && !pending {
                    return None;
                }
                let in_history = history.contains(root, ts);
                let history_index = history_rows[usize::from(in_history)].len();
                history_rows[usize::from(in_history)].push(row_index);
                if ts == root || pending {
                    always_selected.push(row_index);
                }
                row_index += 1;
                Some(Row {
                    ts: ts.into(),
                    history: in_history,
                    pending,
                    source_index,
                    previous_source_index: predecessor,
                    history_index,
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
        self.source_len = bag.messages.len();
        self.pending_snapshot = bag.pending.clone();
        self.rows = rows;
        self.always_selected = always_selected;
        self.history_rows = history_rows;
        self.start = start;
        self.end = end;
        self.prune();
        self.rebuild_offsets();
        self.rebuild_selection();
    }
    // Kept as the independent linear selection reference in regression probes.
    #[allow(dead_code)]
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
        let mut geometry_changed = false;
        for (id, height) in heights {
            if height.is_finite() && *height > 0.0 && self.heights.get(id) != Some(height) {
                self.heights.insert(id.clone(), *height);
                changed = true;
                geometry_changed = true;
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
        geometry_changed |= self.prune();
        if geometry_changed {
            self.rebuild_offsets();
        }
        if changed || geometry_changed {
            self.rebuild_selection();
        }
        changed
    }
    /// Viewport-only updates reuse canonical source positions and history flags.
    /// Core mutations must still call sync before rich projection.
    pub fn align_viewport(&mut self, target: Option<&str>, bottom: bool) {
        let old = (self.start, self.end);
        if let Some(index) = target.and_then(|ts| self.rows.iter().position(|row| row.ts == ts)) {
            self.start = index.saturating_sub(24);
            self.end = (self.start + LIMIT).min(self.rows.len());
        } else if bottom {
            self.start = self.start.max(self.rows.len().saturating_sub(LIMIT));
            self.end = self.rows.len();
        } else if self.start >= self.end {
            self.start = self.rows.len().saturating_sub(LIMIT);
            self.end = self.rows.len();
        }
        let geometry_changed = self.prune();
        if geometry_changed {
            self.rebuild_offsets();
        }
        if old != (self.start, self.end) || geometry_changed {
            self.rebuild_selection();
        }
    }
    pub fn source_shape_matches(&self, bag: &ChannelMessages) -> bool {
        self.source_len == bag.messages.len() && self.pending_snapshot == bag.pending
    }
    pub fn selected_indices(&self) -> &[usize] {
        &self.selected_indices
    }
    pub fn pieces(&self, history: bool) -> &[PaintPiece] {
        &self.pieces[usize::from(history)]
    }

    fn rebuild_offsets(&mut self) {
        for history in [false, true] {
            let indices = &self.history_rows[usize::from(history)];
            let mut offsets = Vec::with_capacity(indices.len() + 1);
            let mut offset = 0.0;
            offsets.push(offset);
            for &index in indices {
                offset += self.height(&self.rows[index].ts);
                offsets.push(offset);
            }
            self.history_offsets[usize::from(history)] = offsets;
        }
    }
    fn rebuild_selection(&mut self) {
        self.selected_indices.clear();
        self.selected_indices.extend(self.start..self.end);
        self.selected_indices
            .extend_from_slice(&self.always_selected);
        self.selected_indices.sort_unstable();
        self.selected_indices.dedup();
        for history in [false, true] {
            let slot = usize::from(history);
            let offsets = &self.history_offsets[slot];
            let indices = &self.history_rows[slot];
            let mut pieces = Vec::with_capacity(self.selected_indices.len() + 2);
            let mut cursor = 0;
            for (projected, &index) in self.selected_indices.iter().enumerate() {
                let row = &self.rows[index];
                if row.history != history {
                    continue;
                }
                if cursor < row.history_index {
                    pieces.push(PaintPiece::Spacer {
                        index: indices[cursor],
                        height: offsets[row.history_index] - offsets[cursor],
                    });
                }
                pieces.push(PaintPiece::Row { index, projected });
                cursor = row.history_index + 1;
            }
            if cursor < indices.len() {
                pieces.push(PaintPiece::Spacer {
                    index: indices[cursor],
                    height: offsets[indices.len()] - offsets[cursor],
                });
            }
            self.pieces[slot] = pieces;
        }
    }
    fn prune(&mut self) -> bool {
        let before = self.heights.len();
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
        before != self.heights.len()
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

#[cfg(test)]
mod cache_tests;
