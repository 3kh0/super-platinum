//! 1njB/u5Lm trigger round-trip for an acquired trigger, never a guessed ID.
mod external;
mod keys;
use super::*;
use serde_json::Map;

#[derive(Clone, Debug, PartialEq)]
pub struct TriggerUpdate {
    original: Value,
    arguments: Value,
}
impl TriggerUpdate {
    pub fn from_editor(editor: &Editor) -> Result<Self, Error> {
        Self::capture(editor.workflow_id(), editor.trigger().ok_or_else(invalid)?)
    }
    pub fn capture(workflow: &str, trigger: &Value) -> Result<Self, Error> {
        if !id(workflow) || trigger["workflow"]["workflow_id"].as_str() != Some(workflow) {
            return Err(invalid());
        }
        let trigger_id = identifier(trigger, "id")?;
        let kind = trigger["type"].as_str().ok_or_else(invalid)?;
        let inputs = trigger.get("inputs").cloned().unwrap_or_else(|| json!({}));
        if !inputs.is_object() {
            return Err(invalid());
        }
        let mut args = json!({"trigger_id":trigger_id,"workflow":workflow,"type":kind,"inputs":inputs,
            "name":trigger.get("name").and_then(Value::as_str).unwrap_or("")});
        if let Some(description) = trigger.get("description").filter(|v| !v.is_null()) {
            args["description"] = description.clone();
        }
        match kind {
            "scheduled" => {
                let schedule = trigger
                    .get("schedule")
                    .filter(|v| v.is_object())
                    .ok_or_else(invalid)?;
                // Missing timezone would require the actual member timezone
                // producer. Do not invent the web member/default timezone.
                if schedule["timezone"].as_str().is_none_or(str::is_empty) {
                    return Err(invalid());
                }
                args["schedule"] = keys::camelize(schedule, 0)?;
            }
            "shortcut" | "webhook" => {
                if kind == "shortcut" && !args.as_object().unwrap().contains_key("description") {
                    args["description"] = json!("");
                }
                let available = trigger.get("available_data").and_then(Value::as_object);
                let mut schema = Map::new();
                if let Some(available) = available {
                    for (reference, value) in available {
                        let supported = matches!(
                            value["type"].as_str(),
                            Some("string" | "slack#/types/user_id" | "slack#/types/channel_id")
                        ) || kind == "webhook"
                            && value["type"] == json!("slack#/types/user_email");
                        if !supported
                            || kind == "shortcut" && value["is_customizable"] != json!(true)
                            || kind == "webhook" && !data_reference(reference)
                        {
                            continue;
                        }
                        let key = value[if kind == "shortcut" { "name" } else { "title" }]
                            .as_str()
                            .ok_or_else(invalid)?;
                        let fields = if kind == "shortcut" {
                            &["type", "name", "title", "description", "is_customizable"][..]
                        } else {
                            &["type", "name", "title", "is_required", "is_hidden"][..]
                        };
                        let mut item = Map::new();
                        for field in fields {
                            if let Some(value) = value.get(field) {
                                item.insert((*field).into(), value.clone());
                            }
                        }
                        // Lost JSON insertion order cannot safely choose between
                        // duplicate user schema names. Retain input and refuse.
                        if schema.insert(key.into(), Value::Object(item)).is_some() {
                            return Err(invalid());
                        }
                    }
                }
                args[kind] = json!({"schema":schema});
            }
            "event" => args["event"] = external::event(trigger)?,
            "external" => args["external"] = external::external(trigger)?,
            "service" => {
                let service = trigger
                    .get("service")
                    .and_then(Value::as_object)
                    .ok_or_else(invalid)?;
                let mut inputs = service
                    .get("inputs")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                // lqV5.$v selects function-declared required/configured inputs;
                // pX6g supplies only an empty typed value for absent requireds.
                if let Some(function) = trigger
                    .pointer("/service_config/function")
                    .filter(|v| v.is_object())
                {
                    let parameters = array(function, "input_parameters")?;
                    let mut selected = Map::new();
                    for parameter in parameters {
                        let name = parameter["name"].as_str().ok_or_else(invalid)?;
                        if parameter["is_required"] == json!(true) || inputs.contains_key(name) {
                            selected.insert(name.into(),inputs.remove(name).unwrap_or_else(||json!({"value":if matches!(parameter["type"].as_str(),Some("array"|"slack#/types/interactivity")){json!([])}else{Value::Null}})));
                        }
                    }
                    inputs = selected;
                }
                inputs.retain(|_, v| v["value"].is_number() || !js_empty(&v["value"]));
                args["service"] = json!({"trigger_type":service.get("trigger_type").ok_or_else(invalid)?,"inputs":inputs});
                if !args.as_object().unwrap().contains_key("description") {
                    args["description"] = json!("");
                }
            }
            _ => return Err(invalid()),
        }
        Ok(Self {
            original: trigger.clone(),
            arguments: keys::wire(&args)?,
        })
    }
    pub fn original(&self) -> &Value {
        &self.original
    }
    pub fn arguments(&self) -> &Value {
        &self.arguments
    }
    pub fn metadata(mut self, name: &str, description: &str) -> Result<Self, Error> {
        self.arguments["workflow_app_id"] =
            json!(identifier(&self.original["workflow"], "app_id")?);
        if !name.is_empty() {
            self.arguments["name"] = json!(name);
        }
        if !description.is_empty() {
            self.arguments["description"] = json!(description);
        }
        Ok(self)
    }
    /// 8dyy.ea runs for both publish and unpublish, using the new workflow.
    pub fn synchronize(workflow: &Value, trigger: &Value) -> Result<Option<RequestPlan>, Error> {
        let workflow_id = identifier(workflow, "id")?;
        let desired = workflow["input_parameters"]
            .as_object()
            .ok_or_else(invalid)?;
        let previous = trigger["inputs"].as_object().ok_or_else(invalid)?;
        if desired.len() == previous.len() && desired.keys().all(|key| previous.contains_key(key)) {
            return Ok(None);
        }
        let mut inputs = Map::new();
        for key in desired.keys() {
            if let Some(value) = previous.get(key).filter(|v| js_truthy(v)) {
                inputs.insert(key.clone(), value.clone());
                continue;
            }
            if let Some((reference, _)) = available(trigger, Lookup::Name(key))? {
                let short = key.split("__").nth(1).unwrap_or(key);
                let matches = trigger["available_data"]
                    .as_object()
                    .ok_or_else(invalid)?
                    .iter()
                    .filter(|(name, _)| name.contains(short))
                    .collect::<Vec<_>>();
                if matches.len() > 1 {
                    return Err(invalid());
                }
                let customizable = matches
                    .first()
                    .is_some_and(|(_, value)| value["is_customizable"] == json!(true));
                inputs.insert(
                    key.clone(),
                    if customizable {
                        json!({"customizable":true})
                    } else {
                        json!({"value":reference})
                    },
                );
            }
        }
        let mut prepared = Self::capture(&workflow_id, trigger)?;
        prepared.arguments["inputs"] = Value::Object(inputs);
        Ok(Some(RequestPlan::new(
            "workflows.triggers.update",
            "publish-workflow",
            prepared.arguments,
        )))
    }
}
fn data_reference(s: &str) -> bool {
    // QJPX uses the first nFyI.iJ match and compares its namespace with the
    // lowercase literal data. The regexp is case insensitive; that comparison
    // is not. Retain the optional dollar marker and UTF-16 path range.
    for (start, _) in s.match_indices("{{") {
        let tail = &s[start + 2..];
        let Some(end) = tail.find("}}") else { continue };
        let token = tail[..end].strip_prefix('$').unwrap_or(&tail[..end]);
        let namespace_len = token.bytes().take_while(u8::is_ascii_alphabetic).count();
        if namespace_len == 0 {
            continue;
        }
        let namespace = &token[..namespace_len];
        let rest = &token[namespace_len..];
        let valid_path = |path: &str| {
            !path.is_empty()
                && path.chars().all(|c| {
                    c.is_ascii_alphanumeric()
                        || matches!(c, '-' | '_' | '.' | '[' | ']')
                        // nFyI uses a non-u JS regexp: astral scalars are two
                        // accepted UTF-16 surrogate units in U+0080..U+FFFF.
                        || c >= '\u{80}'
                })
        };
        let mut matched = false;
        for initial_dot in [false, true] {
            let Some(rest) = (if initial_dot {
                rest.strip_prefix('.')
            } else {
                Some(rest)
            }) else {
                continue;
            };
            let owner_len = rest
                .bytes()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == b'-')
                .count();
            for length in 0..=owner_len {
                let tail = &rest[length..];
                if let Some(path) = tail.strip_prefix('.').or_else(|| tail.strip_prefix("__")) {
                    if valid_path(path) || path.strip_prefix('.').is_some_and(valid_path) {
                        matched = true;
                        break;
                    }
                }
            }
            if matched {
                break;
            }
        }
        if matched {
            return namespace == "data";
        }
    }
    false
}
pub(super) fn js_empty(value: &Value) -> bool {
    match value {
        Value::String(s) => s.is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
        _ => true,
    }
}
pub(super) fn js_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::String(s) => !s.is_empty(),
        Value::Number(n) => n.as_f64() != Some(0.0),
        _ => true,
    }
}
#[derive(Clone, Copy)]
pub(super) enum Lookup<'a> {
    Name(&'a str),
    Reference(&'a str),
}
pub(super) fn available<'a>(
    trigger: &'a Value,
    lookup: Lookup<'_>,
) -> Result<Option<(&'a str, &'a Value)>, Error> {
    let Some(data) = trigger["available_data"].as_object() else {
        return Ok(None);
    };
    let mut found = Vec::new();
    for (reference, value) in data {
        let mut candidates = vec![(reference.as_str(), value)];
        if value["type"] == json!("object") {
            if let Some(properties) = value["properties"].as_object() {
                candidates.extend(properties.iter().map(|(k, v)| (k.as_str(), v)));
            }
        }
        for (reference, value) in candidates {
            let matches = match lookup {
                Lookup::Reference(wanted) => reference == wanted,
                Lookup::Name(wanted) => value["name"].as_str().is_some_and(|name| {
                    name == wanted
                        || format!("{}__{name}", trigger["id"].as_str().unwrap_or("")) == wanted
                }),
            };
            if matches {
                found.push((reference, value));
            }
        }
    }
    // HbJD chooses first. A serde Value does not retain original key ordering;
    // duplicate semantic identities therefore cannot authorize a guessed write.
    if found.len() > 1 {
        Err(invalid())
    } else {
        Ok(found.pop())
    }
}
pub(super) fn required_parameters(
    trigger: &Value,
    references: &[String],
) -> Result<Map<String, Value>, Error> {
    let mut result = Map::new();
    for reference in references {
        let Some((_, value)) = available(trigger, Lookup::Reference(reference))? else {
            continue;
        };
        let Some(name) = value["name"].as_str().filter(|s| !s.is_empty()) else {
            continue;
        };
        let key = format!("{}__{name}", identifier(trigger, "id")?);
        let mut parameter = Map::new();
        parameter.insert("name".into(), json!(key));
        for field in [
            "type",
            "title",
            "description",
            "is_hidden",
            "nullable",
            "supplemental_type",
        ] {
            if let Some(value) = value.get(field) {
                parameter.insert(field.into(), value.clone());
            }
        }
        if value["type"] == json!("array") {
            if let Some(items) = value.get("items") {
                parameter.insert("items".into(), items.clone());
            }
        }
        result.insert(key, Value::Object(parameter));
    }
    Ok(result)
}
