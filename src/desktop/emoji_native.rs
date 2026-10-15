//! Named disconnected native acceptance only. No production executor or bypass.
use crate::state::ShellState;
use serde_json::{Value, json};
use std::sync::{Mutex, OnceLock};
#[path = "emoji_native/check.rs"]
mod check;
pub(crate) use check::start;
pub const ACCOUNT: &str = "emoji-native-disconnected";
pub const BASE: &str = "\u{1f98a} replace end";
pub const NEWER: &str = "Newer \u{65e5}\u{672c}\u{8a9e} \u{1f680} e\u{301}";
pub const STAGES: &[&str] = &[
    "initial",
    "completion-offered",
    "completion-standard",
    "custom-offered",
    "completion-custom",
    "picker-selected",
    "picker-cancelled",
    "picker-unready",
    "picker-ready",
    "picker-inserted",
    "guard-newer",
    "guard-catalog",
    "guard-account",
    "guard-mount",
    "guard-navigation",
    "guard-composition",
];
#[derive(Default)]
struct Probe {
    stage: String,
    events: Vec<Value>,
    hold: Option<String>,
    reached: bool,
    preferences: bool,
}
static PROBE: OnceLock<Mutex<Probe>> = OnceLock::new();
pub fn enabled() -> bool {
    std::env::var("SUPER_PLATINUM_EMOJI_NATIVE_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("emoji-native" | "emoji-native-narrow")
        )
        && PROBE.get().is_some()
}
pub fn observe(kind: &str, value: Value) {
    if !enabled() {
        return;
    }
    let mut p = PROBE.get().unwrap().lock().unwrap();
    let sequence = p.events.len();
    let stage = p.stage.clone();
    p.events
        .push(json!({"sequence":sequence,"stage":stage,"kind":kind,"value":value}));
}
pub async fn boundary(kind: &str, value: Value) {
    if !enabled() {
        return;
    }
    observe(kind, value);
    for _ in 0..1500 {
        let waiting = {
            let mut p = PROBE.get().unwrap().lock().unwrap();
            if p.hold.as_deref() == Some(kind) {
                p.reached = true;
                true
            } else {
                false
            }
        };
        if !waiting {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    // A failed check must never freeze production or manufacture admission.
    observe("hold-timeout", json!({"kind":kind}));
}
pub fn preferences_held() -> bool {
    enabled() && PROBE.get().unwrap().lock().unwrap().preferences
}
fn stage(name: &str) {
    PROBE.get().unwrap().lock().unwrap().stage = name.into();
}
fn arm(kind: &str) {
    let mut p = PROBE.get().unwrap().lock().unwrap();
    p.hold = Some(kind.into());
    p.reached = false;
}
fn release() {
    PROBE.get().unwrap().lock().unwrap().hold = None;
}
fn events() -> Vec<Value> {
    PROBE.get().unwrap().lock().unwrap().events.clone()
}
pub fn install(shell: &mut ShellState) {
    // Alias installation alone never activates runtime observations or holds.
    shell.core.session = None;
    shell.core.transport = None;
    shell.core.cache = None;
    shell.core.active_account = Some(ACCOUNT.into());
    shell.overlay = None;
    shell.toast = None;
    shell.close_thread();
    shell.close_profile();
    shell.core.threads.clear();
    shell.pending_delivery.clear();
    let workspace = shell.core.workspaces.get_mut("T1").unwrap();
    workspace.emoji_preferences.replace(&Default::default());
    workspace.custom_emoji.insert(
        "party_fixture".into(),
        super_platinum_core::slack::models::Emoji {
            name: "party_fixture".into(),
            value: "alias:rocket".into(),
            ..Default::default()
        },
    );
    workspace.custom_emoji_revision += 1;
    shell.core.composer.text = BASE.into();
    shell.checkpoint_composers();
    let _ = PROBE.set(Mutex::new(Probe::default()));
}
