use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use crate::state::ShellState;

pub(crate) mod completion;
pub(crate) mod emoji_lifecycle;
pub(crate) mod emoji_retirement;
pub(crate) mod runtime_probe;

#[derive(Clone, Copy)]
pub(crate) enum StartupStage {
    FirstPaint,
    CacheLoaded,
    ConversationPaint,
    HistoryPaint,
    RealtimeConnected,
}

impl StartupStage {
    fn key(self) -> &'static str {
        match self {
            Self::FirstPaint => "first_paint_ms",
            Self::CacheLoaded => "cache_loaded_ms",
            Self::ConversationPaint => "conversation_paint_ms",
            Self::HistoryPaint => "history_paint_ms",
            Self::RealtimeConnected => "realtime_connected_ms",
        }
    }
}

static STARTED: OnceLock<Instant> = OnceLock::new();
static STARTUP: Mutex<std::collections::BTreeMap<&'static str, f64>> =
    Mutex::new(std::collections::BTreeMap::new());

pub(crate) fn start() {
    STARTED.get_or_init(Instant::now);
}

pub(crate) fn record(stage: StartupStage) {
    let Some(started) = STARTED.get() else { return };
    let mut metrics = STARTUP.lock().unwrap();
    if metrics.contains_key(stage.key()) {
        return;
    }
    let elapsed = started.elapsed().as_secs_f64() * 1000.0;
    metrics.insert(stage.key(), elapsed);
    if std::env::var_os("SUPER_PLATINUM_STARTUP_TRACE").is_some() {
        eprintln!("super-platinum startup: {}={elapsed:.1}", stage.key());
    }
}

pub(crate) fn startup_snapshot() -> serde_json::Value {
    serde_json::to_value(&*STARTUP.lock().unwrap()).unwrap_or_default()
}

pub(crate) async fn mark_history_painted(
    state: Signal<ShellState>,
    team: String,
    channel: String,
    generation: u64,
) {
    let matches = || {
        let shell = state.read();
        shell.core.active_team.as_ref() == Some(&team)
            && shell.core.active_channel.as_ref() == Some(&channel)
            && shell.channel_generation == generation
    };
    if !matches() {
        return;
    }
    if dioxus::document::eval(
        "requestAnimationFrame(() => requestAnimationFrame(() => dioxus.send(true)));",
    )
    .recv::<bool>()
    .await
    .is_ok()
        && matches()
    {
        record(StartupStage::HistoryPaint);
    }
}

pub async fn mark_painted(state: Signal<ShellState>) {
    // Capture readiness before waiting: loading content after this evaluation
    // starts must get its own paint acknowledgement.
    let ready = {
        let shell = state.read();
        shell.signed_in && !shell.loading && !shell.channels.is_empty()
    };
    if dioxus::document::eval(
        "requestAnimationFrame(() => requestAnimationFrame(() => dioxus.send(true)));",
    )
    .recv::<bool>()
    .await
    .is_err()
    {
        return;
    }
    record(StartupStage::FirstPaint);
    if ready {
        record(StartupStage::ConversationPaint);
    }
    record_paint_latency(state, Instant::now());
}

/// Timing acknowledgements with no pending sample must not invalidate the UI.
/// Real samples retain the ordinary notifying write and existing take-once rules.
fn record_paint_latency(mut state: Signal<ShellState>, now: Instant) {
    let pending = {
        let shell = state.peek();
        shell.channel_switch_started.is_some() || shell.realtime_insert_started.is_some()
    };
    if !pending {
        return;
    }
    let mut shell = state.write();
    if let Some(started) = shell.channel_switch_started.take() {
        shell.performance.channel_switch_ms =
            Some(now.duration_since(started).as_secs_f64() * 1000.0);
    }
    if let Some(started) = shell.realtime_insert_started.take() {
        shell.performance.realtime_insert_ms =
            Some(now.duration_since(started).as_secs_f64() * 1000.0);
    }
}

