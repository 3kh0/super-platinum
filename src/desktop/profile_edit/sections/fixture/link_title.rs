//! Closed ordinary link-title leaves; original file and editor ownership stay intact.
use super::*;
use serde_json::json;

pub(crate) const FLAG: &str = "SUPER_PLATINUM_PROFILE_LINK_TITLE_CHECK";
pub(crate) const ORIGINAL: &str = "&amp; *x* A%2F E N [ ]";
pub(crate) const RENAMED: &str = "Changed &amp; A%2F E N [ ]";

pub(crate) fn named() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && mentions::named()
        && std::env::var(mentions::FLAG).as_deref() == Ok("1")
        && std::env::var(FLAG).as_deref() == Ok("1")
        && std::env::var(files::FLAG).as_deref() == Ok("1")
        && std::env::var(files::BLOCKS_FLAG).as_deref() == Ok("1")
        && !files::list()
        && std::env::var(empty_canvas::FLAG).as_deref() != Ok("1")
        && std::env::var("SUPER_PLATINUM_PROFILE_USER_TITLE_CHECK").as_deref() != Ok("1")
}

pub(crate) fn blocks(renamed: bool) -> Value {
    json!([{"type":"rich_text","block_id":"link-title-block",
    "opaque_block":{"retain":["link",7]},"elements":[{
    "type":"rich_text_section","opaque_section":{"retain":true},"elements":[
        {"type":"link","text":if renamed{"Changed &amp;"}else{"&amp; *x*"},"url":"https://fixture.invalid/ignored","truncated":true,"style":{"bold":true},"opaque":{"index":0}},
        {"type":"text","text":" "},
        {"type":"link","url":"A%2F","style":{"italic":true},"opaque":{"index":1}},
        {"type":"text","text":" "},
        {"type":"link","text":"","url":"E","opaque":{"index":2}},
        {"type":"text","text":" "},
        {"type":"link","text":null,"url":"N","opaque":{"index":3}},
        {"type":"text","text":" ["},
        {"type":"link","text":" ","url":"SHOULD_NOT_RENDER","opaque":{"index":4}},
        {"type":"text","text":"]"},
        {"type":"link","url":null,"opaque":{"index":5}},
        {"type":"link","text":null,"url":"","opaque":{"index":6}},
        {"type":"link","opaque":{"index":7}}
    ]}]}])
}

pub(crate) fn snapshot() -> Value {
    json!({"original":blocks(false),"renamed":blocks(true),"label":ORIGINAL,"renamedLabel":RENAMED})
}
