//! Paired CPU probe of the former renderer scans and the production cache.
//! This does not measure WebView frames; native receipts remain a separate gate.
use super::*;
use std::hint::black_box;
use std::time::Instant;

fn linear_spacers(shell: &ShellState, start: usize, end: usize) -> (f64, f64) {
    (
        shell.channel_rows[..start]
            .iter()
            .map(|row| shell.estimated_row_height(&row.id))
            .sum(),
        shell.channel_rows[end..]
            .iter()
            .map(|row| shell.estimated_row_height(&row.id))
            .sum(),
    )
}

fn linear_jump(shell: &ShellState, top: f64, height: f64) -> (usize, usize) {
    let bottom = top + height;
    let mut first = None;
    let mut offset = 0.0;
    for (index, row) in shell.channel_rows.iter().enumerate() {
        offset += shell.estimated_row_height(&row.id);
        if first.is_none() && offset > top {
            first = Some(index);
        }
        if offset >= bottom {
            return (first.unwrap_or(index), index);
        }
    }
    let last = shell.channel_rows.len().saturating_sub(1);
    (first.unwrap_or(last), last)
}

fn elapsed(mut action: impl FnMut()) -> f64 {
    let start = Instant::now();
    action();
    start.elapsed().as_secs_f64() * 1000.0
}

fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    (sorted[3] + sorted[4]) / 2.0
}

#[test]
#[ignore = "manual paired geometry CPU measurement; run alone with --nocapture"]
fn paired_reads_jumps_and_rebuild() {
    for count in [200, 2000, 10000] {
        let mut shell = shell(count);
        let measurements = measurements(&shell)
            .into_iter()
            .take(300)
            .collect::<Vec<_>>();
        shell.set_timeline_window(count.saturating_sub(20), count - 1, measurements, false);
        let total = heights(&shell).iter().sum::<f64>();
        let queries = (0..128)
            .map(|index| {
                let first = index * 7919 % count;
                (
                    first,
                    (first + 100).min(count),
                    total * index as f64 / 128.0,
                    [0.0, 132.0, 700.0][index % 3],
                )
            })
            .collect::<Vec<_>>();
        for &(start, end, top, height) in &queries {
            assert_eq!(
                shell.channel_geometry.spacer_heights(start, end),
                linear_spacers(&shell, start, end)
            );
            assert_eq!(
                shell.estimated_timeline_window(top, height),
                linear_jump(&shell, top, height)
            );
        }
        let mut reads_before = vec![];
        let mut reads_after = vec![];
        let mut jumps_before = vec![];
        let mut jumps_after = vec![];
        // Warm both paths once, then alternate order within each process.
        for sample in 0..9 {
            let mut read_pair = [0.0; 2];
            let mut jump_pair = [0.0; 2];
            for path in if sample % 2 == 0 { [0, 1] } else { [1, 0] } {
                read_pair[path] = elapsed(|| {
                    for _ in 0..4 {
                        for &(start, end, _, _) in &queries {
                            black_box(if path == 0 {
                                linear_spacers(&shell, black_box(start), black_box(end))
                            } else {
                                shell
                                    .channel_geometry
                                    .spacer_heights(black_box(start), black_box(end))
                            });
                        }
                    }
                });
                jump_pair[path] = elapsed(|| {
                    for _ in 0..4 {
                        for &(_, _, top, height) in &queries {
                            black_box(if path == 0 {
                                linear_jump(&shell, black_box(top), black_box(height))
                            } else {
                                shell.estimated_timeline_window(black_box(top), black_box(height))
                            });
                        }
                    }
                });
            }
            if sample > 0 {
                reads_before.push(read_pair[0]);
                reads_after.push(read_pair[1]);
                jumps_before.push(jump_pair[0]);
                jumps_after.push(jump_pair[1]);
            }
        }
        let mut rebuilds = vec![];
        for sample in 0..9 {
            let timing = elapsed(|| {
                for _ in 0..16 {
                    shell.rebuild_channel_geometry();
                    black_box(&shell.channel_geometry);
                }
            }) / 16.0;
            if sample > 0 {
                rebuilds.push(timing);
            }
        }
        // Include production measurement acceptance and cache rebuild, while
        // pinning the mounted window to exclude projection and eviction work.
        // Construct owned batches before timing; alternate so every call changes
        // all 100 rows, including the first call of every sample.
        shell.core.pending_scroll_to = None;
        shell.selection_pinned = true;
        let batch_a = shell.channel_rows[..100]
            .iter()
            .enumerate()
            .map(|(index, row)| (row.id.clone(), 40.25 + index as f64))
            .collect::<Vec<_>>();
        let batch_b = batch_a
            .iter()
            .map(|(id, height)| (id.clone(), height + 12.5))
            .collect::<Vec<_>>();
        shell.set_timeline_window(0, 99, batch_a.clone(), false);
        let mut changed_batches = vec![];
        for sample in 0..9 {
            let mut batches = (0..16)
                .map(|index| if index % 2 == 0 { &batch_b } else { &batch_a }.clone())
                .collect::<Vec<_>>();
            let timing = elapsed(|| {
                for batch in batches.drain(..) {
                    shell.set_timeline_window(0, 99, batch, false);
                    black_box(&shell.channel_geometry);
                }
            }) / 16.0;
            if sample > 0 {
                changed_batches.push(timing);
            }
        }
        println!(
            "timeline_geometry_cpu {}",
            serde_json::json!({
                "rows":count,"queries_per_sample":512,"samples":8,
                "spacer_linear_median_ms":median(&reads_before),"spacer_cached_median_ms":median(&reads_after),
                "jump_linear_median_ms":median(&jumps_before),"jump_cached_median_ms":median(&jumps_after),
                "rebuild_median_ms":median(&rebuilds),
                "changed_measurement_batch_scope":"set_timeline_window: acceptance + rebuild; selection pinned; excludes batch construction, projection, eviction and WebView",
                "changed_rows_per_batch":100,"changed_batches_per_sample":16,
                "changed_measurement_batch_median_ms":median(&changed_batches),
                "changed_measurement_batch_ms":changed_batches,
                "spacer_linear_ms":reads_before,"spacer_cached_ms":reads_after,
                "jump_linear_ms":jumps_before,"jump_cached_ms":jumps_after,"rebuild_ms":rebuilds,
            })
        );
    }
}
