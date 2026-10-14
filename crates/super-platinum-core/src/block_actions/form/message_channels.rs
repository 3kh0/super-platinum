//! Ordinary z+fW multi-channel selections; staging never commits wire state.
//! Source: https://a.slack-edge.com/bv1-13/gantry-v2-async-gantry-v2-shared-boot-async.56e901654548b2fe.min.js
//! SHA256 1681ecc18bbfead52efb391b1ccfb3e15f1b4a1093a8475d847b221ce196c537.
//! Decoded offsets: z+fW 13257089, xjrl 1726688, iGIl 1742940,
//! cxJV 1750345, AM6q 1754021. Ordered selected_channels arrays, initial
//! arrays (including []), and no response URL flag. Public/nonarchived
//! directory eligibility belongs to the renderer; this control does not have
//! the single-channel Enterprise service-team restriction.
use super::*;
use std::collections::BTreeSet;

pub fn parse_message_channels(element: &Value) -> Result<FormField, FormError> {
    if element.get("type").and_then(Value::as_str) != Some("multi_channels_select") {
        return Err(FormError::ValueTypeMismatch);
    }
    if unsupported_settings(element) {
        return Err(FormError::ValueTypeMismatch);
    }
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
    let initial = match element.get("initial_channels") {
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
        label: "Select channels".into(),
        hint: None,
        placeholder,
        optional: true,
        kind: InputKind::MultiChannel {
            max_selected_items: None,
        },
        initial_value: InputValue::Channels(initial.clone()),
        value: InputValue::Channels(initial.clone()),
        dirty: false,
        error: None,
        revision: 0,
        raw: element.clone(),
    };
    // A newly reduced maximum must leave existing selections visible/removable.
    // Validate their identities here; enforce requiredness and the current limit
    // when the user applies a new selection, not on untouched sibling state.
    validate_message_channels(&field, &initial)?;
    field.kind = InputKind::MultiChannel {
        max_selected_items: max,
    };
    field.optional = !required;
    Ok(field)
}

pub fn validate_message_channels(
    field: &FormField,
    selected: &[String],
) -> Result<Value, FormError> {
    let InputKind::MultiChannel { max_selected_items } = field.kind else {
        return Err(FormError::ValueTypeMismatch);
    };
    let mut seen = BTreeSet::new();
    if selected.len() > 1000
        || (!field.optional && selected.is_empty())
        || max_selected_items.is_some_and(|max| selected.len() > max)
        || selected
            .iter()
            .any(|id| !modal_channel::valid_id(id) || !seen.insert(id))
    {
        return Err(FormError::ValueTypeMismatch);
    }
    Ok(json!({"type":"multi_channels_select", "selected_channels":selected}))
}

// Do not pretend conversation defaults/filters or specialized auth are supported.
// response_url_enabled is ignored by z+fW/AM6q, even when supplied as true.
fn unsupported_settings(element: &Value) -> bool {
    [
        "filter",
        "default_to_current_conversation",
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
