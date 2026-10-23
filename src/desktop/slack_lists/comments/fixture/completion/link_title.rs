//! Exact ordinary-link metadata input; all acquisition and title authority stays real.
use super::*;
mod transcript;
pub(crate) use transcript::Transcript;

pub(crate) const FLAG: &str = "SUPER_PLATINUM_LIST_LINK_TITLE_CHECK";
pub(crate) const CASE: &str = "link-title-lifecycle";
pub(crate) const LABEL: &str = "Review &amp; %2F / https://example.test/a%2Fb?x=1&amp;y=2";

pub(crate) fn named() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && super::super::compose::check_enabled()
        && std::env::var(FLAG).as_deref() == Ok("1")
        && std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref() == Ok(CASE)
}

pub(crate) fn blocks() -> Value {
    json!([{"type":"rich_text","block_id":"ordinary-link-title","opaque_block":{"keep":7},"elements":[{"type":"rich_text_section","elements":[
        {"type":"link","text":"Review &amp; %2F","url":"https://example.test/not-the-label","style":{"bold":true},"opaque_link":{"keep":"first"}},
        {"type":"text","text":" / "},
        {"type":"link","text":"","url":"https://example.test/a%2Fb?x=1&amp;y=2","style":{"italic":true},"opaque_link":{"keep":"second"}}
    ]}]}])
}
