//! Opt-in native event regression check; runs only in a disconnected fixture.
use crate::state::ShellState;
use dioxus::prelude::*;

pub async fn run(state: Signal<ShellState>) {
    if !crate::fixture::is_fixture() || std::env::var_os("SUPER_PLATINUM_IME_CHECK").is_none() {
        return;
    }
    if state.peek().core.transport.is_some() || state.peek().core.session.is_some() {
        eprintln!("super-platinum: IME native check refused connected state");
        return;
    }
    let thread = state.peek().thread_root.clone();
    let mut check = document::eval(include_str!("keyboard_check.js"));
    let mut guarded = 0;
    let mut sends = 0;
    while let Ok(receipt) = check.recv::<serde_json::Value>().await {
        let kind = receipt["kind"].as_str().unwrap_or_default();
        if kind == "done" {
            if guarded == 12 && sends == 2 {
                eprintln!(
                    "super-platinum: IME native check passed: {guarded} guarded keys, {sends} offline sends"
                );
            } else {
                eprintln!("super-platinum: IME native check failed: incomplete receipts");
            }
            return;
        }
        let shell = state.peek();
        let text = if receipt["field"] == "channel-composer" {
            &shell.core.composer.text
        } else {
            &shell.core.thread_composer.text
        };
        let valid = match kind {
            "guarded" => {
                guarded += 1;
                receipt["defaultAllowed"] == true
                    && receipt["bubbled"] == false
                    && receipt["text"].as_str() == Some(text.as_str())
                    && receipt["domUnchanged"] == true
                    && shell.thread_root == thread
                    && shell.overlay.is_none()
            }
            "send" => {
                sends += 1;
                receipt["bubbled"] == true && text.is_empty() && receipt["domEmpty"] == true
            }
            _ => false,
        };
        if !valid {
            // Only fixture identifiers/case labels, never real composer text.
            eprintln!(
                "super-platinum: IME native check failed: {} {}",
                receipt["field"], receipt["case"]
            );
            return;
        }
    }
    eprintln!("super-platinum: IME native check failed: evaluator ended early");
}
