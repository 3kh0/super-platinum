//! QjSI arrays use the conversation directory without inferring an origin.
use super::{
    channels,
    user::{conversations::Filter, directory::Choice},
};
use crate::{message_buttons::source::Source, state::ShellState};
use super_platinum_core::state::Workspace;
pub(crate) const ORIGIN_UNAVAILABLE: &str =
    "Conversation selection with a default conversation is not supported yet.";
pub(crate) const INVALID: &str =
    "A selected conversation is unavailable. Remove it or select it again.";

pub(crate) fn validate(
    w: &Workspace,
    policy: &Filter,
    selected: &[String],
    proofs: &channels::Proofs,
) -> Result<Vec<Choice>, &'static str> {
    if let Some(reason) = super::user::conversations::unavailable(policy) {
        return Err(reason);
    }
    channels::validate(w, policy, selected, proofs).map_err(|_| INVALID)
}

pub(crate) fn origin_unsupported(raw: &serde_json::Value) -> bool {
    raw["type"] == "multi_conversations_select" && raw["default_to_current_conversation"] == true
}
pub(super) fn unavailable(shell: &ShellState, source: &Source) -> Option<&'static str> {
    source
        .message(shell)
        .ok()?
        .blocks
        .iter()
        .any(|block| {
            block.get("accessory").is_some_and(origin_unsupported)
                || block
                    .get("elements")
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|elements| elements.iter().any(origin_unsupported))
        })
        .then_some(ORIGIN_UNAVAILABLE)
}
