//! Opt-in offline checks through the native WebView and real composer handlers.
use crate::state::ShellState;
use dioxus::prelude::*;

pub async fn run(mut state: Signal<ShellState>) {
    if !crate::fixture::is_fixture()
        || std::env::var_os("SUPER_PLATINUM_COMPLETION_CHECK").is_none()
    {
        return;
    }
    if state.peek().core.transport.is_some() || state.peek().core.session.is_some() {
        eprintln!("super-platinum: completion native check refused connected state");
        return;
    }
    let thread = state.peek().thread_root.clone();
    let capture_only = std::env::var_os("SUPER_PLATINUM_COMPLETION_CAPTURE_ONLY").is_some();
    if capture_only {
        state
            .write()
            .show_toast("Message is queued until Slack reconnects");
    }
    let mut check = document::eval(&format!(
        "const captureOnly = {capture_only};\n{}",
        include_str!("completion_check.js")
    ));
    let mut count = 0;
    while let Ok(receipt) = check.recv::<serde_json::Value>().await {
        match receipt["kind"].as_str() {
            Some("capture") if capture_only => {
                eprintln!(
                    "super-platinum: completion native capture passed: picker and toast do not overlap"
                );
                return;
            }
            Some("native-key") => {
                eprintln!(
                    "super-platinum: completion native key: {} {}",
                    receipt["id"],
                    receipt["action"].as_str().unwrap_or_default()
                );
            }
            Some("model") => {
                let shell = state.peek();
                let text = if receipt["field"] == "channel-composer" {
                    &shell.core.composer.text
                } else {
                    &shell.core.thread_composer.text
                };
                if receipt["text"].as_str() != Some(text.as_str()) || shell.thread_root != thread {
                    eprintln!(
                        "super-platinum: completion native check failed: model {}",
                        receipt["case"]
                    );
                    return;
                }
                count += 1;
            }
            Some("done") if count == 12 && receipt["count"] == 12 => {
                eprintln!("super-platinum: completion native check passed: {count} model receipts");
                return;
            }
            _ => {
                eprintln!(
                    "super-platinum: completion native check failed: {}",
                    receipt["case"]
                );
                return;
            }
        }
    }
    eprintln!("super-platinum: completion native check failed: evaluator ended early");
}
