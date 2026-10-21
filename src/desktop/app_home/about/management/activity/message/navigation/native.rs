//! Keep renderer custody through the parent's synchronous final owner check.
use crate::app_home::inputs::native::{Release, retain};
use serde_json::Value;
use std::time::Duration;

pub(super) async fn pending(expected: &Value) -> bool {
    let mut eval = dioxus::document::eval(&format!(
        "{}\n{}\ndioxus.send(workflowActivityChannelPending({expected}));await dioxus.recv();",
        include_str!("../../date_witness.js"),
        include_str!("native.js"),
    ));
    let accepted = matches!(
        tokio::time::timeout(Duration::from_secs(3), eval.recv::<bool>()).await,
        Ok(Ok(true))
    );
    let _ = eval.send(());
    accepted
}

pub(super) fn settled(expected: &Value) {
    // Cleanup deliberately does not need the old Rust Source to stay current.
    // JS compares the retained original native owner and pending phase only.
    let mut eval = dioxus::document::eval(&format!(
        "{}\n{}\ndioxus.send(workflowActivityChannelSettled({expected}));await dioxus.recv();",
        include_str!("../../date_witness.js"),
        include_str!("native.js"),
    ));
    dioxus::core::spawn_forever(async move {
        let _ = tokio::time::timeout(Duration::from_secs(3), eval.recv::<bool>()).await;
        let _ = eval.send(());
    });
}

pub(in crate::app_home::about::management::activity::message) fn feedback(
    expected: &Value,
    message: &str,
) {
    let mut eval = dioxus::document::eval(&format!(
        "{}\n{}\ndioxus.send(workflowActivityChannelFeedback({expected},{}));await dioxus.recv();",
        include_str!("../../date_witness.js"),
        include_str!("native.js"),
        serde_json::json!(message)
    ));
    dioxus::core::spawn_forever(async move {
        let _ = tokio::time::timeout(Duration::from_secs(3), eval.recv::<bool>()).await;
        let _ = eval.send(());
    });
}

pub(in crate::app_home::about::management::activity::message) async fn lock(
    expected: &Value,
) -> Option<Release> {
    let mut eval = dioxus::document::eval(&format!(
        "{}\n{}\nawait workflowActivityChannelLock({expected},dioxus);",
        include_str!("../../date_witness.js"),
        include_str!("native.js")
    ));
    if !matches!(
        tokio::time::timeout(Duration::from_secs(3), eval.recv::<bool>()).await,
        Ok(Ok(true))
    ) {
        let _ = eval.send(());
        return None;
    }
    Some(retain(eval))
}
