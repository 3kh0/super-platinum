use super::*;

pub(super) fn event(trigger: &Value) -> Result<Value, Error> {
    let kind = trigger
        .get("event_type")
        .or_else(|| trigger.pointer("/event/event_type"))
        .and_then(Value::as_str)
        .ok_or_else(invalid)?;
    let mut event = Map::new();
    event.insert("event_type".into(), json!(kind));
    for key in ["filter", "team_ids", "channel_ids"] {
        if let Some(value) = trigger.get(key) {
            event.insert(key.into(), value.clone());
        }
    }
    if let Some(nested) = trigger["event"].as_object() {
        event.extend(
            keys::camelize(&Value::Object(nested.clone()), 1)?
                .as_object()
                .ok_or_else(invalid)?
                .clone(),
        );
    }
    match kind {
        "slack#/events/message_posted" => {
            if let Some(value) = trigger.get("classifier_prompt").filter(|v| js_truthy(v)) {
                event.insert("classifier_prompt".into(), value.clone());
            }
        }
        "slack#/events/reaction_added" => {
            if trigger.get("only_first_reaction").is_some_and(js_truthy) {
                event.insert("only_first_reaction".into(), json!(true));
            }
        }
        "slack#/events/list_record_created" | "slack#/events/list_record_updated" => {
            let channel = trigger["channel_ids"]
                .as_array()
                .and_then(|v| v.first())
                .and_then(Value::as_str)
                .filter(|s| s.starts_with('C'))
                .ok_or_else(invalid)?;
            event.insert("channel_ids".into(), json!([channel]));
            event.remove("list_id");
            if event
                .get("filter")
                .and_then(|v| v.get("root"))
                .is_some_and(|root| {
                    root.get("operator").is_some()
                        && root["inputs"].as_array().is_some_and(Vec::is_empty)
                })
            {
                event.remove("filter");
            }
        }
        "slack#/events/salesforce_user_joined_record_channel" => {
            event.insert("all_resources".into(), json!(true));
            event.remove("record_conditions");
            if let Some(conditions) = trigger.get("record_conditions").filter(|v| v.is_object()) {
                if let Some(prepared) = record_conditions(conditions)? {
                    event.insert("record_conditions".into(), prepared);
                }
            }
        }
        _ => {}
    }
    Ok(Value::Object(event))
}
fn clean_filter(filter: &Value) -> Result<Option<Value>, Error> {
    let mut result = filter.as_object().cloned().ok_or_else(invalid)?;
    let mut groups = Vec::new();
    for group in array(filter, "conditions")? {
        let mut prepared = group.as_object().cloned().ok_or_else(invalid)?;
        prepared.remove("id");
        let mut leaves = Vec::new();
        for leaf in array(group, "conditions")? {
            if leaf.get("type").is_none() && leaf.get("schema_field_id").is_none() {
                continue;
            }
            let mut value = leaf.as_object().cloned().ok_or_else(invalid)?;
            for key in ["id", "rhs", "isAutoSelected", "isFieldValid"] {
                value.remove(key);
            }
            if let Some(rhs) = leaf.get("rhs").filter(|v| !v.is_null()) {
                let values = if let Some(values) = rhs.as_array() {
                    values.clone()
                } else {
                    vec![rhs.clone()]
                };
                let values = values
                    .into_iter()
                    .map(|mut value| {
                        if let Some(object) = value.as_object_mut() {
                            object.remove("isError");
                        }
                        value
                    })
                    .collect::<Vec<_>>();
                value.insert("rhs".into(), json!(values));
            }
            leaves.push(Value::Object(value));
        }
        if !leaves.is_empty() {
            prepared.insert("conditions".into(), json!(leaves));
            groups.push(Value::Object(prepared));
        }
    }
    if groups.is_empty() {
        return Ok(None);
    }
    result.insert("conditions".into(), json!(groups));
    Ok(Some(Value::Object(result)))
}
fn record_conditions(conditions: &Value) -> Result<Option<Value>, Error> {
    let filter = conditions
        .get("filter")
        .map(clean_filter)
        .transpose()?
        .flatten();
    let mut children = Vec::new();
    if let Some(raw) = conditions["child_filters"].as_array() {
        for child in raw {
            if let Some(filter) = clean_filter(&child["filter"])? {
                let mut prepared = child.as_object().cloned().ok_or_else(invalid)?;
                prepared.remove("id");
                prepared.insert("filter".into(), filter);
                children.push(Value::Object(prepared));
            }
        }
    }
    if filter.is_none() && children.is_empty() {
        return Ok(None);
    }
    let mut output = Map::new();
    for key in ["datasource_id", "schema_object_id", "object_identifier"] {
        if let Some(value) = conditions.get(key) {
            output.insert(key.into(), value.clone());
        }
    }
    if let Some(filter) = filter {
        output.insert("filter".into(), filter);
    }
    if !children.is_empty() {
        output.insert("child_filters".into(), json!(children));
    }
    Ok(Some(Value::Object(output)))
}
pub(super) fn external(trigger: &Value) -> Result<Value, Error> {
    let raw = trigger["external"].as_object().ok_or_else(invalid)?;
    let subtype = raw
        .get("subtype")
        .and_then(Value::as_str)
        .ok_or_else(invalid)?;
    let mut prepared = Map::new();
    for key in ["subtype", "datasource_id", "schema_object_id"] {
        if let Some(value) = raw.get(key) {
            prepared.insert(key.into(), value.clone());
        }
    }
    prepared.insert(
        "schema".into(),
        raw.get("schema").cloned().unwrap_or_else(|| json!({})),
    );
    let mut conditions = raw.get("conditions").filter(|v|js_truthy(v)).cloned().unwrap_or_else(||json!({"type":"AndTriggerCondition","conditions":[{"type":"AndTriggerCondition","conditions":[{}]}]}));
    normalize_conditions(&mut conditions)?;
    strip_all_ids(&mut conditions);
    // Instant aP rebuilds the root condition object from type and conditions;
    // digest/snapshot/lookback retain the actual condition payload.
    if !["digest", "snapshot", "lookback"].contains(&subtype) {
        conditions = json!({"type":conditions.get("type").ok_or_else(invalid)?,"conditions":conditions.get("conditions").ok_or_else(invalid)?});
    }
    prepared.insert("conditions".into(), conditions);
    let mut filters = Vec::new();
    if let Some(items) = raw.get("filters").and_then(Value::as_array) {
        for filter in items {
            let mut item = filter.as_object().cloned().ok_or_else(invalid)?;
            item.remove("id");
            let mut conditions = strip_group_ids(&filter["conditions"])?;
            conditions = json!({"type":conditions.get("type").ok_or_else(invalid)?,"conditions":conditions["conditions"]});
            item.insert("conditions".into(), conditions);
            filters.push(Value::Object(item));
        }
    }
    prepared.insert("filters".into(), json!(filters));
    if ["digest", "snapshot", "lookback"].contains(&subtype) {
        let schedule = raw
            .get("schedule")
            .and_then(Value::as_object)
            .ok_or_else(invalid)?;
        let frequency = schedule
            .get("frequency")
            .and_then(Value::as_object)
            .ok_or_else(invalid)?;
        let mut freq = Map::new();
        for key in ["type", "on_days", "on_week_num", "repeats_every"] {
            if let Some(value) = frequency.get(key) {
                freq.insert(key.into(), value.clone());
            }
        }
        prepared.insert("schedule".into(),json!({"timezone":"Etc/UTC","frequency":freq,"start_time":schedule.get("start_time").ok_or_else(invalid)?}));
        let sort = raw
            .get("sort_config")
            .and_then(Value::as_object)
            .ok_or_else(invalid)?;
        let mut selected = Map::new();
        for key in ["schema_field_id", "direction", "value", "limit"] {
            if let Some(value) = sort.get(key) {
                selected.insert(key.into(), value.clone());
            }
        }
        prepared.insert("sort_config".into(), Value::Object(selected));
    } else {
        let mut extras = Vec::new();
        if let Some(values) = raw.get("extra_fields").and_then(Value::as_array) {
            for value in values {
                let mut item = value.as_object().cloned().ok_or_else(invalid)?;
                item.remove("id");
                // aP adds an editor-only isFieldValid key, not the unknown raw
                // is_field_valid key, so the latter remains in the wire object.
                if let Some(filter) = value.get("filter") {
                    if matches!(
                        filter["type"].as_str(),
                        Some("AndFilterCondition" | "OrFilterCondition")
                    ) {
                        item.insert("filter".into(), strip_group_ids(filter)?);
                    } else {
                        item.remove("filter");
                    }
                }
                extras.push(Value::Object(item));
            }
        }
        prepared.insert("extra_fields".into(), json!(extras));
    }
    Ok(Value::Object(prepared))
}
fn strip_group_ids(value: &Value) -> Result<Value, Error> {
    let mut result = value.as_object().cloned().ok_or_else(invalid)?;
    let mut groups = Vec::new();
    for group in array(value, "conditions")? {
        let mut item = group.as_object().cloned().ok_or_else(invalid)?;
        item.remove("id");
        let mut conditions = Vec::new();
        for condition in array(group, "conditions")? {
            let mut leaf = condition.as_object().cloned().ok_or_else(invalid)?;
            leaf.remove("id");
            conditions.push(Value::Object(leaf));
        }
        item.insert("conditions".into(), json!(conditions));
        groups.push(Value::Object(item));
    }
    result.insert("conditions".into(), json!(groups));
    Ok(Value::Object(result))
}
fn strip_all_ids(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.remove("id");
            for value in map.values_mut() {
                strip_all_ids(value);
            }
        }
        Value::Array(values) => {
            for value in values {
                strip_all_ids(value);
            }
        }
        _ => {}
    }
}

