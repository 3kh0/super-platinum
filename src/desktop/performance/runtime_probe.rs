//! Opt-in, twenty-second startup observation outside the VDOM task queue.
//!
//! A stalled edit acknowledgement prevents the pinned desktop renderer from
//! polling application tasks. Native events and this custom protocol therefore
//! must not rely on `spawn`/`document::eval` to report that condition. This probe
//! observes interpreter structure only; it neither wraps edits nor activates a
//! window, and cannot establish that an acknowledgement was actually delivered.

use std::borrow::Cow;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use dioxus::desktop::Config;
use dioxus::desktop::tao::event::{Event, WindowEvent};
use dioxus::desktop::wry::http::{Method, Request, Response};
use serde::{Deserialize, Serialize};

const LIMIT: Duration = Duration::from_secs(20);
const MAX_SAMPLES: u64 = 16;
static ENABLED: OnceLock<bool> = OnceLock::new();
static PROBE: OnceLock<Probe> = OnceLock::new();

struct Probe {
    started: Instant,
    native_events: AtomicU64,
    user_events: AtomicU64,
    window_events: AtomicU64,
    focus: AtomicU64,
    heartbeats: AtomicU64,
    last_heartbeat: AtomicU64,
    js_samples: AtomicU64,
}

impl Probe {
    fn active(&self) -> bool {
        self.started.elapsed() < LIMIT
    }

    fn elapsed_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    fn snapshot(&self, phase: &'static str, js: Option<JsSample>) {
        let now = self.elapsed_ms();
        let last = self.last_heartbeat.load(Ordering::Relaxed);
        let focus = match self.focus.load(Ordering::Relaxed) {
            1 => Some(false),
            2 => Some(true),
            _ => None,
        };
        // Every string in this record is a fixed enum/literal. No paths, URLs,
        // window titles, request contents, DOM text, or IPC payloads are logged.
        let record = serde_json::json!({
            "phase": phase,
            "elapsed_ms": now,
            "native_events": self.native_events.load(Ordering::Relaxed),
            "user_events": self.user_events.load(Ordering::Relaxed),
            "window_events": self.window_events.load(Ordering::Relaxed),
            "native_focused": focus,
            "heartbeats": self.heartbeats.load(Ordering::Relaxed),
            "heartbeat_age_ms": (last != 0).then(|| now.saturating_sub(last - 1)),
            "js_samples": self.js_samples.load(Ordering::Relaxed),
            "js": js,
        });
        eprintln!("super-platinum runtime-probe: {record}");
    }
}

pub(crate) fn enabled() -> bool {
    *ENABLED.get_or_init(|| std::env::var("SUPER_PLATINUM_RUNTIME_PROBE").as_deref() == Ok("1"))
}

