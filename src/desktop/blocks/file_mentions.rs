//! Preserve whole file occurrences; unsupported variants remain visible text.
use super::{BlockCtx, RichNode, Value};
use super_platinum_core::slack::api::custom::RichTextContext;
#[cfg(test)]
mod tests;

pub(super) fn node(ctx: BlockCtx<'_>, raw: &Value) -> RichNode {
    let fallback = fallback(raw);
    let whole = RichTextContext::new_ordered(&ctx.workspace.team_id, &ctx.workspace.self_user_id)
        .and_then(|mut context| context.file(raw, fallback.clone()))
        .is_ok();
    if whole {
        RichNode::FileMention {
            team: ctx.workspace.team_id.clone(),
            raw: raw.clone(),
            fallback,
        }
    } else {
        // Do not grant section/record/skill behavior via a whole-file renderer.
        RichNode::StyledText {
            text: fallback,
            bold: flag(raw, "bold"),
            italic: flag(raw, "italic"),
            strike: flag(raw, "strike"),
            code: flag(raw, "code"),
        }
    }
}
fn flag(raw: &Value, name: &str) -> bool {
    raw["style"][name].as_bool().unwrap_or(false)
}
fn fallback(raw: &Value) -> String {
    ["text", "url", "file_id"]
        .into_iter()
        .find_map(|key| raw[key].as_str().filter(|s| !s.is_empty()))
        .unwrap_or(if raw["type"] == "canvas" {
            "Canvas"
        } else {
            "File"
        })
        .to_owned()
}