fn normalize_conditions(value: &mut Value) -> Result<(), Error> {
    fn group(value: &mut Value) -> Result<(), Error> {
        let Some(leaves) = value.get_mut("conditions").and_then(Value::as_array_mut) else {
            return Ok(());
        };
        for leaf in leaves {
            let replacement = match leaf["type"].as_str() {
                Some(
                    "TimePeriodToAnotherTriggerCondition"
                    | "TimePeriodToFutureTriggerCondition"
                    | "TimePeriodToEarlierTriggerCondition"
                    | "TimePeriodIsMoreThanTriggerCondition"
                    | "TimePeriodIsLessThanTriggerCondition"
                    | "TimePeriodIsEqualToTriggerCondition",
                ) => Some("TimePeriodHasChangedByTriggerCondition"),
                Some(
                    "TimePeriodAfterTriggerCondition"
                    | "TimePeriodBeforeTriggerCondition"
                    | "TimePeriodCurrentTriggerCondition",
                ) => Some("TimePeriodWithinTriggerCondition"),
                _ => None,
            };
            if let Some(kind) = replacement {
                leaf["type"] = json!(kind);
            }
        }
        Ok(())
    }
    fn groups(value: &mut Value) -> Result<(), Error> {
        if let Some(groups) = value.get_mut("conditions").and_then(Value::as_array_mut) {
            for value in groups {
                group(value)?;
            }
        }
        Ok(())
    }
    // Temo.ti leaves static snapshots unchanged. Digest has two different
    // condition depths; normalization applies only to declared condition leaves.
    if value.get("static_condition").is_some() {
        return Ok(());
    }
    if value.get("change_condition").is_some() {
        group(value.get_mut("change_condition").ok_or_else(invalid)?)?;
        groups(value.get_mut("digest_condition").ok_or_else(invalid)?)
    } else {
        groups(value)
    }
}
