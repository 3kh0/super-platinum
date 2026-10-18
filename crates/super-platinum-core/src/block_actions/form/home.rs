//! Home interactive state uses the actual input/action/accessory identities.
//! It is not a synthetic modal or a views.submit permission.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dispatch {
    Quiet,
    Change,
    Enter,
    Character,
    EnterOrCharacter,
}
#[derive(Clone, Debug, PartialEq)]
pub struct HomeForm {
    pub fields: BTreeMap<InputKey, FormField>,
    pub unsupported: Vec<String>,
    dispatch: BTreeMap<InputKey, Dispatch>,
    view: Value,
    wire: BTreeMap<InputKey, Value>,
    native_errors: BTreeMap<InputKey, String>,
}
fn invalid() -> FormError {
    FormError::InvalidView("Malformed Home input".into())
}
fn id(v: &Value) -> Result<String, FormError> {
    v.as_str()
        .filter(|s| !s.trim().is_empty() && s.len() <= 255)
        .map(str::to_owned)
        .ok_or_else(invalid)
}
fn plain(v: Option<&Value>) -> Result<Option<String>, FormError> {
    match v {
        None => Ok(None),
        Some(v) if v["type"] == "plain_text" => v["text"]
            .as_str()
            .map(|s| Some(s.into()))
            .ok_or_else(invalid),
        _ => Err(invalid()),
    }
}
impl HomeForm {
    pub fn parse(view: &Value) -> Result<Self, FormError> {
        if view["type"] != "home" {
            return Err(invalid());
        }
        id(&view["id"])?;
        let blocks = view["blocks"]
            .as_array()
            .filter(|b| b.len() <= 100)
            .ok_or_else(invalid)?;
        let mut out = Self {
            fields: BTreeMap::new(),
            unsupported: vec![],
            dispatch: BTreeMap::new(),
            view: view.clone(),
            wire: BTreeMap::new(),
            native_errors: BTreeMap::new(),
        };
        let mut seen = std::collections::BTreeSet::new();
        for block in blocks {
            if let Some(k) = block.get("block_id") {
                if !seen.insert(id(k)?) {
                    return Err(invalid());
                }
            }
            let kind = block["type"].as_str().ok_or_else(invalid)?;
            let elements: Vec<&Value> = match kind {
                "input" => vec![block.get("element").ok_or_else(invalid)?],
                "section" => block.get("accessory").into_iter().collect(),
                "actions" => block["elements"]
                    .as_array()
                    .ok_or_else(invalid)?
                    .iter()
                    .collect(),
                "header" | "context" | "image" | "divider" | "rich_text" => vec![],
                other => {
                    out.unsupported.push(other.into());
                    vec![]
                }
            };
            for element in elements {
                let wire = element["type"].as_str().ok_or_else(invalid)?;
                if matches!(wire, "button" | "workflow_button" | "image") {
                    continue;
                }
                let key = InputKey {
                    block_id: id(&block["block_id"])?,
                    action_id: id(&element["action_id"])?,
                };
                if out.fields.contains_key(&key) {
                    return Err(invalid());
                }
                let (kind, initial_value) = parse::input(element, true, None, None)?;
                if let InputKind::Unsupported { kind } = &kind {
                    out.unsupported.push(kind.clone())
                }
                if matches!(
                    kind,
                    InputKind::ExternalSelect { .. }
                        | InputKind::MultiExternalSelect { .. }
                        | InputKind::User
                        | InputKind::MultiUser { .. }
                        | InputKind::Channel
                        | InputKind::MultiChannel { .. }
                        | InputKind::Conversation
                        | InputKind::MultiConversation { .. }
                ) {
                    out.unsupported.push(
                        "Home lookup controls require their own account/view-bound choices".into(),
                    );
                }
                // Existing source-supported typed parser remains authority for shape/initial
                // selection. Interactive eligibility/remote option lookup is renderer-owned.
                let optional = block
                    .get("optional")
                    .map_or(Ok(false), |v| v.as_bool().ok_or_else(invalid))?;
                let label = plain(block.get("label"))?
                    .or_else(|| element["accessibility_label"].as_str().map(str::to_owned))
                    .unwrap_or_else(|| key.action_id.clone());
                let placeholder = plain(element.get("placeholder").filter(|v| !v.is_null()))?;
                let mut field = FormField {
                    label,
                    hint: plain(block.get("hint"))?,
                    placeholder,
                    optional,
                    kind,
                    initial_value: initial_value.clone(),
                    value: initial_value,
                    dirty: false,
                    error: None,
                    revision: 0,
                    raw: element.clone(),
                };
                field.error = values::error(&field);
                let mode = dispatch(block, element, &field.kind)?;
                if mode != Dispatch::Quiet {
                    out.unsupported.push(
                        "Dispatching Home controls require their owned action lifecycle".into(),
                    );
                }
                if element.get("confirm").is_some() || element.get("visible_to_user_ids").is_some()
                {
                    out.unsupported
                        .push("Input confirmation/visibility requires its owned interaction".into())
                }
                out.dispatch.insert(key.clone(), mode);
                out.fields.insert(key, field);
            }
        }
        if let Some(state) = view.get("state") {
            let values = state["values"].as_object().ok_or_else(invalid)?;
            for (block, actions) in values {
                for (action, saved) in actions.as_object().ok_or_else(invalid)? {
                    let key = InputKey {
                        block_id: block.clone(),
                        action_id: action.clone(),
                    };
                    let Some(field) = out.fields.get_mut(&key) else {
                        out.unsupported.push("Unmodeled Home state".into());
                        continue;
                    };
                    let nullable = match field.kind {
                        InputKind::ExternalSelect { .. } => Some("selected_option"),
                        InputKind::MultiExternalSelect { .. } => Some("selected_options"),
                        InputKind::MultiChannel { .. } => Some("selected_channels"),
                        InputKind::MultiConversation { .. } => Some("selected_conversations"),
                        _ => None,
                    };
                    if nullable.is_some_and(|k| saved.get(k).is_none_or(Value::is_null))
                        && saved["type"] == values::wire_type(&field.kind)
                    {
                        continue;
                    }
                    field.value = values::from_wire(&field.kind, saved)?;
                    field.initial_value = field.value.clone();
                    field.error = values::error(field);
                }
            }
        }
        for (key, field) in &out.fields {
            if valid_state_value(field) {
                out.wire.insert(key.clone(), values::wire(field));
            }
        }
        Ok(out)
    }
    pub fn raw_view(&self) -> &Value {
        &self.view
    }
    pub fn dirty(&self) -> bool {
        self.fields.values().any(|f| f.dirty)
    }
    pub fn native_error_text(&self, key: &InputKey) -> Option<&str> {
        self.native_errors.get(key).map(String::as_str)
    }
    pub fn set_native_error(
        &mut self,
        key: &InputKey,
        text: String,
        error: String,
    ) -> Result<(), FormError> {
        let field = self.fields.get_mut(key).ok_or(FormError::UnknownField)?;
        field.error = Some(error);
        field.dirty = true;
        self.native_errors.insert(key.clone(), text);
        Ok(())
    }
    pub fn reconcile(&mut self, view: &Value) -> Result<(), FormError> {
        if !self.native_errors.is_empty() {
            return Err(FormError::ConflictingUpdate);
        }
        let mut next = Self::parse(view)?;
        if next.view["id"] != self.view["id"] {
            return Err(FormError::ConflictingUpdate);
        }
        for (key, old) in &self.fields {
            if !old.dirty {
                continue;
            }
            let Some(new) = next.fields.get_mut(key) else {
                return Err(FormError::ConflictingUpdate);
            };
            if values::wire_type(&old.kind) != values::wire_type(&new.kind) {
                return Err(FormError::ConflictingUpdate);
            }
            new.value = old.value.clone();
            new.dirty = true;
            new.revision = old.revision;
            new.error = values::error(new);
            if let Some(wire) = self.wire.get(key) {
                next.wire.insert(key.clone(), wire.clone());
            }
        }
        *self = next;
        Ok(())
    }
    pub fn dispatch(&self, key: &InputKey) -> Option<Dispatch> {
        self.dispatch.get(key).copied()
    }
    pub fn set(&mut self, key: &InputKey, value: InputValue) -> Result<(), FormError> {
        let f = self.fields.get_mut(key).ok_or(FormError::UnknownField)?;
        if !values::matches(&f.kind, &value) {
            return Err(FormError::ValueTypeMismatch);
        }
        self.native_errors.remove(key);
        f.value = value;
        f.dirty = true;
        f.error = values::error(f);
        // Keep invalid editing state, but never serialize a stale option or
        // leave an earlier valid wire value available to an action.
        if !valid_state_value(f) {
            self.wire.remove(key);
            return Ok(());
        }
        let mut wire = values::wire(f);
        // Actual PlainTextInputElement.handleChange trims its emitted value while
        // retaining literal native editing text. Enter emits the literal value.
        if let InputValue::Text(text) = &f.value {
            let emitted = trim_input(text);
            wire["value"] = if emitted.is_empty() {
                Value::Null
            } else {
                json!(emitted)
            };
        }
        self.wire.insert(key.clone(), wire);
        Ok(())
    }
    /// blocks.actions carries the whole state, even if a required input is empty.
    /// This is deliberately not modal submission validation.
    pub fn state(&self) -> Result<Value, FormError> {
        if !self.native_errors.is_empty() {
            return Err(FormError::ConflictingUpdate);
        }
        if !self.unsupported.is_empty() {
            return Err(FormError::Unsupported(self.unsupported.clone()));
        }
        if self.fields.values().any(|f| f.error != values::error(f)) {
            return Err(FormError::ConflictingUpdate);
        }
        for field in self.fields.values() {
            // Requiredness belongs to app workflow validation, but invalid
            // retained option/date identities cannot turn into another value.
            if !valid_state_value(field) {
                return Err(FormError::ConflictingUpdate);
            }
        }
        let mut blocks = serde_json::Map::new();
        for (k, f) in &self.fields {
            blocks
                .entry(k.block_id.clone())
                .or_insert_with(|| json!({}))
                .as_object_mut()
                .unwrap()
                .insert(
                    k.action_id.clone(),
                    self.wire.get(k).cloned().unwrap_or_else(|| values::wire(f)),
                );
        }
        Ok(json!({"values":blocks}))
    }
    pub fn action(&self, key: &InputKey) -> Result<Value, FormError> {
        self.state()?;
        let f = self.fields.get(key).ok_or(FormError::UnknownField)?;
        let mut action = self
            .wire
            .get(key)
            .cloned()
            .unwrap_or_else(|| values::wire(f));
        action["block_id"] = json!(key.block_id);
        action["action_id"] = json!(key.action_id);
        Ok(action)
    }
}
// Requiredness is app validation, while invalid non-text identities must
// never reach the serializer, which expects canonical options.
fn valid_state_value(field: &FormField) -> bool {
    if matches!(field.kind, InputKind::Text { .. }) {
        return true;
    }
    let mut optional = field.clone();
    optional.optional = true;
    values::error(&optional).is_none()
}
// ECMAScript String.trim used by the captured Home text callback. Rust's
// whitespace set differs: U+FEFF is removed there, while U+0085 is retained.
fn trim_input(text: &str) -> &str {
    text.trim_matches(|c| {
        matches!(c,
            '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' |
            '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' |
            '\u{205f}' | '\u{3000}' | '\u{feff}'
        )
    })
}
fn dispatch(block: &Value, element: &Value, kind: &InputKind) -> Result<Dispatch, FormError> {
    let quiet = block["type"] == "input"
        && block
            .get("dispatch_action")
            .map_or(Ok(false), |v| v.as_bool().ok_or_else(invalid))?
            == false;
    if quiet {
        return Ok(Dispatch::Quiet);
    }
    if !matches!(kind, InputKind::Text { .. }) {
        return Ok(Dispatch::Change);
    }
    let Some(config) = element.get("dispatch_action_config") else {
        return Ok(Dispatch::Enter);
    };
    let triggers = config["trigger_actions_on"]
        .as_array()
        .filter(|v| !v.is_empty())
        .ok_or_else(invalid)?;
    let (mut enter, mut character) = (false, false);
    for t in triggers {
        match t.as_str() {
            Some("on_enter_pressed") => enter = true,
            Some("on_character_entered") => character = true,
            _ => return Err(invalid()),
        }
    }
    Ok(match (enter, character) {
        (true, true) => Dispatch::EnterOrCharacter,
        (true, false) => Dispatch::Enter,
        (false, true) => Dispatch::Character,
        _ => return Err(invalid()),
    })
}

#[cfg(test)]
mod tests;
