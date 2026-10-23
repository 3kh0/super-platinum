//! Bg2/ ordinary own-edit Texty arguments over actual complete acquired stores.
//! Decoding is not a currentness witness or a native capability override.
use super::{Error, Value, unavailable};
use serde::Serialize;
use serde_json::Map;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::slack_lists::comments::own_edit) struct NativePolicy {
    pub plain_text_mode: bool,
    pub markdown_mode: bool,
    pub supports_portable_content: bool,
    pub slugs_enabled: bool,
    pub emoji_as_text: bool,
    pub convert_emoticons: bool,
    pub composer_smart_symbols: bool,
    pub disable_smart_symbols: bool,
    pub detect_go_links: bool,
    pub enter_creates_newline: bool,
    pub use_pre_newlines: bool,
    pub spellcheck: bool,
    pub underline_links: bool,
    pub flatten_tree_list_before_paste: bool,
}

/// Both arguments must be successful complete original factory responses,
/// retained under the caller's account/auth/preference/assignment guards. A
/// partial cache or a fabricated envelope is not an admitted caller input.
pub(in crate::slack_lists::comments::own_edit) fn native_policy(
    preferences: &Value,
    assignments: &Value,
) -> Result<NativePolicy, Error> {
    let prefs = carrier(preferences, "prefs")?;
    let groups = carrier(assignments, "assignments")?;
    let pref = |name: &str| truthy(prefs.get(name));
    // 4OuQ._Z returns null for a falsy assignment, otherwise its actual group.
    // All JSON primitive/array group results fail the source strict equality;
    // they do not become an inferred on/off group or an unavailable carrier.
    let assigned = |name: &str, expected: &str| {
        groups
            .get(name)
            .and_then(|record| record.get("group"))
            .and_then(Value::as_str)
            == Some(expected)
    };
    let plain_text_mode = pref("plain_text_mode");
    Ok(NativePolicy {
        plain_text_mode,
        // Exact initial state and omitted call props of ordinary Bg2/, not
        // universal composer defaults and not the global plain preference.
        markdown_mode: false,
        supports_portable_content: assigned("portable_content_composer", "treatment")
            && !plain_text_mode,
        slugs_enabled: true,
        emoji_as_text: prefs.get("emoji_mode").and_then(Value::as_str) == Some("as_text"),
        convert_emoticons: pref("convert_emoticons"),
        composer_smart_symbols: assigned("composer_smart_symbols", "on"),
        disable_smart_symbols: pref("disable_smart_symbols"),
        detect_go_links: assigned("detect_go_links", "on"),
        enter_creates_newline: pref("msg_input_send_btn"),
        use_pre_newlines: pref("enter_is_special_in_tbt"),
        spellcheck: pref("webapp_spellcheck"),
        underline_links: pref("underline_links"),
        flatten_tree_list_before_paste: assigned("fe_flatten_tree_list_before_paste", "on"),
    })
}

pub(super) fn carrier<'a>(raw: &'a Value, key: &str) -> Result<&'a Map<String, Value>, Error> {
    if raw["ok"] != true || raw.get("error").is_some_and(|value| !value.is_null()) {
        return Err(unavailable());
    }
    raw.get(key)
        .and_then(Value::as_object)
        .ok_or_else(unavailable)
}
fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null | Value::Bool(false)) => false,
        Some(Value::Number(value)) => value.as_f64().is_some_and(|n| n != 0.0),
        Some(Value::String(value)) => !value.is_empty(),
        Some(Value::Bool(true) | Value::Array(_) | Value::Object(_)) => true,
    }
}

#[cfg(test)]
mod tests;
