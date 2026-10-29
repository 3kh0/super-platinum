use super::*;
use std::collections::HashSet;

fn invalid(message: &str) -> FormError {
    FormError::InvalidView(message.into())
}
fn id(value: Option<&Value>) -> Result<String, FormError> {
    value
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty() && s.len() <= 255)
        .map(str::to_owned)
        .ok_or_else(|| invalid("Missing or invalid field identity"))
}
fn text(value: Option<&Value>) -> Result<Option<String>, FormError> {
    let Some(value) = value else { return Ok(None) };
    if value.get("type").and_then(Value::as_str) != Some("plain_text") {
        return Err(invalid("Expected plain text composition"));
    }
    let raw = value
        .get("text")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("Missing label text"))?;
    Ok(Some(
        crate::state::emoji_text_to_display(raw)
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&"),
    ))
}
fn boolean(value: &Value, key: &str, default: bool) -> Result<bool, FormError> {
    value.get(key).map_or(Ok(default), |v| {
        v.as_bool()
            .ok_or_else(|| invalid("Invalid boolean setting"))
    })
}
fn number(value: &Value, key: &str) -> Result<Option<usize>, FormError> {
    value
        .get(key)
        .map(|v| {
            v.as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .ok_or_else(|| invalid("Invalid numeric constraint"))
        })
        .transpose()
}
fn optional_string(value: &Value, key: &str) -> Result<Option<String>, FormError> {
    value
        .get(key)
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| invalid("Invalid initial text"))
        })
        .transpose()
}

pub(super) fn form(
    view: &Value,
    origin_conversation: Option<&str>,
    viewed_conversation: Option<&ViewedConversation>,
    inputs: ScalarInputs,
    retained: Option<&ModalForm>,
) -> Result<ModalForm, FormError> {
    form_with_introductions(
        view,
        origin_conversation,
        viewed_conversation,
        inputs,
        retained,
        None,
    )
}

