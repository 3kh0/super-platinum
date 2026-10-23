//! Actual JfMk applies right-to-left changes inside triple-backtick captures.
use super::*;
use patterns::{Patterns, group};

pub(super) fn repair(document: &mut Document) -> Result<()> {
    let text = regex_text(document)?;
    for found in Patterns::get()?.all("SM/6:u_", &text).iter().rev() {
        let start = found.range.start + group(found, &text, 1).len();
        let length = group(found, &text, 2).len();
        let original = document.slice(
            start.min(document.len()),
            (start + length).min(document.len()),
        )?;
        let mut change = Change::default();
        change.retain(start);
        for op in original.ops()? {
            if document::recognized_embed(&op["insert"]) {
                change.delete(1);
                if let Some(emoji) = op["insert"].get("slackemoji").filter(|v| truthy(Some(v))) {
                    let text =
                        if emoji.is_boolean() || emoji.as_object().is_some_and(|o| o.is_empty()) {
                            ""
                        } else {
                            emoji
                                .get("text")
                                .and_then(Value::as_str)
                                .ok_or(SeedUnavailable::MalformedMessage)?
                        };
                    change.insert(text, Default::default());
                }
            } else {
                let length = op["insert"]
                    .as_str()
                    .map_or(1, |text| text.encode_utf16().count());
                change.annotate(length,json!({"bold":false,"italic":false,"strike":false,"code":false,"underline":false,"slackmention":false,"slackslug":false,"link":false,"unlink":false}).as_object().unwrap().clone());
            }
        }
        document.apply(&change)?;
    }
    Ok(())
}
