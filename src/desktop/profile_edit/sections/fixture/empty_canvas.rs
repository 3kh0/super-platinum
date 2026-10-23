//! Exact opt-in empty-title responses; no locale or file authority is injected.
use super::*;
use serde_json::json;
pub(crate) const FLAG: &str = "SUPER_PLATINUM_PROFILE_EMPTY_CANVAS_TITLE_CHECK";
pub(crate) fn case() -> String {
    std::env::var("SUPER_PLATINUM_PROFILE_EMPTY_CANVAS_TITLE_CASE").unwrap_or_default()
}
fn gate() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && mentions::named()
        && std::env::var(FLAG).as_deref() == Ok("1")
}
pub(crate) fn existing() -> bool {
    gate()
        && case() == "existing"
        && std::env::var(files::FLAG).as_deref() == Ok("1")
        && std::env::var(files::BLOCKS_FLAG).as_deref() == Ok("1")
}
pub(crate) fn completion() -> bool {
    gate()
        && case() == "completion"
        && std::env::var(completion::FLAG).as_deref() == Ok("1")
        && std::env::var(completion::files::FLAG).as_deref() == Ok("1")
}
pub(crate) const ORIGINAL: &str = "Canvas before empty";
pub(crate) const EMPTY: &str = "Untitled";
pub(crate) fn blocks(empty: bool) -> Value {
    if empty {
        json!([])
    } else {
        json!([{"type":"rich_text","block_id":"empty-title-original","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":ORIGINAL,"style":{"bold":true},"opaque":{"keep":"original title"}}]}]}])
    }
}
pub(crate) fn decorate(raw: &mut Value, empty: bool) {
    raw["file"]["title"] = json!(if empty { " \t " } else { ORIGINAL });
    raw["file"]["title_blocks"] = blocks(empty);
}
pub(crate) fn whitespace_blocks() -> Value {
    json!([{"type":"rich_text","block_id":"empty-completion-title","opaque":{"keep":"block"},"elements":[{"type":"rich_text_section","opaque":{"keep":"section"},"elements":[{"type":"text","text":" \t ","style":{"italic":true},"opaque":{"keep":"whitespace"}}]}]}])
}
pub(crate) fn response() -> String {
    completion::files::RESPONSE.replacen(
        "\"title\":\"Alex Canvas\"",
        &format!(
            "\"title\":\"Misleading plain title\",\"title_blocks\":{}",
            whitespace_blocks()
        ),
        1,
    )
}
pub(crate) fn snapshot() -> Value {
    json!({"case":case(),"plainEmpty": " \t ","existingOriginalBlocks":blocks(false),"existingEmptyBlocks":blocks(true),"completionBlocks":whitespace_blocks(),"completionResponseBytes":response()})
}
