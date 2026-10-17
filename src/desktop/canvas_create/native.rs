use super::domain;
use dioxus::prelude::*;

#[derive(serde::Deserialize)]
#[serde(tag = "status", rename_all = "lowercase", deny_unknown_fields)]
enum Receipt {
    Sealed { title: String, markdown: String },
    Composing {},
    Capacity {},
    Unavailable {},
}
pub(super) async fn install(visit: &str, editable: bool) -> bool {
    let script = format!(
        "{}\ndioxus.send(window.superPlatinumCanvasCreation.install({},{}));",
        include_str!("native.js"),
        serde_json::to_string(visit).unwrap(),
        editable
    );
    document::eval(&script)
        .recv::<bool>()
        .await
        .unwrap_or(false)
}
pub(super) async fn capture(visit: &str, token: &str) -> Result<domain::Content, &'static str> {
    let script = format!(
        "dioxus.send(window.superPlatinumCanvasCreation?.capture({},{}) ?? {{status:'unavailable'}});",
        serde_json::to_string(visit).unwrap(),
        serde_json::to_string(token).unwrap()
    );
    // Retained root task, deliberately not dropped on a timeout: a late
    // successful native payload still owns its already-charged capture slot.
    match document::eval(&script).recv::<Receipt>().await {
        Ok(Receipt::Sealed { title, markdown }) => {
            domain::Content::new(title, markdown).map_err(|_| "Shorten this canvas before saving.")
        }
        Ok(Receipt::Composing {}) => Err("Finish composing text, then try again."),
        Ok(Receipt::Capacity {}) => Err("Shorten this canvas before saving."),
        _ => Err("The editor changed. Your retained draft has not been discarded."),
    }
}
pub(super) fn release(visit: &str, token: &str) {
    document::eval(&format!(
        "window.superPlatinumCanvasCreation?.release({},{});",
        serde_json::to_string(visit).unwrap(),
        serde_json::to_string(token).unwrap()
    ));
}
pub(super) fn retire(visit: &str) {
    document::eval(&format!(
        "window.superPlatinumCanvasCreation?.retire({});",
        serde_json::to_string(visit).unwrap()
    ));
}
pub(super) fn finish(visit: &str, token: &str, editable: bool) {
    document::eval(&format!(
        "window.superPlatinumCanvasCreation?.finish({},{},{});",
        serde_json::to_string(visit).unwrap(),
        serde_json::to_string(token).unwrap(),
        editable
    ));
}
pub(super) fn editable(visit: &str, enabled: bool) {
    document::eval(&format!(
        "window.superPlatinumCanvasCreation?.editable({},{});",
        serde_json::to_string(visit).unwrap(),
        enabled
    ));
}
