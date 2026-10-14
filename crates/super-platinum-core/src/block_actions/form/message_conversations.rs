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

// QjSI and both ordinary wrappers (bgmB B1727423, Te7m B13137439)
// never read this flag: supplied/saved arrays remain authoritative. Do not
// synthesize an ordinary origin. Modal parsing separately resolves its documented
// viewed default before this parser. Upstream normalization is not established.
fn unsupported_settings(element: &Value) -> bool {
    element
        .get("default_to_current_conversation")
        .is_some_and(|value| !value.is_boolean())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_default_flag_never_synthesizes_or_replaces_initials() {
        for flag in [None, Some(json!(false)), Some(json!(true))] {
            for initial in [
                None,
                Some(Value::Null),
                Some(json!([])),
                Some(json!(["C00000001", "U00000002"])),
            ] {
                let mut raw = json!({"type":"multi_conversations_select"});
                if let Some(flag) = &flag {
                    raw["default_to_current_conversation"] = flag.clone();
                }
                if let Some(initial) = &initial {
                    raw["initial_conversations"] = initial.clone();
                }
                let field = parse_message_conversations(&raw).unwrap();
                let expected =
                    initial
                        .as_ref()
                        .and_then(Value::as_array)
                        .map_or_else(Vec::new, |ids| {
                            ids.iter()
                                .map(|id| id.as_str().unwrap().to_owned())
                                .collect()
                        });
                assert_eq!(
                    field.initial_value,
                    InputValue::Conversations(expected.clone())
                );
                assert_eq!(field.value, InputValue::Conversations(expected));
                assert_eq!(field.raw, raw);
            }
        }
    }

    #[test]
    fn ordinary_default_flag_accepts_only_booleans_when_present() {
        for flag in [
            Value::Null,
            json!("true"),
            json!(0),
            json!(1),
            json!([]),
            json!({}),
        ] {
            assert!(parse_message_conversations(&json!({"type":"multi_conversations_select", "default_to_current_conversation":flag})).is_err());
        }
    }
}