pub(super) fn form_with_introductions(
    view: &Value,
    origin_conversation: Option<&str>,
    viewed_conversation: Option<&ViewedConversation>,
    inputs: ScalarInputs,
    retained: Option<&ModalForm>,
    grant: Option<&super::super::modal::integer_introductions::Grant<'_>>,
) -> Result<ModalForm, FormError> {
    if origin_conversation.is_some_and(|id| !valid_message_conversation(id)) {
        return Err(invalid("Invalid originating conversation"));
    }
    if !matches!(
        view.get("type").and_then(Value::as_str),
        Some("modal" | "app_notification_subscription_configuration")
    ) {
        return Err(invalid("Expected modal view"));
    }
    let view_id = id(view.get("id"))?;
    let title = text(view.get("title"))?.ok_or_else(|| invalid("Missing title"))?;
    let blocks = view
        .get("blocks")
        .and_then(Value::as_array)
        .filter(|v| v.len() <= 100)
        .ok_or_else(|| invalid("Missing or excessive blocks"))?;
    let mut form = ModalForm {
        view_id,
        title,
        submit_label: text(view.get("submit"))?,
        close_label: text(view.get("close"))?,
        blocks: vec![],
        fields: BTreeMap::new(),
        unsupported: vec![],
        raw_view: view.clone(),
        origin_conversation: origin_conversation.map(str::to_owned),
        viewed_conversation: viewed_conversation.cloned(),
        inputs,
        url_prefill_keys: BTreeSet::new(),
        url_prefill_history: if inputs.url_prefill {
            retained
                .map(|old| old.url_prefill_history.clone())
                .unwrap_or_default()
        } else {
            BTreeSet::new()
        },
        integer_keys: BTreeSet::new(),
        introduced: retained
            .map(|old| old.introduced.clone())
            .unwrap_or_default(),
    };
    let mut block_ids = HashSet::new();
    for block in blocks {
        let kind = block
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("Missing block type"))?;
        if let Some(block_id) = block.get("block_id") {
            if !block_ids.insert(id(Some(block_id))?) {
                return Err(invalid("Duplicate block identity"));
            }
        }
        if kind != "input" {
            // All stateful section/actions elements need state ownership too.
            // They remain visible, but block submission until implemented.
            find_stateful(block, &mut form.unsupported);
            form.blocks.push(FormBlock::Display(block.clone()));
            continue;
        }
        let element = block
            .get("element")
            .filter(|v| v.is_object())
            .ok_or_else(|| invalid("Missing input element"))?;
        let key = InputKey {
            block_id: id(block.get("block_id"))?,
            action_id: id(element.get("action_id"))?,
        };
        let (kind, initial_value) =
            if inputs.email && view["type"] == "modal" && element["type"] == "email_text_input" {
                if !modal_email::supported(element)? {
                    form.unsupported
                        .push("Prefilled email inputs are not supported yet".into());
                }
                (InputKind::Email, InputValue::Email(None))
            } else if inputs.url && view["type"] == "modal" && element["type"] == "url_text_input" {
                let initial = modal_url::initial(element)?;
                let previous = retained.and_then(|old| old.fields.get(&key));
                let kept = inputs.url_prefill
                    && retained.is_some_and(|old| {
                        old.url_prefill_keys.contains(&key)
                            && previous.is_some_and(|field| matches!(field.kind, InputKind::Url))
                    });
                let saved = view
                    .get("state")
                    .and_then(|state| state.get("values"))
                    .and_then(|values| values.get(key.block_id.as_str()))
                    .and_then(|actions| actions.get(key.action_id.as_str()))
                    .is_some();
                // Initial owned form only. Model key absence after an update
                // does not prove the old widget was disposed before this render.
                let admitted = inputs.url_prefill && (kept || (retained.is_none() && !saved));
                let recreated = !kept && form.url_prefill_history.contains(&key);
                if (initial.is_some() && !admitted) || recreated {
                    form.unsupported
                        .push("Prefilled URL inputs are not supported yet".into());
                }
                if kept || (initial.is_some() && admitted) {
                    form.url_prefill_keys.insert(key.clone());
                    form.url_prefill_history.insert(key.clone());
                }
                (
                    InputKind::Url,
                    InputValue::Url(if admitted { initial } else { None }),
                )
            } else if inputs.required_integer
                && view["type"] == "modal"
                && element["type"] == "number_input"
            {
                let initial = modal_number::initial(element)?;
                let kept = retained.is_some_and(|old| {
                    old.integer_keys.contains(&key)
                        && old
                            .fields
                            .get(&key)
                            .is_some_and(|field| matches!(field.kind, InputKind::Number { .. }))
                });
                let saved = view
                    .get("state")
                    .and_then(|s| s.get("values"))
                    .and_then(|v| v.get(key.block_id.as_str()))
                    .and_then(|v| v.get(key.action_id.as_str()))
                    .is_some();
                let introduced = !kept
                    && !saved
                    && grant.is_some_and(|grant| grant.matches(view) && grant.allows(&key, block))
                    && initial.as_deref().is_some_and(|literal| {
                        if inputs.signed_integer {
                            modal_number::canonical_signed(literal)
                        } else {
                            modal_number::canonical(literal)
                        }
                    });
                let admitted = modal_number::supported(block, element)
                    && (kept
                        || introduced
                        || (retained.is_none()
                            && !saved
                            && initial.as_deref().is_some_and(|literal| {
                                if inputs.signed_integer {
                                    modal_number::canonical_signed(literal)
                                } else {
                                    modal_number::canonical(literal)
                                }
                            })));
                if admitted {
                    if introduced {
                        form.introduced
                            .register(&key, block, grant.expect("checked owner grant"));
                    } else {
                        form.integer_keys.insert(key.clone());
                    }
                } else {
                    form.unsupported.push(
                        "This number input requires an unsupported initialization or setting"
                            .into(),
                    );
                }
                (
                    InputKind::Number {
                        is_decimal_allowed: false,
                        min_value: None,
                        max_value: None,
                    },
                    InputValue::Number(initial),
                )
            } else {
                input(
                    element,
                    view["type"] == "modal",
                    origin_conversation,
                    viewed_conversation,
                )?
            };
        if let InputKind::Unsupported { kind } = &kind {
            form.unsupported.push(kind.clone());
        }
        if matches!(kind, InputKind::Channel) && modal_channel::unsupported_settings(element) {
            form.unsupported
                .push("This channel input requires unsupported selection rules".into());
        }
        // Dispatch-on-change and confirmation require additional interaction
        // flows. Never present those as ordinary submit-only local controls.
        if boolean(block, "dispatch_action", false)?
            || element.get("dispatch_action_config").is_some()
            || element.get("confirm").is_some()
            || element.get("visible_to_user_ids").is_some()
        {
            form.unsupported
                .push("This input requires an unsupported interaction or visibility rule".into());
        }
        let mut field = FormField {
            label: text(block.get("label"))?.ok_or_else(|| invalid("Missing input label"))?,
            hint: text(block.get("hint"))?,
            placeholder: text(element.get("placeholder").filter(|value| {
                !matches!(
                    kind,
                    InputKind::ExternalSelect { .. }
                        | InputKind::MultiExternalSelect { .. }
                        | InputKind::User
                        | InputKind::MultiUser { .. }
                        | InputKind::MultiChannel { .. }
                        | InputKind::MultiConversation { .. }
                        | InputKind::Conversation
                        | InputKind::Channel
                ) || !value.is_null()
            }))?,
            optional: boolean(block, "optional", false)?,
            kind,
            value: initial_value.clone(),
            initial_value,
            dirty: false,
            error: None,
            revision: 0,
            raw: element.clone(),
        };
        field.error = values::error_in(&field, inputs);
        form.fields.insert(key.clone(), field);
        form.blocks.push(FormBlock::Input(key));
    }
    if let Some(state) = view.get("state") {
        let values = state
            .get("values")
            .and_then(Value::as_object)
            .ok_or_else(|| invalid("Malformed view state"))?;
        for (block_id, actions) in values {
            for (action_id, value) in actions
                .as_object()
                .ok_or_else(|| invalid("Malformed block state"))?
            {
                let key = InputKey {
                    block_id: block_id.clone(),
                    action_id: action_id.clone(),
                };
                if let Some(field) = form.fields.get_mut(&key) {
                    // iGIl plus z+fW/QjSI coalesce missing/null saved arrays
                    // to the initial selection. Only an explicit [] is a clear.
                    let saved_key = match field.kind {
                        InputKind::ExternalSelect { .. } => Some("selected_option"),
                        InputKind::MultiExternalSelect { .. } => Some("selected_options"),
                        InputKind::MultiChannel { .. } => Some("selected_channels"),
                        InputKind::MultiConversation { .. } => Some("selected_conversations"),
                        _ => None,
                    };
                    if saved_key.is_some_and(|key| value.get(key).is_none_or(Value::is_null))
                        && value.get("type").and_then(Value::as_str)
                            == Some(values::wire_type(&field.kind))
                    {
                        continue;
                    }
                    let current = values::from_wire(&field.kind, value)?;
                    if inputs.required_integer
                        && matches!(field.kind, InputKind::Number { .. })
                        && retained.is_some_and(|old| {
                            old.integer_keys.contains(&key)
                                && old.fields.get(&key).is_some_and(|f| f.kind == field.kind)
                        })
                    {
                        // Decode first, then retain the existing parent/local row.
                        // Fresh raw saved rows never acquire integer admission.
                        continue;
                    }
                    if matches!(field.kind, InputKind::Email | InputKind::Url) {
                        // iGIl has no saved email/URL projection. A fresh widget
                        // cannot display a nonempty saved parent. Retained
                        // identities keep their owned local row in reconcile;
                        // incoming saved rows must not replace that buffer.
                        if retained.is_some_and(|old| {
                            old.fields.get(&key).is_some_and(|f| f.kind == field.kind)
                        }) {
                            continue;
                        }
                        if matches!(&current, InputValue::Email(Some(v)) | InputValue::Url(Some(v)) if !v.is_empty())
                        {
                            form.unsupported.push(format!(
                                "Restored {} inputs are not supported yet",
                                if matches!(field.kind, InputKind::Email) {
                                    "email"
                                } else {
                                    "URL"
                                }
                            ));
                            continue;
                        }
                    }
                    field.initial_value = current.clone();
                    field.value = current;
                    field.error = values::error_in(field, inputs);
                } else {
                    form.unsupported
                        .push(format!("State for unmodeled field {block_id}/{action_id}"));
                }
            }
        }
    }
    Ok(form)
}

