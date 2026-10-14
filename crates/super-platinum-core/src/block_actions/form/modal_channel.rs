//! Normal submit-only modal channel inputs. Public/nonarchived availability
//! belongs to the renderer's canonical channel directory, not ID prefixes.
//!
//! Source: https://a.slack-edge.com/bv1-13/gantry-v2-async-gantry-v2-shared-boot-async.56e901654548b2fe.min.js
//! UTF-8 SHA256: 1681ecc18bbfead52efb391b1ccfb3e15f1b4a1093a8475d847b221ce196c537.
//! Decoded character offsets: cdYm 13218167, rlGy 1716521, cxJV 1750345,
//! AM6q 1754021. cdYm emits selectedChannel or null, retains truthy
//! responseUrlEnabled, and searches channels with includeArchived:false and
//! includePrivate:false. It does not read conversation defaults or filters.
//! https://docs.slack.dev/reference/block-kit/block-elements/select-menu-element/#select-menu-of-public-channels
use super::*;

pub(super) fn initial(element: &Value) -> Result<InputValue, FormError> {
    if element
        .get("response_url_enabled")
        .is_some_and(|value| !value.is_null() && !value.is_boolean())
    {
        return Err(FormError::ValueTypeMismatch);
    }
    let initial = match element.get("initial_channel") {
        None | Some(Value::Null) => None,
        Some(Value::String(value)) if value.is_empty() => None,
        Some(Value::String(value)) if valid_id(value) => Some(value.clone()),
        _ => return Err(FormError::ValueTypeMismatch),
    };
    // Unlike conversations_select there is no default-to-opening-conversation
    // contract. An absent initial remains empty even when the modal has origin.
    Ok(InputValue::Channel(initial))
}

pub(super) fn valid_id(id: &str) -> bool {
    // Syntax only. G/C/Z identities must still be checked against canonical
    // public channel metadata; IDs alone do not prove public availability.
    matches!(id.as_bytes().first(), Some(b'C' | b'G' | b'Z')) && valid_message_conversation(id)
}

pub(super) fn unsupported_settings(element: &Value) -> bool {
    // Do not silently apply a conversation filter or resolve an Enterprise
    // service-team restriction using the active channel/workspace. These need
    // their own proven context before this bounded input can support them.
    [
        "filter",
        "default_to_current_conversation",
        "requested_scope",
        "show_enterprise_options",
        "service_team_id",
        "container_actions_payload",
        "defaultToCurrentConversation",
        "requestedScope",
        "showEnterpriseOptions",
        "serviceTeamId",
        "containerActionsPayload",
    ]
    .iter()
    .any(|key| element.get(key).is_some())
}
