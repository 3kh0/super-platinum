//! Explicit disconnected native experiment. No ordinary-session clocks or ledger.
use crate::state::ShellState;
use serde_json::{Value, json};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;
#[path = "emoji_lifecycle/check.rs"]
mod check;
#[path = "emoji_lifecycle/corpus.rs"]
mod corpus;
pub(crate) use check::start;
pub const ACCOUNT: &str = "emoji-lifecycle-disconnected";
pub const STAGES: &[&str] = &["initial", "cold-ready", "refresh-ready", "retired"];
pub fn fixture(name: &str) -> Option<&'static str> {
    match name.strip_suffix("-narrow").unwrap_or(name) {
        "emoji-lifecycle-colon-64000" => Some("colon"),
        "emoji-lifecycle-picker-64000" => Some("picker"),
        _ => None,
    }
}
pub fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("SUPER_PLATINUM_EMOJI_LIFECYCLE").as_deref() == Ok("1")
            && std::env::var("SUPER_PLATINUM_FIXTURE")
                .ok()
                .as_deref()
                .and_then(fixture)
                .is_some()
    })
}
#[derive(Default)]
struct Ledger {
    events: Vec<Value>,
    overflow: bool,
    phase: &'static str,
    inventory: Value,
}
static LEDGER: OnceLock<Mutex<Ledger>> = OnceLock::new();
pub fn begin() -> Option<Instant> {
    enabled().then(Instant::now)
}
pub fn record(kind: &str, value: Value) {
    if !enabled() {
        return;
    }
    let mut ledger = LEDGER.get().expect("fixture installed").lock().unwrap();
    if ledger.events.len() == 256 {
        ledger.overflow = true;
        return;
    }
    let seq = ledger.events.len();
    let phase = ledger.phase;
    ledger
        .events
        .push(json!({"seq":seq,"phase":phase,"kind":kind,"value":value}));
}
pub fn finish(at: Option<Instant>, kind: &str, value: impl FnOnce() -> Value) {
    if let Some(at) = at {
        // End timestamp precedes JSON/allocation/lock overhead.
        let milliseconds = at.elapsed().as_secs_f64() * 1000.;
        let mut value = value();
        value["milliseconds"] = json!(milliseconds);
        record(kind, value);
    }
}
fn phase(name: &'static str) {
    LEDGER.get().unwrap().lock().unwrap().phase = name;
}
fn evidence() -> Value {
    let ledger = LEDGER.get().unwrap().lock().unwrap();
    json!({"events":ledger.events,"overflow":ledger.overflow,"inventory":ledger.inventory})
}
pub(crate) fn install(shell: &mut ShellState) {
    assert!(shell.core.session.is_none() && shell.core.transport.is_none());
    shell.core.cache = None;
    shell.core.active_account = Some(ACCOUNT.into());
    shell.overlay = None;
    shell.toast = None;
    shell.close_thread();
    shell.close_profile();
    shell.core.threads.clear();
    shell.pending_delivery.clear();
    shell.core.composer = Default::default();
    shell.checkpoint_composers();
    let workspace = shell.core.workspaces.get_mut("T1").unwrap();
    workspace.emoji_preferences.replace(&Default::default());
    // from_environment runs on the existing bootstrap blocking worker. This
    // allocates canonical fixture data, never a Catalog or captured snapshot.
    workspace.custom_emoji = corpus::corpus(64_000, 0);
    workspace.custom_emoji_revision = 1;
    let inventory = corpus::inventory(&workspace.custom_emoji);
    assert!(
        LEDGER
            .set(Mutex::new(Ledger {
                inventory,
                phase: "setup",
                ..Default::default()
            }))
            .is_ok()
    );
}
pub fn instrument(source: &str) -> Result<String, &'static str> {
    let anchors = [
        (
            "dioxus.send({kind:'query', seq, text:field.value, anchor:field.selectionStart, cursor:field.selectionEnd, direction:field.selectionDirection});",
            "window.__spEmojiLifecycle?.query(field,seq,completionOwner,snapshot());\n    ",
        ),
        (
            "if (!result) break;",
            "window.__spEmojiLifecycle?.receive(field,result,completionOwner,seq,snapshot());\n      ",
        ),
        (
            "emojiGate.dismiss(); hide(); source = ''; query(); continue;",
            "window.__spEmojiLifecycle?.invalidate(field,result,completionOwner);\n        ",
        ),
    ];
    let accepted = "popup.hidden = false; field.setAttribute('aria-expanded','true'); highlight();";
    if anchors.iter().any(|(a, _)| source.matches(a).count() != 1)
        || source.matches(accepted).count() != 1
    {
        return Err("Emoji cold controller anchors changed");
    }
    let mut result = source.to_owned();
    for (anchor, prefix) in anchors {
        result = result.replace(anchor, &format!("{prefix}{anchor}"));
    }
    Ok(result.replace(accepted,&format!("{accepted}\n      window.__spEmojiLifecycle?.accepted(field,result,completionOwner,()=>result.seq===seq && source===snapshot() && document.activeElement===field && !composing);")))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_gate_names_and_anchors() {
        for name in [
            "emoji-lifecycle-colon-64000",
            "emoji-lifecycle-picker-64000-narrow",
        ] {
            assert!(fixture(name).is_some());
        }
        for name in [
            "emoji-native",
            "emoji-lifecycle-colon-64001",
            "emoji-lifecycle-picker-64000-narrow-narrow",
        ] {
            assert!(fixture(name).is_none());
        }
        let source = include_str!("../composer_completion.js");
        assert!(
            instrument(source)
                .unwrap()
                .contains("__spEmojiLifecycle?.accepted")
        );
        assert!(instrument("").is_err());
        assert!(instrument(&format!("{source}{source}")).is_err());
    }
}
