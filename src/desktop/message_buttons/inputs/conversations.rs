//! QjSI arrays use the conversation directory without inferring an origin.
use super::{
    channels,
    user::{conversations::Filter, directory::Choice},
};
use super_platinum_core::state::Workspace;
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
