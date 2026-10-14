//! QjSI multi-conversation arrays; editing/capture never dispatches requests.
//! Source: https://a.slack-edge.com/bv1-13/gantry-v2-async-gantry-v2-shared-boot-async.56e901654548b2fe.min.js
//! SHA256 1681ecc18bbfead52efb391b1ccfb3e15f1b4a1093a8475d847b221ce196c537.
//! Decoded offsets: QjSI 13262315, bgmB 1727423, iGIl 1742940,
//! cxJV 1750345, AM6q 1754817. Ordered arrays, explicit [] clears,
//! supplied initial arrays, and no response URL flags. Directory eligibility
//! belongs to the renderer; member IDs remain member IDs, never opened DMs.
//! See docs/slack-conversation-select-protocol.md for default-resolution bounds.
use super::*;
use std::collections::BTreeSet;

pub fn parse_message_conversations(element: &Value) -> Result<FormField, FormError> {
    if element.get("type").and_then(Value::as_str) != Some("multi_conversations_select") {
        return Err(FormError::ValueTypeMismatch);
    }
    if unsupported_settings(element) {
        return Err(FormError::ValueTypeMismatch);
    }
    ConversationFilter::parse(element)?;
    let max = element
        .get("max_selected_items")
        .map(|value| {
            value
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .filter(|n| *n > 0)
                .ok_or(FormError::ValueTypeMismatch)
        })
        .transpose()?;
    let initial = match element.get("initial_conversations") {
        None | Some(Value::Null) => vec![],
        Some(Value::Array(values)) if values.len() <= 1000 => values
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or(FormError::ValueTypeMismatch)
            })
            .collect::<Result<Vec<_>, _>>()?,
        _ => return Err(FormError::ValueTypeMismatch),
    };
    let required = element.get("required").map_or(Ok(false), |v| {
        v.as_bool().ok_or(FormError::ValueTypeMismatch)
    })?;
    let placeholder = match element.get("placeholder") {
        None | Some(Value::Null) => None,
        Some(value) if value["type"] == "plain_text" => Some(
            value
                .get("text")
                .and_then(Value::as_str)
                .filter(|s| s.encode_utf16().count() <= 150)
                .ok_or(FormError::ValueTypeMismatch)?
                .to_owned(),
        ),
        _ => return Err(FormError::ValueTypeMismatch),
    };
    let mut field = FormField {
        label: "Select conversations".into(),
        hint: None,
        placeholder,
        optional: true,
        kind: InputKind::MultiConversation {
            max_selected_items: None,
        },
        initial_value: InputValue::Conversations(initial.clone()),
        value: InputValue::Conversations(initial.clone()),
        dirty: false,
        error: None,
        revision: 0,
        raw: element.clone(),
    };
    // A newly reduced maximum must leave existing selections visible/removable.
    // Validate their identities here; enforce requiredness and the current limit
    // when the user applies a new selection, not on untouched sibling state.
    validate_message_conversations(&field, &initial)?;
    field.kind = InputKind::MultiConversation {
        max_selected_items: max,
    };
    field.optional = !required;
    Ok(field)
}

pub fn validate_message_conversations(
    field: &FormField,
    selected: &[String],
) -> Result<Value, FormError> {
    let InputKind::MultiConversation { max_selected_items } = field.kind else {
        return Err(FormError::ValueTypeMismatch);
    };
    let mut seen = BTreeSet::new();
    if selected.len() > 1000
        || (!field.optional && selected.is_empty())
        || max_selected_items.is_some_and(|max| selected.len() > max)
        || selected
            .iter()
            .any(|id| !valid_message_conversation(id) || !seen.insert(id))
    {
        return Err(FormError::ValueTypeMismatch);
    }
    Ok(json!({"type":"multi_conversations_select", "selected_conversations":selected}))
}

// The default resolver is not present in the recovered renderer. The official
// multi-select rule differs from the single selector: default overrides initial.
// Keep this opt-in unavailable in both contexts until its origin/filter behavior
// is implemented. An absent flag or explicit false needs no origin resolution.
fn unsupported_settings(element: &Value) -> bool {
    element
        .get("default_to_current_conversation")
        .is_some_and(|value| value != &Value::Bool(false))
        || [
            "defaultToCurrentConversation",
            "requested_scope",
            "requestedScope",
            "service_team_id",
            "serviceTeamId",
            "show_enterprise_options",
            "showEnterpriseOptions",
            "container_actions_payload",
            "containerActionsPayload",
            "third_party_auth",
            "thirdPartyAuth",
            "agent_prompt",
            "agentPrompt",
            "agent_prompt_display",
        ]
        .iter()
        .any(|key| element.get(key).is_some())
}
