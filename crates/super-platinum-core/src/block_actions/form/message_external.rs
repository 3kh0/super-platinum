//! IKXd single external selection. Dynamic options do not require static membership.
use super::*;
pub fn parse_message_external(element: &Value) -> Result<FormField, FormError> {
    if element["type"] != "external_select"
        || [
            "options",
            "option_groups",
            "filter",
            "response_url_enabled",
            "default_to_current_conversation",
            "requested_scope",
            "service_team_id",
            "serviceTeamId",
            "third_party_auth",
            "thirdPartyAuth",
            "agent_prompt",
            "agentPrompt",
            "agent_prompt_display",
            "container_actions_payload",
            "containerActionsPayload",
        ]
        .iter()
        .any(|key| element.get(*key).is_some())
    {
        return Err(FormError::ValueTypeMismatch);
    }
    let min_query_length = element
        .get("min_query_length")
        .map(|v| {
            v.as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .ok_or(FormError::ValueTypeMismatch)
        })
        .transpose()?
        .unwrap_or(3);
    let initial = element
        .get("initial_option")
        .filter(|v| !v.is_null())
        .map(ExternalOption::parse)
        .transpose()?;
    let optional = !element
        .get("required")
        .map(|v| v.as_bool().ok_or(FormError::ValueTypeMismatch))
        .transpose()?
        .unwrap_or(false);
    let placeholder = element
        .get("placeholder")
        .filter(|v| !v.is_null())
        .map(|v| super::super::external::plain(v, 150).map(str::to_owned))
        .transpose()?;
    Ok(FormField {
        label: "Select an option".into(),
        hint: None,
        placeholder,
        optional,
        kind: InputKind::ExternalSelect { min_query_length },
        initial_value: InputValue::External(initial.clone()),
        value: InputValue::External(initial),
        dirty: false,
        error: None,
        revision: 0,
        raw: element.clone(),
    })
}
pub fn validate_message_external(
    field: &FormField,
    selected: Option<&ExternalOption>,
) -> Result<Value, FormError> {
    if !matches!(field.kind, InputKind::ExternalSelect { .. })
        || (!field.optional && selected.is_none())
    {
        return Err(FormError::ValueTypeMismatch);
    }
    Ok(json!({"type":"external_select", "selected_option":selected.map(ExternalOption::raw)}))
}
