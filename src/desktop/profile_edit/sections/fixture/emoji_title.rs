//! Closed ordinary name-only emoji title vectors through the existing file lifecycle.
use super::*;
use serde_json::json;

pub(crate) const FLAG: &str = "SUPER_PLATINUM_PROFILE_EMOJI_TITLE_CHECK";
pub(crate) const CASE: &str = "SUPER_PLATINUM_PROFILE_EMOJI_TITLE_CASE";
pub(crate) fn case() -> &'static str {
    if std::env::var(CASE).as_deref() == Ok("empty") {
        "empty"
    } else {
        "ordinary"
    }
}
pub(crate) fn named() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && mentions::named()
        && std::env::var(mentions::FLAG).as_deref() == Ok("1")
        && std::env::var(FLAG).as_deref() == Ok("1")
        && matches!(std::env::var(CASE).as_deref(), Ok("ordinary" | "empty"))
        && std::env::var(files::FLAG).as_deref() == Ok("1")
        && std::env::var(files::BLOCKS_FLAG).as_deref() == Ok("1")
        && !files::list()
        && std::env::var(link_title::FLAG).as_deref() != Ok("1")
        && std::env::var(empty_canvas::FLAG).as_deref() != Ok("1")
        && std::env::var("SUPER_PLATINUM_PROFILE_USER_TITLE_CHECK").as_deref() != Ok("1")
        && std::env::var(completion::FLAG).as_deref() != Ok("1")
        && std::env::var("SUPER_PLATINUM_PROFILE_FILE_COMPLETION_CHECK").as_deref() != Ok("1")
}
pub(crate) fn label(renamed: bool) -> &'static str {
    match (case(), renamed) {
        ("empty", false) => "",
        ("empty", true) => ":wave: Plan",
        (_, false) => "Project plan",
        (_, true) => "Project :wave: plan",
    }
}
pub(crate) fn blocks(renamed: bool) -> Value {
    vector(case(), renamed)
}
fn vector(case: &str, renamed: bool) -> Value {
    let emoji = json!({"type":"emoji","name":"wave","style":{"bold":true},"opaque":{"keep":1}});
    let text = |s: &str| json!({"type":"text","text":s});
    let leaves = match (case, renamed) {
        ("empty", false) => vec![emoji],
        ("empty", true) => vec![emoji.clone(), emoji, text(" Plan")],
        (_, false) => vec![emoji, text(" Project plan")],
        (_, true) => vec![text("Project "), emoji, text(" plan")],
    };
    json!([{"type":"rich_text","block_id":"kept-block","elements":[
        {"type":"rich_text_section","elements":leaves}]}])
}
pub(crate) fn snapshot() -> Value {
    json!({"case":case(),"original":blocks(false),"renamed":blocks(true),
        "label":label(false),"renamedLabel":label(true)})
}