fn find_stateful(value: &Value, unsupported: &mut Vec<String>) {
    if let Some(kind) = value.get("type").and_then(Value::as_str)
        && (kind.ends_with("_select")
            || kind.ends_with("_input")
            || matches!(
                kind,
                "datepicker"
                    | "timepicker"
                    | "datetimepicker"
                    | "checkboxes"
                    | "radio_buttons"
                    | "password"
            ))
    {
        unsupported.push(format!("{kind} outside an input block"));
    }
    if let Some(accessory) = value.get("accessory") {
        find_stateful(accessory, unsupported);
    }
    if let Some(elements) = value.get("elements").and_then(Value::as_array) {
        for element in elements {
            find_stateful(element, unsupported);
        }
    }
}

pub(super) fn input(
    element: &Value,
    ordinary_modal: bool,
    origin_conversation: Option<&str>,
    viewed_conversation: Option<&ViewedConversation>,
) -> Result<(InputKind, InputValue), FormError> {
    let kind = element
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("Missing input type"))?;
    Ok(match kind {
        "multi_external_select" if ordinary_modal => {
            let field = parse_message_external_options(element)?;
            (field.kind, field.initial_value)
        }
        "external_select" if ordinary_modal => {
            let field = parse_message_external(element)?;
            (field.kind, field.initial_value)
        }
        "channels_select" if ordinary_modal => {
            (InputKind::Channel, modal_channel::initial(element)?)
        }
        "conversations_select" if ordinary_modal => {
            // o5++ (B13224329) and AM6q (B1754021) preserve conversation/null
            // and response_url_enabled. The documented default resolves from
            // the conversation at opening, only when no initial is supplied:
            // https://docs.slack.dev/reference/block-kit/block-elements/select-menu-element/
            let field = parse_message_conversation(element)?;
            let initial = match field.initial_value {
                InputValue::Conversation(None)
                    if element.get("default_to_current_conversation")
                        == Some(&Value::Bool(true)) =>
                {
                    InputValue::Conversation(origin_conversation.map(str::to_owned))
                }
                value => value,
            };
            (field.kind, initial)
        }
        "users_select" if ordinary_modal => {
            // HvtU (B13315319) takes input requiredness from 1hVl (B1739929)
            // and emits selectedUser or null. Reuse the message value shape;
            // input.optional remains the modal's requiredness authority.
            let field = parse_message_user(element)?;
            (field.kind, field.initial_value)
        }
        "multi_conversations_select" if ordinary_modal => {
            if element.get("default_to_current_conversation") == Some(&Value::Bool(true)) {
                if let Some(origin) = viewed_conversation {
                    // Official multi-select reference: the viewed conversation
                    // overrides explicit initials. QjSI/iGIl saved arrays (even
                    // []) still win afterwards. Do not modify the raw element.
                    let mut resolved = element.clone();
                    resolved["default_to_current_conversation"] = Value::Bool(false);
                    resolved["initial_conversations"] = json!([origin.id()]);
                    let field = parse_message_conversations(&resolved)?;
                    (field.kind, field.initial_value)
                } else {
                    (
                        InputKind::Unsupported { kind: kind.into() },
                        InputValue::Unsupported,
                    )
                }
            } else {
                // bgmB inline input forwards over-max arrays for submit validation.
                // The input block, not element.required, owns requiredness.
                let field = parse_message_conversations(element)?;
                (field.kind, field.initial_value)
            }
        }
        "multi_channels_select" if ordinary_modal => {
            // xjrl forces inline input selection; z+fW forwards over-max local
            // arrays for submit validation. Requiredness is input.optional.
            let field = parse_message_channels(element)?;
            (field.kind, field.initial_value)
        }
        "multi_users_select" if ordinary_modal => {
            // RBm/ renders normal input-block multi-users inline. hGZA forwards
            // every selection, including an over-limit one, into form state.
            // Keep shared shape/identity validation, but requiredness belongs
            // to input.optional, not the message element's required setting.
            let field = parse_message_users(element)?;
            (field.kind, field.initial_value)
        }
        "plain_text_input" => {
            let min = number(element, "min_length")?;
            let max = number(element, "max_length")?;
            if min.is_some_and(|n| n > 3000)
                || max.is_some_and(|n| n == 0 || n > 3000)
                || min.zip(max).is_some_and(|(a, b)| a > b)
            {
                return Err(invalid("Invalid text length constraints"));
            }
            (
                InputKind::Text {
                    multiline: boolean(element, "multiline", false)?,
                    min_length: min,
                    max_length: max,
                },
                InputValue::Text(optional_string(element, "initial_value")?.unwrap_or_default()),
            )
        }
        "static_select" | "multi_static_select" | "checkboxes" | "radio_buttons" => {
            let options = options(element)?;
            let multiple = matches!(kind, "multi_static_select" | "checkboxes");
            let initial = initial_options(element, multiple, &options)?;
            let kind = match kind {
                "checkboxes" => InputKind::Checkboxes { options },
                "radio_buttons" => InputKind::Radio { options },
                _ => {
                    let max = number(element, "max_selected_items")?;
                    if max == Some(0) {
                        return Err(invalid("Invalid maximum selections"));
                    }
                    InputKind::StaticSelect {
                        options,
                        multiple,
                        max_selected_items: max,
                    }
                }
            };
            (kind, InputValue::Selected(initial))
        }
        "datepicker" => (
            InputKind::Date,
            InputValue::Date(optional_string(element, "initial_date")?),
        ),
        "timepicker" => (
            InputKind::Time {
                timezone: optional_string(element, "timezone")?,
            },
            InputValue::Time(optional_string(element, "initial_time")?),
        ),
        "datetimepicker" => (
            InputKind::DateTime,
            InputValue::DateTime(
                element
                    .get("initial_date_time")
                    .map(|v| {
                        v.as_i64()
                            .ok_or_else(|| invalid("Invalid initial timestamp"))
                    })
                    .transpose()?,
            ),
        ),
        _ => (
            InputKind::Unsupported { kind: kind.into() },
            InputValue::Unsupported,
        ),
    })
}

