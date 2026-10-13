//! Standalone CPU probe. Includes production ThreadWindow; synthetic core types
//! isolate its geometry and equivalent renderer/index walks, not rich parsing/UI.
#![allow(dead_code)]
extern crate self as super_platinum_core;
mod state {
    #[derive(Default)]
    pub struct Message {
        pub ts: Option<String>,
    }
    #[derive(Default)]
    pub struct ChannelMessages {
        pub messages: Vec<Message>,
        pub pending: Vec<String>,
    }
    impl ChannelMessages {
        pub fn is_pending(&self, ts: &str) -> bool {
            self.pending.iter().any(|p| p == ts)
        }
    }
}
mod thread_history {
    #[derive(Default)]
    pub struct ThreadHistory;
    impl ThreadHistory {
        pub fn visible(&self, _: &str, _: &str) -> bool {
            true
        }
        pub fn contains(&self, _: &str, ts: &str) -> bool {
            ts.parse::<usize>().unwrap() % 3 != 0
        }
    }
}
#[path = "../src/desktop/thread_window.rs"]
mod thread_window;
use std::{collections::HashSet, hint::black_box, time::Instant};
fn sample(mut action: impl FnMut()) -> f64 {
    let now = Instant::now();
    for _ in 0..100 {
        action();
    }
    now.elapsed().as_secs_f64() * 10.0
}
fn stats(mut values: Vec<f64>) -> (f64, f64) {
    values.sort_by(f64::total_cmp);
    (
        values[values.len() / 2],
        values[(values.len() * 95).div_ceil(100) - 1],
    )
}
fn scan_indices(window: &thread_window::ThreadWindow, bag: &state::ChannelMessages) -> Vec<usize> {
    let selected = window
        .rows
        .iter()
        .enumerate()
        .filter(|(i, _)| window.selected(*i, "0"))
        .map(|(_, r)| r.ts.as_str())
        .collect::<HashSet<_>>();
    bag.messages
        .iter()
        .enumerate()
        .filter_map(|(i, m)| {
            m.ts.as_deref()
                .is_some_and(|ts| selected.contains(ts))
                .then_some(i)
        })
        .collect()
}
fn scan_pieces(window: &thread_window::ThreadWindow) -> Vec<(bool, usize, f64)> {
    let selected = window
        .rows
        .iter()
        .enumerate()
        .filter(|(i, _)| window.selected(*i, "0"))
        .map(|(_, r)| r.ts.as_str())
        .collect::<HashSet<_>>();
    let mut result = vec![];
    for history in [false, true] {
        let mut omitted = 0.0;
        let mut start = 0;
        for (i, row) in window
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.history == history)
        {
            if selected.contains(row.ts.as_str()) {
                if omitted > 0.0 {
                    result.push((history, start, omitted));
                    omitted = 0.0;
                }
                result.push((history, i, 0.0));
            } else {
                if omitted == 0.0 {
                    start = i;
                }
                omitted += window.height(&row.ts);
            }
        }
        if omitted > 0.0 {
            result.push((history, start, omitted));
        }
    }
    result
}

fn indexed_indices(
    window: &thread_window::ThreadWindow,
    bag: &state::ChannelMessages,
) -> Vec<usize> {
    assert!(window.source_shape_matches(bag));
    window
        .selected_indices()
        .iter()
        .map(|&index| {
            let row = &window.rows[index];
            assert_eq!(
                bag.messages[row.source_index].ts.as_deref(),
                Some(row.ts.as_str())
            );
            row.source_index
        })
        .collect()
}
fn cached_pieces(window: &thread_window::ThreadWindow) -> Vec<(bool, usize, f64)> {
    [false, true]
        .into_iter()
        .flat_map(|history| {
            window
                .pieces(history)
                .iter()
                .map(move |piece| match *piece {
                    thread_window::PaintPiece::Spacer { index, height } => (history, index, height),
                    thread_window::PaintPiece::Row { index, .. } => (history, index, 0.0),
                })
        })
        .collect()
}
fn main() {
    for count in [200usize, 2000, 10000] {
        let bag = state::ChannelMessages {
            messages: (0..count)
                .map(|i| state::Message {
                    ts: Some(i.to_string()),
                })
                .collect(),
            pending: vec![(count / 3).to_string()],
        };
        let history = thread_history::ThreadHistory;
        let mut window = thread_window::ThreadWindow::default();
        window.sync("bench", &bag, &history, "0", None, true);
        let mut indices = [vec![], vec![]];
        let mut pieces = [vec![], vec![]];
        let mut sync = vec![];
        let mut viewport = vec![];
        for run in 0..21 {
            // Each comparison uses the same full-size tail, independent of the
            // viewport queries measured at the end of the previous round.
            window.measure(count.saturating_sub(100), count - 1, &[], false);
            window.sync("bench", &bag, &history, "0", None, true);
            assert_eq!(scan_indices(&window, &bag), indexed_indices(&window, &bag));
            assert_eq!(scan_pieces(&window), cached_pieces(&window));
            for path in if run % 2 == 0 { [0, 1] } else { [1, 0] } {
                let a = sample(|| {
                    black_box(if path == 0 {
                        scan_indices(&window, &bag)
                    } else {
                        indexed_indices(&window, &bag)
                    });
                });
                let b = sample(|| {
                    black_box(if path == 0 {
                        scan_pieces(&window)
                    } else {
                        cached_pieces(&window)
                    });
                });
                if run > 0 {
                    indices[path].push(a);
                    pieces[path].push(b);
                }
            }
            let c = sample(|| {
                window.sync("bench", &bag, &history, "0", None, true);
                black_box(&window);
            });
            let mut query = 0;
            let d = sample(|| {
                let first = query * 7919 % count;
                query += 1;
                window.measure(first, (first + 8).min(count - 1), &[], false);
                window.align_viewport(None, false);
                black_box(&window);
            });
            if run > 0 {
                sync.push(c);
                viewport.push(d);
            }
        }
        let (im, ip) = stats(indices[0].clone());
        let (am, ap) = stats(indices[1].clone());
        let (pm, pp) = stats(pieces[0].clone());
        let (qm, qp) = stats(pieces[1].clone());
        let (sm, sp) = stats(sync);
        let (vm, vp) = stats(viewport);
        println!(
            "rows={count} linear_index_ms={im:.6}/{ip:.6} indexed_ms={am:.6}/{ap:.6} linear_pieces_ms={pm:.6}/{pp:.6} cached_pieces_ms={qm:.6}/{qp:.6} sync_ms={sm:.6}/{sp:.6} viewport_ms={vm:.6}/{vp:.6}"
        );
    }
}
