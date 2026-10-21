use super::*;
const SCRIPT: &str = include_str!("native.js");
async fn proof(q: &Query, action: Option<&Value>, bind: bool) -> bool {
    let original = q.original.expected();
    let local = &q.local;
    let expected = &q.expected;
    let action = action.cloned().unwrap_or(Value::Null);
    let expression = if bind {
        format!("await workflowConfirmationAddBind({original},{local},{expected})")
    } else {
        format!("workflowConfirmationAddCurrent({original},{local},{expected},{action})")
    };
    let mut eval = document::eval(&format!(
        "{}\n{SCRIPT}\nawait workflowPermissionsReply({expression},dioxus);",
        super::super::native::source()
    ));
    let result = tokio::time::timeout(Duration::from_secs(3), eval.recv::<bool>()).await;
    let _ = eval.send(());
    matches!(result, Ok(Ok(true)))
}
pub(super) async fn bind(q: &Query) -> bool {
    proof(q, None, true).await
}
pub(super) async fn current(q: &Query, action: Option<&Value>) -> bool {
    proof(q, action, false).await
}
pub(super) async fn lock(q: &Query) -> Option<Release> {
    let original = q.original.expected();
    let local = &q.local;
    let expected = &q.expected;
    let mut eval = document::eval(&format!(
        "{}\n{SCRIPT}\nawait workflowConfirmationAddLock({original},{local},{expected},dioxus);",
        super::super::native::source()
    ));
    if !matches!(
        tokio::time::timeout(Duration::from_secs(3), eval.recv::<bool>()).await,
        Ok(Ok(true))
    ) {
        let _ = eval.send(());
        return None;
    }
    Some(crate::app_home::inputs::native::retain(eval))
}
