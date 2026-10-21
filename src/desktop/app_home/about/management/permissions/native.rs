use super::*;
const SCRIPT: &str = include_str!("native.js");
pub(super) async fn lock(original: &Original) -> Option<Release> {
    let expected = original.expected();
    let mut eval = document::eval(&format!(
        "{}\n{SCRIPT}\nawait workflowPermissionsLock({expected},dioxus);",
        include_str!("../native.js")
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
pub(super) async fn current(original: &Original) -> bool {
    current_for(original, false).await
}
pub(super) async fn current_for(original: &Original, run_access: bool) -> bool {
    let expected = original.expected();

    let mut eval = document::eval(&format!(
        "{SCRIPT}\nawait workflowPermissionsReply(workflowPermissionsCheck({expected},{run_access}),dioxus);"
    ));
    let result = tokio::time::timeout(Duration::from_secs(3), eval.recv::<bool>()).await;
    let _ = eval.send(());
    matches!(result, Ok(Ok(true)))
}

#[derive(serde::Deserialize)]
pub(super) struct Finish {
    pub(super) title: String,
    pub(super) description: String,
}
pub(super) async fn finish_snapshot(original: &Original) -> Option<Finish> {
    let expected = original.expected();
    let mut eval = document::eval(&format!(
        "{SCRIPT}\nawait workflowPermissionsReply(workflowPermissionsFinish({expected}),dioxus);"
    ));
    let result = tokio::time::timeout(Duration::from_secs(3), eval.recv::<Option<Finish>>()).await;
    let _ = eval.send(());
    result.ok()?.ok()?
}

pub(super) async fn bind(original: &Original) {
    let expected = original.expected();
    let mut eval = document::eval(&format!(
        "{SCRIPT}\nawait workflowPermissionsReply(workflowPermissionsBind({expected}),dioxus);"
    ));
    let _ = tokio::time::timeout(Duration::from_secs(3), eval.recv::<bool>()).await;
    let _ = eval.send(());
}
pub(super) async fn choice(original: &Original, value: &str) -> bool {
    let expected = original.expected();
    let value = json!(value);
    let mut eval = document::eval(&format!(
        "{SCRIPT}\nawait workflowPermissionsReply(workflowPermissionsChoice({expected},{value}),dioxus);"
    ));
    let result = tokio::time::timeout(Duration::from_secs(3), eval.recv::<bool>()).await;
    let _ = eval.send(());
    matches!(result, Ok(Ok(true)))
}

pub(super) async fn settings(
    original: &Original,
    expected: &Value,
    action: Option<&Value>,
) -> bool {
    let original = original.expected();
    let action = action.cloned().unwrap_or(Value::Null);
    let mut eval = document::eval(&format!(
        "{SCRIPT}\nawait new Promise(resolve=>requestAnimationFrame(resolve)); await workflowPermissionsReply(workflowPermissionsSettings({original},{expected},{action}),dioxus);"
    ));
    let result = tokio::time::timeout(Duration::from_secs(3), eval.recv::<bool>()).await;
    let _ = eval.send(());
    matches!(result, Ok(Ok(true)))
}

pub(super) async fn bind_settings(original: &Original, expected: &Value) -> bool {
    let original = original.expected();
    let mut eval = document::eval(&format!(
        "{SCRIPT}\nawait workflowPermissionsReply(await workflowPermissionsBindSettings({original},{expected}),dioxus);"
    ));
    let result = tokio::time::timeout(Duration::from_secs(3), eval.recv::<bool>()).await;
    let _ = eval.send(());
    matches!(result, Ok(Ok(true)))
}
