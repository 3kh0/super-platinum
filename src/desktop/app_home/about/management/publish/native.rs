use super::*;
use crate::app_home::inputs::native::retain;
const SCRIPT: &str = include_str!("native.js");
pub(super) async fn lock(original: &Original) -> Option<Release> {
    let expected = original.expected();
    let mut eval = document::eval(&format!(
        "{}\n{SCRIPT}\nawait workflowPublishLock({expected},dioxus);",
        include_str!("../native.js")
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
async fn proof(original: &Original, function: &str) -> Option<Value> {
    let expected = original.expected();
    let mut eval = document::eval(&format!(
        "{SCRIPT}\nawait workflowPublishReply({function}({expected}),dioxus);"
    ));
    let result = tokio::time::timeout(Duration::from_secs(3), eval.recv::<Value>()).await;
    let _ = eval.send(());
    result.ok().and_then(Result::ok)
}
pub(super) async fn current(original: &Original) -> bool {
    proof(original, "workflowPublishCurrent").await == Some(json!(true))
}
#[derive(serde::Deserialize)]
pub(super) struct Finish {
    pub title: String,
    pub description: String,
}
pub(super) async fn finish_snapshot(original: &Original) -> Option<Finish> {
    serde_json::from_value(proof(original, "workflowPublishFinish").await?).ok()
}
