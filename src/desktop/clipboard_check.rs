//! Opt-in clipboard-event checks, isolated from the system clipboard and Slack.
use crate::state::ShellState;
use dioxus::prelude::*;

pub async fn run(state: Signal<ShellState>) {
    if !crate::fixture::is_fixture() || std::env::var_os("SUPER_PLATINUM_CLIPBOARD_CHECK").is_none()
    {
        return;
    }
    if state.peek().core.transport.is_some() || state.peek().core.session.is_some() {
        eprintln!("super-platinum: clipboard native check refused connected state");
        return;
    }
    let thread = state.peek().thread_root.clone();
    let mut check = document::eval(include_str!("clipboard_check.js"));
    let mut count = 0;
    while let Ok(receipt) = check.recv::<serde_json::Value>().await {
        match receipt["kind"].as_str() {
            Some("model") => {
                let shell = state.peek();
                let text = if receipt["field"] == "channel-composer" {
                    &shell.core.composer.text
                } else {
                    &shell.core.thread_composer.text
                };
                if receipt["text"].as_str() != Some(text.as_str()) || shell.thread_root != thread {
                    eprintln!(
                        "super-platinum: clipboard native check failed: model {}",
                        receipt["case"]
                    );
                    return;
                }
                count += 1;
            }
            Some("done") if count == 9 && receipt["count"] == 9 => {
                eprintln!("super-platinum: clipboard native check passed: {count} model receipts");
                return;
            }
            _ => {
                eprintln!(
                    "super-platinum: clipboard native check failed: {}",
                    receipt["case"]
                );
                return;
            }
        }
    }
    eprintln!("super-platinum: clipboard native check failed: evaluator ended early");
}
