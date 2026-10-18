use crate::app_home::inputs::native::{Release, retain};
pub(super) async fn lock(key: &str) -> Option<Release> {
    let key = serde_json::to_string(key).ok()?;
    let mut eval = dioxus::document::eval(&format!(
        "{}\nawait workflowLogsEntry({key},dioxus);",
        include_str!("native.js")
    ));
    if !matches!(
        tokio::time::timeout(std::time::Duration::from_secs(3), eval.recv::<bool>()).await,
        Ok(Ok(true))
    ) {
        let _ = eval.send(());
        return None;
    }
    Some(retain(eval))
}
