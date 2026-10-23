//! Exact opt-in transcript title inputs. Canonical publication stays in the reader.
use super::*;

pub(crate) const FLAG: &str = "SUPER_PLATINUM_LIST_EMPTY_CANVAS_TRANSCRIPT_CHECK";
pub(crate) fn case() -> Option<&'static str> {
    if !cfg!(any(test, feature = "protected-native-fixture"))
        || !compose::check_enabled()
        || std::env::var(FLAG).as_deref() != Ok("1")
    {
        return None;
    }
    match (
        std::env::var("SUPER_PLATINUM_LIST_EMPTY_CANVAS_TRANSCRIPT_CASE").as_deref(),
        std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref(),
    ) {
        (Ok("plain-empty"), Ok("cold-files")) => Some("plain-empty"),
        (Ok("blocks-empty"), Ok("cold-files-recovery")) => Some("blocks-empty"),
        _ => None,
    }
}

pub(crate) fn label(original: &str) -> &str {
    if case().is_some() {
        "Untitled"
    } else {
        original
    }
}

/// Replace only the original scalar title, retaining the literal channel_map
/// key order consumed by OrderedFileInfo. Name remains an unrelated raw field.
pub(super) fn response(id: &str, original: &str) -> Result<String, Error> {
    let Some(case) = case().filter(|_| id == CANVAS) else {
        return Ok(original.into());
    };
    let (title, blocks) = if case == "plain-empty" {
        (" \t ", json!([]))
    } else {
        (
            "Misleading plain Canvas title",
            json!([{"type":"rich_text","block_id":"empty-transcript-title","opaque":{"keep":"block"},"elements":[{"type":"rich_text_section","opaque":{"keep":"section"},"elements":[{"type":"text","text":" \t ","style":{"italic":true},"opaque":{"keep":"whitespace"}}]}]}]),
        )
    };
    let marker = "\"title\":\"Alex Canvas\"";
    if original.matches(marker).count() != 1 {
        return Err(refused());
    }
    Ok(original.replacen(
        marker,
        &format!("\"title\":{},\"title_blocks\":{blocks}", json!(title)),
        1,
    ))
}
