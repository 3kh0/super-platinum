//! cdYm (B13219531) derives Enterprise service-team restrictions from context.
//! The bounded message slice does not substitute external_team for allowed_internal_teams.
use super::*;
use super_platinum_core::block_actions::form::ConversationFilter;
pub(crate) const UNAVAILABLE: &str =
    "Channel selection is unavailable for this Enterprise Grid context.";
pub(crate) fn filter(shell: &ShellState, team: &str) -> user::conversations::Filter {
    user::conversations::Filter {
        spec: ConversationFilter {
            include: Some(vec!["public".into()]),
            ..Default::default()
        },
        enterprise_id: shell
            .core
            .session
            .as_ref()
            .and_then(|s| s.workspaces.get(team))
            .and_then(|w| w.enterprise_id.clone()),
        default_channels: false,
    }
}
/// Checked before cached models are accepted and again at request preparation.
/// Inspect only message-owned controls; the normal parser rejects hidden/nested controls.
pub(in crate::message_buttons) fn unavailable(
    shell: &ShellState,
    source: &Source,
) -> Option<&'static str> {
    if shell
        .core
        .session
        .as_ref()
        .and_then(|s| s.workspaces.get(&source.owner().team_id))
        .and_then(|w| w.enterprise_id.as_ref())
        .is_none()
    {
        return None;
    }
    let message = source.message(shell).ok()?;
    message
        .blocks
        .iter()
        .any(|block| {
            block.get("accessory").is_some_and(is_channel)
                || block
                    .get("elements")
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|es| es.iter().any(is_channel))
        })
        .then_some(UNAVAILABLE)
}
fn is_channel(value: &serde_json::Value) -> bool {
    value.get("type").and_then(serde_json::Value::as_str) == Some("channels_select")
}
