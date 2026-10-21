//! Bounded qe6w/8A13 ordinary title projection, independent of Profile Quip titles.
use super::*;
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::app_home::about::management::activity::message) struct Label {
    pub text: String,
    pub url: Option<String>,
}
pub(super) fn truth(v: &Value) -> bool {
    match v {
        Value::Null | Value::Bool(false) => false,
        Value::String(s) => !s.is_empty(),
        Value::Number(n) => n.as_f64().is_some_and(|v| v != 0.0),
        _ => true,
    }
}
pub(super) fn kind(raw: &Value) -> Option<Kind> {
    if !raw.is_object()
        || [
            "isUnknown",
            "isNonExistent",
            "is_deleted",
            "is_tombstoned",
            "is_hidden_by_limit",
            "is_external",
            "is_skill",
            "is_huddle_canvas",
            "is_huddle_transcript_canvas",
        ]
        .iter()
        .any(|k| truth(&raw[*k]))
        || [
            "restriction_type",
            "file_access",
            "product_icon",
            "canvas_template_mode",
            "list_template_mode",
            "huddle_summary_id",
            "huddle_transcript_file_id",
        ]
        .iter()
        .any(|k| truth(&raw[*k]))
    {
        return None;
    }
    if raw["mode"] == "quip" || raw["filetype"] == "quip" {
        Some(Kind::Canvas)
    } else if (raw["mode"] == "list" || raw["filetype"] == "list")
        && raw["subtype"].as_str().is_none_or(|s| s.is_empty())
    {
        Some(Kind::List)
    } else {
        None
    }
}
// L7LR is one lodash-unescape pass, not a broad HTML/numeric-entity decoder.
fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        if let Some((a, b)) = [
            ("&amp;", "&"),
            ("&lt;", "<"),
            ("&gt;", ">"),
            ("&quot;", "\""),
            ("&#39;", "'"),
        ]
        .into_iter()
        .find(|(a, _)| rest.starts_with(a))
        {
            out.push_str(b);
            rest = &rest[a.len()..];
        } else {
            out.push('&');
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    out
}
pub(super) fn label(raw: &Value, expected: Kind) -> Option<Label> {
    if kind(raw) != Some(expected) {
        return None;
    }
    // Rich titles and nested title actions are deliberately not flattened.
    if expected == Kind::Canvas && raw.get("title_blocks").is_some_and(|v| !v.is_null()) {
        return None;
    }
    let title = match raw.get("title") {
        None | Some(Value::Null) => "",
        Some(Value::String(s)) => s,
        _ => return None,
    };
    let text = if expected == Kind::List {
        // Default qe6w, without an admitted highlight mode: P6 decodes once,
        // then BCG1's no-marker fallback forwards that text to escaped HOzI.
        // Keep these two passes local; Canvas and the List heading differ.
        unescape(&unescape(title))
    } else if !title.is_empty() {
        unescape(title)
    } else {
        let name = raw.get("name")?.as_str()?.trim();
        if name.is_empty() {
            return None;
        }
        name.into()
    };
    // This slice does not substitute a Profile name fallback for an empty List title.
    if text.is_empty() {
        return None;
    }
    let url = raw
        .get("permalink")
        .and_then(Value::as_str)
        .filter(|s| s.starts_with("https://") && !s.chars().any(char::is_control))
        .map(str::to_owned);
    Some(Label { text, url })
}
