//! Keep renderer custody through the parent's synchronous final owner check.
use crate::app_home::inputs::native::{Release, retain};
use serde_json::Value;
use std::time::Duration;

pub(super) fn feedback(expected: &Value, message: &str) {
    let mut eval = dioxus::document::eval(&format!(
        "{}\ndioxus.send(workflowActivityChannelFeedback({expected},{}));await dioxus.recv();",
        include_str!("native.js"),
        serde_json::json!(message)
    ));
    dioxus::core::spawn_forever(async move {
        let _ = tokio::time::timeout(Duration::from_secs(3), eval.recv::<bool>()).await;
        let _ = eval.send(());
    });
}

pub(super) async fn lock(expected: &Value) -> Option<Release> {
    let mut eval = dioxus::document::eval(&format!(
        "{}\nawait workflowActivityChannelLock({expected},dioxus);",
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
