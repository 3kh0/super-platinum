//! Pure ordinary document identity; no actor, member or IO authority.
use super::*;
use serde_json::Value;

fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::String(v) => !v.is_empty(),
        Value::Number(v) => v.as_f64().is_some_and(|v| v != 0.0),
        _ => true,
    }
}
pub(crate) fn id<'a>(team: Option<&str>, message: &'a Message) -> Option<&'a str> {
    if message.kind.as_deref() != Some("message")
        || message.subtype.is_some()
        || message.message.is_some()
        || message.bot_id.is_some()
        || message.bot_profile.is_some()
        || message.user.as_deref() == Some("USLACKBOT")
        || !message.files.is_empty()
        || message.team.as_deref().is_some_and(|t| team != Some(t))
        || [
            "file",
            "comment",
            "ai_context",
            "list_edit",
            "room_id",
            "tab_added_user_id",
            "item_added_user_id",
            "app_id",
        ]
        .iter()
        .any(|key| message.extra.contains_key(*key))
    {
        return None;
    }
    let doc = message.extra.get("document_comment")?.as_object()?;
    let id = if let Some(editor) = doc.get("most_recent_editor").filter(|v| truthy(v)) {
        editor.as_str()?
    } else if let Some(authors) = doc.get("authors").filter(|v| truthy(v)) {
        let authors = authors.as_array()?;
        if let Some(first) = authors.first() {
            first.as_str()?
        } else {
            message.user.as_deref()?
        }
    } else {
        message.user.as_deref()?
    };
    super_platinum_core::profile_reading::valid_id(id).then_some(id)
}
