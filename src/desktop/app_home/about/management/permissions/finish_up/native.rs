use super::*;
const SCRIPT: &str = include_str!("native.js");
pub(super) fn source() -> String {
    format!(
        "{}\n{SCRIPT}\n{}\n{}",
        include_str!("../native.js"),
        include_str!("cropper/geometry.js"),
        include_str!("cropper/bridge.js")
    )
}
pub(super) async fn bind(o: &Original, e: &Value) -> bool {
    proof(o, e, None, true).await
}
pub(super) async fn current(o: &Original, e: &Value, action: Option<&Value>) -> bool {
    proof(o, e, action, false).await
}
async fn proof(o: &Original, e: &Value, action: Option<&Value>, bind: bool) -> bool {
    let original = o.expected();
    let action = action.cloned().unwrap_or(Value::Null);
    let proof = if bind {
        format!("await workflowFinishUpBind({original},{e})")
    } else {
        format!("workflowFinishUp({original},{e},{action},{bind})")
    };
    let mut eval = document::eval(&format!(
        "{}\nawait new Promise(resolve=>requestAnimationFrame(resolve)); await workflowPermissionsReply({proof},dioxus);",
        source()
    ));
    let result = tokio::time::timeout(Duration::from_secs(3), eval.recv::<bool>()).await;
    let _ = eval.send(());
    matches!(result, Ok(Ok(true)))
}
