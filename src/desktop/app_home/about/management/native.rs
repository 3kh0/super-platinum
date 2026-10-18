use super::*;
use crate::app_home::inputs::native::{Release, retain};
const SCRIPT: &str = include_str!("native.js");
pub(super) async fn lock_snapshot(expected: &Value) -> Option<Release> {
    let mut eval = dioxus::document::eval(&format!(
        "{SCRIPT}\nawait workflowManagementLock({expected},dioxus);"
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
pub(super) fn mount(visit: u64) {
    document::eval(&format!("{SCRIPT}\nworkflowManagementMount({visit});"));
}
pub(super) async fn lock(state: Signal<ShellState>, id: u64) -> Option<Release> {
    let (visit, document, revision, title, description) = write::expected(&state.peek(), id)?;
    let expected = json!({"visit":visit,"document":document,"revision":revision,"title":title,"description":description});
    let mut eval = dioxus::document::eval(&format!(
        "{SCRIPT}\nawait workflowManagementLock({expected},dioxus);"
    ));
    let ack = tokio::time::timeout(Duration::from_secs(3), eval.recv::<bool>()).await;
    if !matches!(ack, Ok(Ok(true))) || !write::exact(&state.peek(), id) {
        let _ = eval.send(());
        return None;
    }
    Some(retain(eval))
}
