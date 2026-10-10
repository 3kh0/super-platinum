use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use crate::state::ShellState;

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

pub async fn mark_painted(mut state: Signal<ShellState>) {
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
    let now = std::time::Instant::now();
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

pub async fn watch_scroll(mut state: Signal<ShellState>) {
    let auto_scroll = std::env::var_os("SUPER_PLATINUM_PERF_SCROLL").is_some();
    let script = format!(
        r#"
      let activeUntil = 0, previous = 0, samples = [];
      document.addEventListener('scroll', event => {{ if (event.target?.id === 'message-timeline') activeUntil = performance.now() + 180; }}, true);
      function frame(now) {{
        if (now < activeUntil && previous) samples.push(now - previous);
        previous = now;
        if (samples.length >= 10) {{ dioxus.send(samples.splice(0)); }}
        requestAnimationFrame(frame);
      }}
      requestAnimationFrame(frame);
      if ({auto_scroll}) setTimeout(() => {{ const t=document.getElementById('message-timeline'); if (!t) return; t.scrollTo({{top:0,behavior:'smooth'}}); setTimeout(() => t.scrollTo({{top:t.scrollHeight,behavior:'smooth'}}),700); }}, 600);
    "#
    );
    let mut bridge = dioxus::document::eval(&script);
    while let Ok(samples) = bridge.recv::<Vec<f64>>().await {
        let mut shell = state.write();
        shell.performance.scroll_frame_ms.extend(
            samples
                .into_iter()
                .filter(|sample| sample.is_finite() && *sample < 250.0),
        );
        if shell.performance.scroll_frame_ms.len() > 600 {
            let drain = shell.performance.scroll_frame_ms.len() - 600;
            shell.performance.scroll_frame_ms.drain(..drain);
        }
    }
}