/// Called once while building the initial document, before any app task runs.
pub(crate) fn head(csp: &str) -> String {
    if !enabled() {
        return format!(r#"<meta http-equiv="Content-Security-Policy" content="{csp}">"#);
    }
    let csp = csp.replace(
        "connect-src ",
        "connect-src super-platinum-probe: http://super-platinum-probe.runtime ",
    );
    let script = include_str!("runtime_probe.js");
    format!(
        r#"<meta http-equiv="Content-Security-Policy" content="{csp}"><script>{script}</script>"#
    )
}

/// Install on the app's single config. No existing custom native-event or
/// on-window handler is installed there; framework event dispatch is unchanged.
pub(crate) fn configure(config: Config) -> Config {
    if !enabled() {
        return config;
    }
    let probe = PROBE.get_or_init(|| Probe {
        started: Instant::now(),
        native_events: AtomicU64::new(0),
        user_events: AtomicU64::new(0),
        window_events: AtomicU64::new(0),
        focus: AtomicU64::new(0),
        heartbeats: AtomicU64::new(0),
        last_heartbeat: AtomicU64::new(0),
        js_samples: AtomicU64::new(0),
    });
    static WATCHDOG: OnceLock<()> = OnceLock::new();
    WATCHDOG.get_or_init(|| {
        probe.snapshot("configured", None);
        // No window APIs on this thread. It terminates after the final sample,
        // even if neither the event loop nor the JS engine makes progress.
        let _ = std::thread::Builder::new()
            .name("runtime-probe".into())
            .spawn(move || {
                for seconds in [1, 2, 5, 10, 20] {
                    let deadline = probe.started + Duration::from_secs(seconds);
                    std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
                    probe.snapshot(
                        if seconds == 20 {
                            "finished"
                        } else {
                            "watchdog"
                        },
                        None,
                    );
                }
            });
    });
    config
        .with_on_window(move |window, _| {
            if probe.active() {
                probe
                    .focus
                    .store(if window.is_focused() { 2 } else { 1 }, Ordering::Relaxed);
                probe.snapshot(
                    if window.is_visible() {
                        "window_visible"
                    } else {
                        "window_hidden"
                    },
                    None,
                );
            }
        })
        .with_custom_event_handler(move |event, _| {
            if !probe.active() {
                return;
            }
            probe.native_events.fetch_add(1, Ordering::Relaxed);
            match event {
                Event::UserEvent(_) => {
                    probe.user_events.fetch_add(1, Ordering::Relaxed);
                }
                Event::WindowEvent { event, .. } => {
                    probe.window_events.fetch_add(1, Ordering::Relaxed);
                    if let WindowEvent::Focused(focused) = event {
                        probe
                            .focus
                            .store(if *focused { 2 } else { 1 }, Ordering::Relaxed);
                    }
                }
                _ => {}
            }
        })
        .with_custom_protocol("super-platinum-probe", move |_, request| {
            receive(probe, request)
        })
}

#[derive(Clone, Copy)]
pub(crate) struct Heartbeat(&'static Probe);

pub(crate) fn heartbeat() -> Option<Heartbeat> {
    PROBE.get().filter(|probe| probe.active()).map(Heartbeat)
}

impl Heartbeat {
    /// The caller drops the handle after expiry, avoiding even atomic work on
    /// subsequent ordinary runtime ticks.
    pub(crate) fn beat(self) -> bool {
        if !self.0.active() {
            return false;
        }
        self.0.heartbeats.fetch_add(1, Ordering::Relaxed);
        self.0
            .last_heartbeat
            .store(self.0.elapsed_ms() + 1, Ordering::Relaxed);
        true
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Initial,
    DomReady,
    Loaded,
    Timer,
    Visibility,
    Finished,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum Ready {
    Loading,
    Interactive,
    Complete,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum Visibility {
    Hidden,
    Visible,
    Prerender,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct JsSample {
    phase: Phase,
    elapsed_ms: u32,
    ready: Ready,
    visibility: Visibility,
    focused: bool,
    interpreter: bool,
    headless: Option<bool>,
    socket_state: Option<u8>,
    queued_batches: Option<u32>,
    socket_observer_generation: Option<u32>,
    socket_observer_since_ms: Option<u32>,
    socket_messages: Option<u32>,
    socket_opens: Option<u32>,
    socket_closes: Option<u32>,
    socket_errors: Option<u32>,
    socket_buffered_bytes: Option<u32>,
    interpreter_nodes: Option<u32>,
    interpreter_stack: Option<u32>,
    root_children: u32,
    raf_callbacks: u8,
    script_errors: u32,
    resource_errors: u32,
    unhandled_rejections: u32,
}

fn decode(body: &[u8]) -> Option<JsSample> {
    if body.len() > 1024 {
        return None;
    }
    let sample: JsSample = serde_json::from_slice(body).ok()?;
    (sample.elapsed_ms <= 20_000
        && sample.socket_state.is_none_or(|state| state <= 3)
        && sample.queued_batches.is_none_or(|count| count <= 1_000_000)
        && sample
            .socket_observer_generation
            .is_none_or(|count| (1..=16).contains(&count))
        && sample
            .socket_observer_since_ms
            .is_none_or(|time| time <= sample.elapsed_ms)
        && [
            sample.socket_messages,
            sample.socket_opens,
            sample.socket_closes,
            sample.socket_errors,
            sample.socket_buffered_bytes,
            sample.interpreter_nodes,
            sample.interpreter_stack,
        ]
        .into_iter()
        .all(|count| count.is_none_or(|count| count <= 1_000_000))
        && sample.root_children <= 1_000_000
        && sample.raf_callbacks <= 2
        && sample.script_errors <= 1_000_000
        && sample.resource_errors <= 1_000_000
        && sample.unhandled_rejections <= 1_000_000)
        .then_some(sample)
}

fn receive(probe: &Probe, request: Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
    if probe.active() && request.method() == Method::POST && request.uri().path() == "/sample" {
        if let Some(sample) = decode(request.body()) {
            if probe
                .js_samples
                .try_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
                    (count < MAX_SAMPLES).then_some(count + 1)
                })
                .is_ok()
            {
                probe.snapshot("javascript", Some(sample));
            }
        }
    }
    Response::builder()
        .status(204)
        .header("Access-Control-Allow-Origin", "*")
        .header("Access-Control-Allow-Methods", "POST, OPTIONS")
        .header("Access-Control-Allow-Headers", "Content-Type")
        .header("Cache-Control", "no-store")
        .body(Cow::Borrowed(&[][..]))
        .expect("static probe response")
}

#[cfg(test)]
mod tests;
