use super::*;
const SCRIPT: &str = include_str!("native.js");
pub(super) async fn witness(o: &Original, e: &Value, action: Option<&Value>, bind: bool) -> bool {
    let original = o.expected();
    let action = action.cloned().unwrap_or(Value::Null);
    let call = if bind {
        format!("await workflowManagersBind({original},{e})")
    } else {
        format!("workflowManagers({original},{e},{action})")
    };
    let mut eval = document::eval(&format!(
        "{}\n{SCRIPT}\nawait workflowPermissionsReply({call},dioxus);",
        include_str!("../native.js")
    ));
    let result = tokio::time::timeout(Duration::from_secs(3), eval.recv::<bool>()).await;
    let _ = eval.send(());
    matches!(result, Ok(Ok(true)))
}
