//! Named link-only completion bytes; no new policy, member or metadata authority.
use super::*;
use serde_json::json;

pub(crate) const FLAG: &str = "SUPER_PLATINUM_PROFILE_LINK_TITLE_COMPLETION_CHECK";
pub(crate) const LABEL: &str = "Review &amp; %2F / https://example.test/a%2Fb?x=1&amp;y=2";

pub(crate) fn named() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && completion::files::named()
        && std::env::var(FLAG).as_deref() == Ok("1")
        && std::env::var(mentions::FLAG).as_deref() == Ok("1")
        && std::env::var("SUPER_PLATINUM_PROFILE_USER_MENTIONS_CHECK").as_deref() == Ok("1")
        && std::env::var(empty_canvas::FLAG).as_deref() != Ok("1")
        && std::env::var(completion::title_users::FLAG).as_deref() != Ok("1")
        && std::env::var(files::FLAG).as_deref() != Ok("1")
}

pub(crate) fn blocks() -> Value {
    json!([{"type":"rich_text","block_id":"link-completion-title","opaque":{"keep":"block"},"elements":[
    {"type":"rich_text_section","opaque":{"keep":"section"},"elements":[
        {"type":"link","text":"Review &amp; %2F","url":"https://example.test/not-the-label","style":{"bold":true},"opaque_link":{"keep":"first"}},
        {"type":"text","text":" / "},
        {"type":"link","text":"","url":"https://example.test/a%2Fb?x=1&amp;y=2","style":{"italic":true},"opaque_link":{"keep":"second"}}
    ]}]}])
}

pub(crate) fn response() -> String {
    // Keep authored channel_map ordering and every unrelated raw byte intact.
    completion::files::RESPONSE.replacen(
        "\"title\":\"Alex Canvas\"",
        &format!("\"title\":\"Alex Canvas\",\"title_blocks\":{}", blocks()),
        1,
    )
}

pub(crate) fn snapshot() -> Value {
    json!({"label":LABEL,"blocks":blocks(),"completionResponseBytes":response()})
}