pub(super) fn options(element: &Value) -> Result<Vec<OptionSpec>, FormError> {
    let mut result = vec![];
    let mut push = |raw: &Value, group: Option<String>| -> Result<(), FormError> {
        let value = raw
            .get("value")
            .and_then(Value::as_str)
            .filter(|v| v.len() <= 2000)
            .ok_or_else(|| invalid("Missing option value"))?
            .to_owned();
        let label = raw
            .get("text")
            .and_then(|v| v.get("text"))
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("Missing option label"))?;
        if result.iter().any(|old: &OptionSpec| old.value == value) {
            return Err(invalid("Duplicate option value"));
        }
        result.push(OptionSpec {
            text: crate::state::emoji_text_to_display(label)
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&amp;", "&"),
            value,
            raw: raw.clone(),
            group,
        });
        Ok(())
    };
    if element.get("options").is_some() && element.get("option_groups").is_some() {
        return Err(invalid("Conflicting option sources"));
    }
    if let Some(options) = element.get("options") {
        for option in options
            .as_array()
            .ok_or_else(|| invalid("Invalid option list"))?
        {
            push(option, None)?;
        }
    } else if let Some(groups) = element.get("option_groups") {
        for group in groups
            .as_array()
            .ok_or_else(|| invalid("Invalid option groups"))?
        {
            let label =
                text(group.get("label"))?.ok_or_else(|| invalid("Missing option group label"))?;
            for option in group
                .get("options")
                .and_then(Value::as_array)
                .ok_or_else(|| invalid("Missing group options"))?
            {
                push(option, Some(label.clone()))?;
            }
        }
    }
    if result.is_empty() || result.len() > 1000 {
        return Err(invalid("Missing or excessive options"));
    }
    Ok(result)
}

