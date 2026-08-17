use dioxus::prelude::{Signal, WritableExt};

use crate::state::ShellState;

pub async fn mark_painted(mut state: Signal<ShellState>) {
    let _ = dioxus::document::eval(
        "requestAnimationFrame(() => requestAnimationFrame(() => dioxus.send(true)));",
    )
    .recv::<bool>()
    .await;
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