pub async fn watch_scroll(state: Signal<ShellState>) {
    let auto_scroll = std::env::var_os("SUPER_PLATINUM_PERF_SCROLL").is_some();
    let script = format!(
        "const autoScroll = {auto_scroll};\n{}",
        include_str!("scroll_performance.js")
    );
    let mut bridge = dioxus::document::eval(&script);
    while let Ok(samples) = bridge.recv::<ScrollBatch>().await {
        // Metrics are read on demand by the agent. Updating them must not
        // rerender the conversation while the user is actively scrolling.
        #[allow(deprecated)]
        let mut shell = state.write_silent();
        match samples.surface {
            ScrollSurface::Channel => {
                retain_samples(&mut shell.performance.scroll_frame_ms, samples.frames);
                retain_samples(
                    &mut shell.performance.scroll_start_delay_ms,
                    samples.start_delays,
                );
            }
            ScrollSurface::Thread => {
                retain_samples(
                    &mut shell.performance.thread_scroll_frame_ms,
                    samples.frames,
                );
                retain_samples(
                    &mut shell.performance.thread_scroll_start_delay_ms,
                    samples.start_delays,
                );
            }
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum ScrollSurface {
    Channel,
    Thread,
}

#[derive(serde::Deserialize)]
struct ScrollBatch {
    surface: ScrollSurface,
    frames: Vec<f64>,
    start_delays: Vec<f64>,
}

fn retain_samples(target: &mut Vec<f64>, samples: impl IntoIterator<Item = f64>) {
    target.extend(
        samples
            .into_iter()
            .filter(|sample| sample.is_finite() && *sample >= 0.0),
    );
    if target.len() > 600 {
        target.drain(..target.len() - 600);
    }
}

/// Statistics describe the retained sample window, not lifetime totals.
/// p95 uses nearest rank; an empty window has no percentile or maximum.
pub(crate) fn sample_summary(samples: &[f64]) -> serde_json::Value {
    let mut sorted = samples
        .iter()
        .copied()
        .filter(|sample| sample.is_finite() && *sample >= 0.0)
        .collect::<Vec<_>>();
    sorted.sort_by(f64::total_cmp);
    let count = sorted.len();
    let median = (!sorted.is_empty()).then(|| {
        if count % 2 == 0 {
            (sorted[count / 2 - 1] + sorted[count / 2]) / 2.0
        } else {
            sorted[count / 2]
        }
    });
    serde_json::json!({
        "samples":count,
        "p50_ms":median,
        "p95_ms":(!sorted.is_empty()).then(|| sorted[(95 * count).div_ceil(100) - 1]),
        "max_ms":sorted.last(),
        "over_50_ms":sorted.iter().filter(|sample| **sample > 50.0).count(),
        "at_least_250_ms":sorted.iter().filter(|sample| **sample >= 250.0).count(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn retention_keeps_visible_stalls_and_only_bounds_sample_count() {
        let mut samples = vec![16.0; 600];
        retain_samples(&mut samples, [250.0, 1800.0, f64::NAN, -1.0, f64::INFINITY]);
        assert_eq!(samples.len(), 600);
        assert_eq!(&samples[598..], &[250.0, 1800.0]);
        assert_eq!(sample_summary(&samples)["at_least_250_ms"], 2);
    }

    #[test]
    fn summary_distinguishes_empty_samples_and_reports_stall_thresholds() {
        assert_eq!(
            sample_summary(&[]),
            json!({"samples":0,"p50_ms":null,"p95_ms":null,"max_ms":null,"over_50_ms":0,"at_least_250_ms":0})
        );
        assert_eq!(
            sample_summary(&[16.0, 50.0, 250.0, 1000.0]),
            json!({"samples":4,"p50_ms":150.0,"p95_ms":1000.0,"max_ms":1000.0,"over_50_ms":2,"at_least_250_ms":2})
        );
    }
}

#[cfg(test)]
mod paint_tests;
