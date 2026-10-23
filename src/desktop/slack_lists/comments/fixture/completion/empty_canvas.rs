//! Named empty Canvas reply inputs. No display authority or query receipt is seeded.
use super::*;
pub(crate) const FLAG: &str = "SUPER_PLATINUM_LIST_EMPTY_CANVAS_REPLY_CHECK";
pub(crate) fn own_edit_named() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && super::super::compose::check_enabled()
        && std::env::var("SUPER_PLATINUM_LIST_OWN_EDIT_EMPTY_CANVAS_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref(),
            Ok("own-edit-empty-lifecycle" | "own-edit-empty-recovery")
        )
}
pub(crate) fn named() -> bool {
    // The exact link mode reuses only this no-user response/lifecycle machinery.
    if super::link_title::named() {
        return true;
    }
    if own_edit_named() {
        return true;
    }
    cfg!(any(test, feature = "protected-native-fixture"))
        && super::super::compose::check_enabled()
        && std::env::var(FLAG).as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref(),
            Ok("empty-canvas-lifecycle" | "empty-canvas-recovery")
        )
}
pub(crate) const LABEL: &str = "Untitled";
pub(crate) fn plain_seed() -> bool {
    own_edit_named()
        && std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref()
            == Ok("own-edit-empty-lifecycle")
}
pub(crate) fn blocks() -> Value {
    if super::link_title::named() {
        return super::link_title::blocks();
    }
    json!([{"type":"rich_text","block_id":"empty-reply-title","opaque":{"keep":"block"},"elements":[{"type":"rich_text_section","elements":[{"type":"text","text":" \t ","style":{"italic":true},"opaque":{"keep":"whitespace"}}]}]}])
}
pub(crate) fn response(original: &str, plain: bool) -> Result<String, Error> {
    if super::link_title::named() && plain {
        return Err(refused());
    }
    let marker = "\"title\":\"Alex Canvas\"";
    if original.matches(marker).count() != 1 {
        return Err(refused());
    }
    Ok(original.replacen(
        marker,
        &format!(
            "\"title\":{},\"title_blocks\":{}",
            json!(if plain { " \t " } else { "Alex Canvas" }),
            if plain { json!([]) } else { blocks() }
        ),
        1,
    ))
}
pub(crate) fn search_response() -> String {
    response(files::RESPONSE, false).expect("original literal Canvas title")
}