pub(super) fn static_select_options(element: &Value) -> Result<Vec<OptionSpec>, FormError> {
    if element.get("type").and_then(Value::as_str) != Some("static_select") {
        return Err(invalid("Expected a single static select"));
    }
    let options = options(element)?;
    for option in &options {
        text(option.raw.get("text"))?.ok_or_else(|| invalid("Missing option label"))?;
        if option.raw.get("description").is_some() {
            text(option.raw.get("description"))?;
        }
    }
    Ok(options)
}

pub(super) fn radio_options(element: &Value) -> Result<Vec<OptionSpec>, FormError> {
    if element.get("type").and_then(Value::as_str) != Some("radio_buttons")
        || element.get("option_groups").is_some()
    {
        return Err(invalid("Expected radio buttons with direct options"));
    }
    let options = options(element)?;
    // Captured gMsa renders both formats with noLinking. Keep raw composition
    // metadata for the action while exposing inert text labels to renderers.
    for option in &options {
        for name in ["text", "description"] {
            let Some(value) = option.raw.get(name) else {
                if name == "text" {
                    return Err(invalid("Missing option label"));
                }
                continue;
            };
            if !matches!(
                value.get("type").and_then(Value::as_str),
                Some("plain_text" | "mrkdwn")
            ) || value.get("text").and_then(Value::as_str).is_none()
            {
                return Err(invalid("Invalid radio option composition"));
            }
        }
    }
    Ok(options)
}

