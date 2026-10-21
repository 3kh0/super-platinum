use super::*;
use std::time::Duration;
const SCRIPT: &str = include_str!("native.js");
async fn evaluate(expression: String) -> bool {
    // The reply owns the original Eval until Rust consumes it. Finishing the
    // script early can retire Dioxus's query channel before queued delivery.
    let mut eval = document::eval(&format!(
        "{SCRIPT}\nconst result=await ({expression}); dioxus.send(result); await dioxus.recv();"
    ));
    let result = tokio::time::timeout(Duration::from_secs(3), eval.recv::<bool>()).await;
    let _ = eval.send(());
    matches!(result, Ok(Ok(true)))
}
pub(super) async fn capture(s: &Selection) -> bool {
    evaluate(format!("workflowDetailsCapture({})", s.expected())).await
}
pub(super) async fn bind(s: &Selection) -> bool {
    evaluate(format!("workflowDetailsBind({})", s.expected())).await
}
pub(super) async fn current(s: &Selection, action: &str) -> bool {
    evaluate(format!(
        "workflowDetailsCurrent({},{})",
        s.expected(),
        json!(action)
    ))
    .await
}
pub(super) async fn restore(s: &Selection) {
    let _ = evaluate(format!("workflowDetailsRestore({})", s.expected())).await;
}
