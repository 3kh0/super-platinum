//! Home interactive state uses the actual input/action/accessory identities.
//! It is not a synthetic modal or a views.submit permission.
use super::*;
mod channels;
mod checkboxes;
mod conversations;
mod date;
mod datetime;
mod email;
mod external;
mod number;
mod radio;
mod static_select;
mod text;
mod time;
mod users;
pub use channels::{ChannelParent, ChannelTeamPolicy};
pub use checkboxes::CheckboxSelection;
pub use conversations::ConversationParent;
pub use date::DateParent;
pub use datetime::DatetimeParent;
pub use email::{EmailMount, EmailParent, EmailTrigger};
pub use external::ExternalParent;
pub use number::{NumberParent, NumberTrigger};
pub use static_select::StaticEvent;
pub use text::{TextEvent, TextTrigger};
pub use time::TimeParent;
pub use users::UserParent;
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
                // QjSI ignores default_to_current_conversation; Home has no viewed
                // provider. Keep Modal's origin-aware parser unchanged.
                let (kind, initial_value) = if element["type"] == "email_text_input" {
                    email::parse(element)?
                } else if element["type"] == "number_input" {
                    number::parse(element)?
                } else if element["type"] == "multi_conversations_select" {
                    let field = parse_message_conversations(element)?;
                    (field.kind, field.initial_value)
                } else {
                    parse::input(element, true, None, None)?
                };
                if let InputKind::Unsupported { kind } = &kind {
                    out.unsupported.push(kind.clone())
                }
                if matches!(kind, InputKind::Checkboxes { .. })
                    && !checkboxes::supported(block, element, &kind)
                {
                    out.unsupported
                        .push("Home checkboxes require supported input option rows".into());
                }
                if matches!(kind, InputKind::Email) && !email::supported(block, element, &kind) {
                    out.unsupported
                        .push("Home email requires a supported input".into());
                }
                if matches!(kind, InputKind::Number { .. })
                    && !number::supported(block, element, &kind)
                {
                    out.unsupported
                        .push("Home number requires a supported input".into());
                }
                if matches!(kind, InputKind::DateTime)
                    && !datetime::supported(block, element, &kind)
                {
                    out.unsupported
                        .push("Home datetime requires a supported input".into());
                }
                if matches!(kind, InputKind::Radio { .. })
                    && !radio::supported(block, element, &kind)
                {
                    out.unsupported
                        .push("Home radio buttons require supported input option rows".into());
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
                ) && !external::supported(block, element)
                    && !users::supported(block, element)
                    && !channels::supported(block, element)
                    && !conversations::supported(block, element)
                {
                    out.unsupported.push(
                        "Home lookup controls require their own account/view-bound choices".into(),
                    );
                }
                // Existing source-supported typed parser remains authority for shape/initial
                // selection. Interactive eligibility/remote option lookup is renderer-owned.
                let optional = block
                    .get("optional")
                    .map_or(Ok(false), |v| v.as_bool().ok_or_else(invalid))?
                    || (block["type"] != "input"
                        && static_select::supported(block, element, &kind));
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
                field.error = home_error(&field);
                let mode = dispatch(block, element, &field.kind)?;
                if mode != Dispatch::Quiet
                    && !text::supported(block, element, &field.kind)
                    && !number::supported(block, element, &field.kind)
                    && !email::supported(block, element, &field.kind)
                    && !static_select::supported(block, element, &field.kind)
                    && !checkboxes::supported(block, element, &field.kind)
                    && !radio::supported(block, element, &field.kind)
                    && !date::supported(block, element, &field.kind)
                    && !time::supported(block, element, &field.kind)
                    && !datetime::supported(block, element, &field.kind)
                    && !users::supported(block, element)
                    && !(matches!(
                        field.kind,
                        InputKind::ExternalSelect { .. } | InputKind::MultiExternalSelect { .. }
                    ) && external::supported(block, element))
                    && !(matches!(
                        field.kind,
                        InputKind::Channel | InputKind::MultiChannel { .. }
                    ) && channels::supported(block, element))
                    && !(matches!(
                        field.kind,
                        InputKind::Conversation | InputKind::MultiConversation { .. }
                    ) && conversations::supported(block, element))
                {
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
                    field.error = home_error(field);
                }
            }
        }
        for (key, field) in &out.fields {
            if valid_state_value(field) {
                out.wire.insert(key.clone(), values::wire(field));
            }
        }
        out.initialize_number_wire();
        out.initialize_external_wire();
        out.initialize_datetime_wire();
        out.initialize_user_wire();
        out.initialize_channel_wire();
        out.initialize_conversation_wire();
        out.refresh_text_errors();
        out.refresh_checkbox_errors();
        out.refresh_radio_errors();
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
        self.reconcile_with_date_parents(view, &[])
    }
    /// Only the renderer can attest which date fields remain initialized in its
    /// current owned visit. Ordinary reconciliation retains its prior policy.
    pub fn reconcile_with_date_parents(
        &mut self,
        view: &Value,
        parents: &[DateParent],
    ) -> Result<(), FormError> {
        self.reconcile_with_calendar_parents(view, parents, &[])
    }
    /// Retain only current renderer-attested Date/Time parents. Tokens carry
    /// identity, never a captured value; latest trusted parent state wins.
    pub fn reconcile_with_calendar_parents(
        &mut self,
        view: &Value,
        dates: &[DateParent],
        times: &[TimeParent],
    ) -> Result<(), FormError> {
        self.reconcile_with_all_calendar_parents(view, dates, times, &[])
    }
    /// Initialized composite parents remain independent of changed source props.
    /// Existing Date/Time callers retain their exact reconciliation behavior.
    pub fn reconcile_with_all_calendar_parents(
        &mut self,
        view: &Value,
        dates: &[DateParent],
        times: &[TimeParent],
        datetimes: &[DatetimeParent],
    ) -> Result<(), FormError> {
        self.reconcile_with_initialized_parents(view, dates, times, datetimes, &[])
    }
    /// Preserve only parents attested by initialized renderer-owned controls.
    /// Tokens bind identity; the latest private parent wire remains authoritative.
    pub fn reconcile_with_initialized_parents(
        &mut self,
        view: &Value,
        dates: &[DateParent],
        times: &[TimeParent],
        datetimes: &[DatetimeParent],
        users: &[UserParent],
    ) -> Result<(), FormError> {
        self.reconcile_with_lookup_parents(view, dates, times, datetimes, users, &[])
    }
    /// Add initialized single/multi-channel parents without changing existing callers.
    pub fn reconcile_with_lookup_parents(
        &mut self,
        view: &Value,
        dates: &[DateParent],
        times: &[TimeParent],
        datetimes: &[DatetimeParent],
        users: &[UserParent],
        channels: &[ChannelParent],
    ) -> Result<(), FormError> {
        self.reconcile_with_all_lookup_parents(view, dates, times, datetimes, users, channels, &[])
    }
    /// Add initialized conversation parents without widening Channel tokens.
    pub fn reconcile_with_all_lookup_parents(
        &mut self,
        view: &Value,
        dates: &[DateParent],
        times: &[TimeParent],
        datetimes: &[DatetimeParent],
        users: &[UserParent],
        channels: &[ChannelParent],
        conversations: &[ConversationParent],
    ) -> Result<(), FormError> {
        self.reconcile_with_external_parents(
            view,
            dates,
            times,
            datetimes,
            users,
            channels,
            conversations,
            &[],
        )
    }
    /// Add initialized kind-bound external parents without changing existing callers.
    /// Tokens attest identity, not a captured descriptor or query authority.
    pub fn reconcile_with_external_parents(
        &mut self,
        view: &Value,
        dates: &[DateParent],
        times: &[TimeParent],
        datetimes: &[DatetimeParent],
        users: &[UserParent],
        channels: &[ChannelParent],
        conversations: &[ConversationParent],
        externals: &[ExternalParent],
    ) -> Result<(), FormError> {
        self.reconcile_with_number_parents(
            view,
            dates,
            times,
            datetimes,
            users,
            channels,
            conversations,
            externals,
            &[],
        )
    }
    /// Retain initialized numeric parents independently of mounted source props.
    pub fn reconcile_with_number_parents(
        &mut self,
        view: &Value,
        dates: &[DateParent],
        times: &[TimeParent],
        datetimes: &[DatetimeParent],
        users: &[UserParent],
        channels: &[ChannelParent],
        conversations: &[ConversationParent],
        externals: &[ExternalParent],
        numbers: &[NumberParent],
    ) -> Result<(), FormError> {
        self.reconcile_with_email_parents(
            view,
            dates,
            times,
            datetimes,
            users,
            channels,
            conversations,
            externals,
            numbers,
            &[],
        )
    }
    /// Retain initialized email parents; fresh widget mount effects remain explicit.
    pub fn reconcile_with_email_parents(
        &mut self,
        view: &Value,
        dates: &[DateParent],
        times: &[TimeParent],
        datetimes: &[DatetimeParent],
        users: &[UserParent],
        channels: &[ChannelParent],
        conversations: &[ConversationParent],
        externals: &[ExternalParent],
        numbers: &[NumberParent],
        emails: &[EmailParent],
    ) -> Result<(), FormError> {
        if !self.native_errors.is_empty() {
            return Err(FormError::ConflictingUpdate);
        }
        let mut next = Self::parse(view)?;
        if next.view["id"] != self.view["id"] {
            return Err(FormError::ConflictingUpdate);
        }
        let mut calendar_keys = self.date_parent_keys(&next, dates)?;
        calendar_keys.extend(self.time_parent_keys(&next, times)?);
        calendar_keys.extend(self.datetime_parent_keys(&next, datetimes)?);
        calendar_keys.extend(self.user_parent_keys(&next, users)?);
        calendar_keys.extend(self.channel_parent_keys(&next, channels)?);
        calendar_keys.extend(self.conversation_parent_keys(&next, conversations)?);
        calendar_keys.extend(self.external_parent_keys(&next, externals)?);
        calendar_keys.extend(self.number_parent_keys(&next, numbers)?);
        calendar_keys.extend(self.email_parent_keys(&next, emails)?);
        for (key, old) in &self.fields {
            if !old.dirty && !calendar_keys.contains(key) {
                continue;
            }
            let Some(new) = next.fields.get_mut(key) else {
                return Err(FormError::ConflictingUpdate);
            };
            if values::wire_type(&old.kind) != values::wire_type(&new.kind) {
                return Err(FormError::ConflictingUpdate);
            }
            if matches!(old.kind, InputKind::Checkboxes { .. })
                && (!self.checkbox_state_valid(key)
                    || checkboxes::owner(&self.view) != checkboxes::owner(&next.view))
            {
                return Err(FormError::ConflictingUpdate);
            }
            if matches!(old.kind, InputKind::Radio { .. })
                && (!self.radio_state_valid(key)
                    || checkboxes::owner(&self.view) != checkboxes::owner(&next.view))
            {
                return Err(FormError::ConflictingUpdate);
            }
            if matches!(old.kind, InputKind::DateTime)
                && (!self.datetime_state_valid(key)
                    || checkboxes::owner(&self.view) != checkboxes::owner(&next.view))
            {
                return Err(FormError::ConflictingUpdate);
            }
            if matches!(
                old.kind,
                InputKind::Conversation | InputKind::MultiConversation { .. }
            ) && (!self.conversation_state_valid(key)
                || checkboxes::owner(&self.view) != checkboxes::owner(&next.view))
            {
                return Err(FormError::ConflictingUpdate);
            }
            if matches!(old.kind, InputKind::User | InputKind::MultiUser { .. })
                && (!self.user_state_valid(key)
                    || checkboxes::owner(&self.view) != checkboxes::owner(&next.view))
            {
                return Err(FormError::ConflictingUpdate);
            }
            if matches!(
                old.kind,
                InputKind::Channel | InputKind::MultiChannel { .. }
            ) && (!self.channel_state_valid(key)
                || checkboxes::owner(&self.view) != checkboxes::owner(&next.view))
            {
                return Err(FormError::ConflictingUpdate);
            }
            if matches!(
                old.kind,
                InputKind::ExternalSelect { .. } | InputKind::MultiExternalSelect { .. }
            ) && (!self.external_state_valid(key)
                || checkboxes::owner(&self.view) != checkboxes::owner(&next.view))
            {
                return Err(FormError::ConflictingUpdate);
            }
            if matches!(old.kind, InputKind::Number { .. })
                && (!self.number_state_valid(key)
                    || checkboxes::owner(&self.view) != checkboxes::owner(&next.view))
            {
                return Err(FormError::ConflictingUpdate);
            }
            if matches!(old.kind, InputKind::Email)
                && (!self.email_state_valid(key)
                    || checkboxes::owner(&self.view) != checkboxes::owner(&next.view))
            {
                return Err(FormError::ConflictingUpdate);
            }
            new.value = old.value.clone();
            new.dirty = old.dirty;
            new.revision = old.revision;
            new.error = home_error(new);
            if let Some(wire) = self.wire.get(key) {
                next.wire.insert(key.clone(), wire.clone());
            }
        }
        next.refresh_text_errors();
        next.refresh_checkbox_errors();
        next.refresh_radio_errors();
        *self = next;
        Ok(())
    }
    pub fn dispatch(&self, key: &InputKey) -> Option<Dispatch> {
        self.dispatch.get(key).copied()
    }
    pub fn set(&mut self, key: &InputKey, value: InputValue) -> Result<(), FormError> {
        if matches!(
            self.fields.get(key).map(|f| &f.kind),
            Some(InputKind::Email)
        ) {
            let InputValue::Email(Some(value)) = value else {
                return Err(FormError::ValueTypeMismatch);
            };
            self.email_store(key, value)?;
            return Ok(());
        }
        if matches!(
            self.fields.get(key).map(|f| &f.kind),
            Some(InputKind::Number { .. })
        ) {
            let InputValue::Number(value) = value else {
                return Err(FormError::ValueTypeMismatch);
            };
            self.number_store(key, value)?;
            return Ok(());
        }
        if self
            .dispatch(key)
            .is_some_and(|mode| mode != Dispatch::Quiet)
            && matches!(
                self.fields.get(key).map(|f| &f.kind),
                Some(InputKind::Text { .. })
            )
        {
            let InputValue::Text(literal) = value else {
                return Err(FormError::ValueTypeMismatch);
            };
            self.text_input(key, literal, TextTrigger::Change)?;
            return Ok(());
        }
        let f = self.fields.get_mut(key).ok_or(FormError::UnknownField)?;
        if !values::matches(&f.kind, &value) {
            return Err(FormError::ValueTypeMismatch);
        }
        self.native_errors.remove(key);
        f.value = value;
        f.dirty = true;
        f.error = home_error(f);
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
        if !self.checkbox_state_inventory_valid() || !self.radio_state_inventory_valid() {
            return Err(FormError::ConflictingUpdate);
        }
        if !self.date_state_inventory_valid()
            || !self.time_state_inventory_valid()
            || !self.datetime_state_inventory_valid()
            || !self.user_state_inventory_valid()
            || !self.channel_state_inventory_valid()
            || !self.conversation_state_inventory_valid()
            || !self.external_state_inventory_valid()
            || !self.number_state_inventory_valid()
            || !self.email_state_inventory_valid()
        {
            return Err(FormError::ConflictingUpdate);
        }
        if self
            .fields
            .iter()
            .any(|(key, f)| f.error != self.expected_error(key, f))
        {
            return Err(FormError::ConflictingUpdate);
        }
        for (key, field) in &self.fields {
            // Requiredness belongs to app workflow validation, but invalid
            // retained option/date identities cannot turn into another value.
            if if matches!(field.kind, InputKind::Checkboxes { .. }) {
                !self.checkbox_state_valid(key)
            } else if matches!(field.kind, InputKind::Radio { .. }) {
                !self.radio_state_valid(key)
            } else {
                !valid_state_value(field)
            } {
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
    if matches!(field.kind, InputKind::Email) {
        return matches!(field.value, InputValue::Email(_));
    }
    if matches!(field.kind, InputKind::Number { .. }) {
        return matches!(&field.value, InputValue::Number(v) if number::valid_value(v.as_deref()));
    }
    if matches!(field.kind, InputKind::Text { .. }) {
        return true;
    }
    let mut optional = field.clone();
    optional.optional = true;
    // Home Buttons carry committed state, including an initial selection or a
    // selection retained after the maximum changes. hGZA, z+fW and QjSI also emit
    // over-limit user/channel/conversation edits for input blocks, stored before validation.
    // etBz does the same for multi-static inputs: a previously queued valid
    // action carries this latest parent state even when the new edit is invalid.
    // Multi-external widget edits that did not emit never enter here.
    // Keep the shared type, duplicate and bounded-array validation intact.
    if let InputKind::MultiExternalSelect {
        max_selected_items, ..
    }
    | InputKind::MultiUser { max_selected_items }
    | InputKind::MultiChannel { max_selected_items }
    | InputKind::MultiConversation { max_selected_items }
    | InputKind::StaticSelect {
        multiple: true,
        max_selected_items,
        ..
    } = &mut optional.kind
    {
        *max_selected_items = None;
    }
    home_error(&optional).is_none()
}
fn home_error(field: &FormField) -> Option<String> {
    if matches!(field.kind, InputKind::Email) {
        email::error(field)
    } else if matches!(field.kind, InputKind::Number { .. }) {
        number::error(field)
    } else if matches!(field.kind, InputKind::DateTime) {
        datetime::error(field)
    } else {
        values::error(field)
    }
}
// Shared EZY9/+Key/2Ztu scalar validation; no Home state/mount capability.
pub(super) fn email_error(field: &FormField) -> Option<String> {
    email::error(field)
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
    if matches!(kind, InputKind::Email) {
        return email::dispatch(element);
    }
    if !matches!(kind, InputKind::Text { .. } | InputKind::Number { .. }) {
        return Ok(Dispatch::Change);
    }
    let Some(config) = element.get("dispatch_action_config") else {
        return Ok(Dispatch::Enter);
    };
    let triggers = config["trigger_actions_on"]
        .as_array()
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
        (false, false) if matches!(kind, InputKind::Number { .. }) => Dispatch::Change,
        // VaQU still dispatches Enter with an empty trigger array.
        _ => Dispatch::Enter,
    })
}

#[cfg(test)]
mod tests;
