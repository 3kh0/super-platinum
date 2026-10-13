//! Ordinary conversations_select values and the captured o5++ directory filter.
use super::*;

pub fn parse_message_conversation(element: &Value) -> Result<FormField, FormError> {
    if element.get("type").and_then(Value::as_str) != Some("conversations_select") {
        return Err(FormError::ValueTypeMismatch);
    }
    ConversationFilter::parse(element)?;
    for key in ["response_url_enabled", "default_to_current_conversation"] {
        if element
            .get(key)
            .is_some_and(|v| !v.is_null() && !v.is_boolean())
        {
            return Err(FormError::ValueTypeMismatch);
        }
    }
    let initial = match element.get("initial_conversation") {
        None | Some(Value::Null) => None,
        Some(Value::String(value)) if value.is_empty() => None,
        Some(Value::String(value)) => Some(value.clone()),
        _ => return Err(FormError::ValueTypeMismatch),
    };
    let placeholder = match element.get("placeholder") {
        None | Some(Value::Null) => None,
        Some(value) if value.get("type").and_then(Value::as_str) == Some("plain_text") => Some(
            value
                .get("text")
                .and_then(Value::as_str)
                .ok_or(FormError::ValueTypeMismatch)?
                .to_owned(),
        ),
        _ => return Err(FormError::ValueTypeMismatch),
    };
    let field = FormField {
        label: "Select a conversation".into(),
        hint: None,
        placeholder,
        optional: true,
        kind: InputKind::Conversation,
        initial_value: InputValue::Conversation(initial.clone()),
        value: InputValue::Conversation(initial),
        dirty: false,
        error: None,
        revision: 0,
        raw: element.clone(),
    };
    if values::error(&field).is_some() {
        return Err(FormError::ValueTypeMismatch);
    }
    Ok(field)
}

/// None is the captured explicit-null action. Empty or malformed strings are not conversations.
pub fn validate_message_conversation(
    field: &FormField,
    selected: Option<&str>,
) -> Result<Value, FormError> {
    if !matches!(field.kind, InputKind::Conversation) || selected.is_some_and(str::is_empty) {
        return Err(FormError::ValueTypeMismatch);
    }
    let mut field = field.clone();
    field.value = InputValue::Conversation(selected.map(str::to_owned));
    if values::error(&field).is_some() {
        return Err(FormError::ValueTypeMismatch);
    }
    let mut wire = values::wire(&field);
    if field.raw.get("response_url_enabled") == Some(&Value::Bool(true)) {
        wire["response_url_enabled"] = json!(true);
    }
    Ok(wire)
}

/// The captured search wrapper forwards item IDs unchanged, including member IDs.
pub fn valid_message_conversation(id: &str) -> bool {
    super::valid_message_user(id)
        || (id.len() >= 9
            && matches!(id.as_bytes().first(), Some(b'C' | b'G' | b'D' | b'Z'))
            && id
                .bytes()
                .skip(1)
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()))
}

/// o5++ filters. Team restrictions are retained, and applied by directory policy.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConversationFilter {
    pub include: Option<Vec<String>>,
    pub exclude_bot_users: bool,
    pub exclude_external_shared_channels: bool,
    pub only_from_team_id: Option<String>,
}
impl ConversationFilter {
    pub fn parse(element: &Value) -> Result<Self, FormError> {
        let Some(value) = element.get("filter").filter(|v| !v.is_null()) else {
            return Ok(Self::default());
        };
        let object = value.as_object().ok_or(FormError::ValueTypeMismatch)?;
        if object.keys().any(|key| {
            !matches!(
                key.as_str(),
                "include"
                    | "exclude_bot_users"
                    | "exclude_external_shared_channels"
                    | "only_from_team_id"
            )
        }) {
            return Err(FormError::ValueTypeMismatch);
        }
        let mut filter = Self::default();
        if let Some(value) = object.get("include").filter(|v| !v.is_null()) {
            let array = value.as_array().ok_or(FormError::ValueTypeMismatch)?;
            let mut include = Vec::new();
            for value in array {
                let kind = value
                    .as_str()
                    .filter(|s| matches!(*s, "public" | "private" | "mpim" | "im"))
                    .ok_or(FormError::ValueTypeMismatch)?;
                if include.iter().any(|s| s == kind) {
                    return Err(FormError::ValueTypeMismatch);
                }
                include.push(kind.to_owned());
            }
            if !include.is_empty() {
                filter.include = Some(include);
            }
        }
        for (key, target) in [
            ("exclude_bot_users", &mut filter.exclude_bot_users),
            (
                "exclude_external_shared_channels",
                &mut filter.exclude_external_shared_channels,
            ),
        ] {
            if let Some(value) = object.get(key).filter(|v| !v.is_null()) {
                *target = value.as_bool().ok_or(FormError::ValueTypeMismatch)?;
            }
        }
        if let Some(value) = object.get("only_from_team_id").filter(|v| !v.is_null()) {
            filter.only_from_team_id = Some(
                value
                    .as_str()
                    .filter(|s| !s.is_empty() && s.bytes().all(|c| c.is_ascii_alphanumeric()))
                    .ok_or(FormError::ValueTypeMismatch)?
                    .into(),
            );
        }
        Ok(filter)
    }
    pub fn allows(&self, kind: &str) -> bool {
        self.include
            .as_ref()
            .is_none_or(|include| include.iter().any(|s| s == kind))
    }
}