pub(super) fn message_static_select(element: &Value) -> Result<FormField, FormError> {
    let options = static_select_options(element)?;
    let kind = InputKind::StaticSelect {
        options,
        multiple: false,
        max_selected_items: None,
    };
    message_choice(element, kind)
}

pub(super) fn message_radio(element: &Value) -> Result<FormField, FormError> {
    let options = radio_options(element)?;
    let kind = InputKind::Radio { options };
    message_choice(element, kind)
}

fn message_choice(element: &Value, kind: InputKind) -> Result<FormField, FormError> {
    let options = match &kind {
        InputKind::StaticSelect { options, .. } | InputKind::Radio { options } => options,
        _ => return Err(invalid("Expected a single choice input")),
    };
    // Captured message cxJV treats a null initial_option as absent. Modal
    // parsing retains its existing independent input validation behavior.
    let initial = if element.get("initial_option").is_none_or(Value::is_null) {
        vec![]
    } else {
        initial_options(element, false, options)?
    };
    let initial_value = InputValue::Selected(initial);
    Ok(FormField {
        label: "Selection".into(),
        hint: None,
        placeholder: text(element.get("placeholder"))?,
        optional: true,
        kind,
        initial_value: initial_value.clone(),
        value: initial_value,
        dirty: false,
        error: None,
        revision: 0,
        raw: element.clone(),
    })
}

fn initial_options(
    element: &Value,
    multiple: bool,
    options: &[OptionSpec],
) -> Result<Vec<String>, FormError> {
    let mut values = vec![];
    let selected: Vec<&Value> = if multiple {
        match element.get("initial_options") {
            None => vec![],
            Some(v) => v
                .as_array()
                .ok_or_else(|| invalid("Invalid initial selections"))?
                .iter()
                .collect(),
        }
    } else {
        element.get("initial_option").into_iter().collect()
    };
    for selected in selected {
        let found = options
            .iter()
            .find(|option| &option.raw == selected)
            .ok_or_else(|| invalid("Initial selection does not match an option"))?;
        if values.contains(&found.value) {
            return Err(invalid("Duplicate initial selection"));
        }
        values.push(found.value.clone());
    }
    Ok(values)
}
