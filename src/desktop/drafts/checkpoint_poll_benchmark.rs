//! Paired component measurement, not native idle CPU, memory, or frame evidence.
//! Run alone: cargo test --locked paired_changed_snapshot_poll -- --ignored --nocapture --test-threads=1
use super::*;
use std::{hint::black_box, time::Instant};

fn elapsed_per_poll(
    queue: &Queue,
    previous: &BTreeMap<DraftKey, Signature>,
    iterations: usize,
    use_optimized: bool,
) -> f64 {
    let started = Instant::now();
    for _ in 0..iterations {
        black_box(if use_optimized {
            optimized(black_box(queue), black_box(previous))
        } else {
            reference(black_box(queue), black_box(previous))
        });
    }
    started.elapsed().as_secs_f64() * 1_000_000.0 / iterations as f64
}

fn summary(values: &[f64]) -> serde_json::Value {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    serde_json::json!({
        "samples_us_per_poll":values,
        "median_us_per_poll":(sorted[3] + sorted[4]) / 2.0,
        "min_us_per_poll":sorted[0], "max_us_per_poll":sorted[7],
    })
}

#[tokio::test]
#[ignore = "manual paired retained-draft poll CPU measurement; run alone with --nocapture"]
async fn paired_changed_snapshot_poll() {
    for (editors, text_bytes, iterations) in [(1, 256, 4096), (64, 32768, 128)] {
        for mode in [
            "unchanged",
            "one_changed",
            "first_observation",
            "released_idle",
        ] {
            let queue = fixture(editors, text_bytes);
            let mut previous = signatures(&queue);
            let expected = match mode {
                "unchanged" => 0,
                "one_changed" => {
                    queue
                        .editors
                        .values()
                        .next()
                        .unwrap()
                        .state
                        .lock()
                        .unwrap()
                        .serial += 1;
                    1
                }
                "first_observation" => {
                    previous.clear();
                    editors
                }
                "released_idle" => {
                    for editor in queue.editors.values() {
                        editor.state.lock().unwrap().base.stored = None;
                    }
                    previous = signatures(&queue);
                    editors
                }
                _ => unreachable!(),
            };
            // Setup, content equivalence, and JSON encoding are outside timings.
            // Both paths observe the same stable states; no writer is running.
            assert_outcomes(&queue, &previous, expected);
            let mut before = Vec::new();
            let mut after = Vec::new();
            for sample in 0..9 {
                let mut pair = [0.0; 2];
                for index in if sample % 2 == 0 { [0, 1] } else { [1, 0] } {
                    pair[index] = elapsed_per_poll(&queue, &previous, iterations, index == 1);
                }
                if sample > 0 {
                    before.push(pair[0]);
                    after.push(pair[1]);
                }
            }
            assert_outcomes(&queue, &previous, expected);
            println!(
                "draft-poll-benchmark: {}",
                serde_json::json!({
                    "scope":"in-memory retained-editor queue; no persistence, native window, or network",
                    "mode":mode, "editors":editors, "local_text_bytes_each":text_bytes,
                    "iterations_per_sample":iterations, "emitted_snapshots_per_poll":expected,
                    "equivalent_outcomes":true, "excluded_warmup_pairs":1,
                    "alternating_measured_pairs":8,
                    "clone_then_signature":summary(&before),
                    "signature_then_clone":summary(&after),
                })
            );
        }
    }
}
