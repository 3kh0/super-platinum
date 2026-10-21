use super::*;
const SCRIPT: &str = include_str!("native.js");
pub(super) async fn lock(original: &Original) -> Option<Release> {
    let expected = original.expected();
    let mut eval = document::eval(&format!(
        "{}\n{SCRIPT}\nawait workflowUnpublishLock({expected},dioxus);",
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
    let expected = original.expected();
    if fixture::unpublish::requested() {
        let mut eval = document::eval(&format!(
            "{SCRIPT}\nawait workflowUnpublishReply(workflowUnpublishDiagnostic({expected}),dioxus);"
        ));
        let result = tokio::time::timeout(Duration::from_secs(3), eval.recv::<Value>()).await;
        let _ = eval.send(());
        return match result {
            Ok(Ok(value)) => {
                let ok = value["ok"] == true;
                if !ok {
                    eprintln!("workflow-unpublish-native-guard: {}", value);
                }
                ok
            }
            Ok(Err(error)) => {
                // EvalError Display reports its enum category, never JS/text.
                eprintln!("workflow-unpublish-native-guard: {error}");
                false
            }
            Err(_) => {
                eprintln!("workflow-unpublish-native-guard: timeout");
                false
            }
        };
    }
    let mut eval = document::eval(&format!(
        "{SCRIPT}\nawait workflowUnpublishReply(workflowUnpublishCheck({expected}),dioxus);"
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
        "{SCRIPT}\nawait workflowUnpublishReply(workflowUnpublishFinish({expected}),dioxus);"
    ));
    let result = tokio::time::timeout(Duration::from_secs(3), eval.recv::<Option<Finish>>()).await;
    let _ = eval.send(());
    result.ok()?.ok()?
}
