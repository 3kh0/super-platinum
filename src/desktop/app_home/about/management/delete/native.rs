use super::*;
const SCRIPT: &str = include_str!("native.js");
pub(super) async fn lock(original: &Original) -> Option<Release> {
    let expected = original.expected();
    let mut eval = document::eval(&format!(
        "{}\n{SCRIPT}\nawait workflowDeleteLock({expected},dioxus);",
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
    let mut eval = document::eval(&format!(
        "{SCRIPT}\ndioxus.send(workflowDeleteCheck({expected}));"
    ));
    matches!(
        tokio::time::timeout(Duration::from_secs(3), eval.recv::<bool>()).await,
        Ok(Ok(true))
    )
}

#[derive(serde::Deserialize)]
pub(super) struct Finish {
    pub(super) title: String,
    pub(super) description: String,
}
pub(super) async fn finish_snapshot(original: &Original) -> Option<Finish> {
    let expected = original.expected();
    let mut eval = document::eval(&format!(
        "{SCRIPT}\ndioxus.send(workflowDeleteFinish({expected}));"
    ));
    tokio::time::timeout(Duration::from_secs(3), eval.recv::<Option<Finish>>())
        .await
        .ok()?
        .ok()?
}
