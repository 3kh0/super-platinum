//! CIuT selectors over explicit acquired facts, separate from native authority.
use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct PublicationInputs {
    workflow: Value,
    trigger: Option<Value>,
    dynamic_inputs: Option<Value>,
    permissions: Option<EditPermissions>,
    scheduled_start_is_past: Option<bool>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PublicationAssessment {
    pub blocking_errors: Vec<Value>,
    pub has_steps: bool,
    pub publish_restricted: bool,
    pub unchanged_published: bool,
    pub can_be_published: bool,
    /// MckU view-only input; deliberately separate from CIuT's predicate.
    pub unavailable_steps: Vec<Value>,
}
impl PublicationInputs {
    pub fn new(
        editor: &Editor,
        dynamic_inputs: Option<&Value>,
        permissions: Option<&EditPermissions>,
        scheduled_start_is_past: Option<bool>,
    ) -> Self {
        Self {
            workflow: editor.raw_workflow().clone(),
            trigger: editor.trigger().cloned(),
            dynamic_inputs: dynamic_inputs.cloned(),
            permissions: permissions.cloned(),
            scheduled_start_is_past,
        }
    }
    pub fn current(&self, editor: &Editor) -> bool {
        self.workflow == *editor.raw_workflow() && self.trigger.as_ref() == editor.trigger()
    }
    /// Scoped native no-demand proof, not a replacement for the general source
    /// selector. Dynamic/wrapped/AI/reference/service paths need real producers.
    pub fn static_inputs(editor: &Editor, permissions: &EditPermissions) -> Result<Self, Error> {
        let trigger = editor.trigger().ok_or_else(invalid)?;
        if trigger["type"] != "shortcut"
            || !trigger["inputs"].is_object()
            || !literal(&trigger["inputs"])
        {
            return Err(invalid());
        }
        for step in flat_steps(editor.raw_workflow())? {
            let function = &step["function"];
            if function["type"] != "builtin"
                || step.get("wrapped_function").is_some()
                || function["id"] == "Fn0163"
                || function["callback_id"] == "send_message"
                || !function["input_parameters"].is_array()
                || dynamic(function)
                || !step["inputs"].is_object()
                || !literal(&step["inputs"])
            {
                return Err(invalid());
            }
            identifier(function, "id")?;
            if function["input_parameters"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| !v.is_object())
            {
                return Err(invalid());
            }
        }
        Ok(Self::new(editor, Some(&json!({})), Some(permissions), None))
    }
    /// Err means required acquisition/shape is unavailable. A complete blocked
    /// assessment is distinct and carries the actual original error records.
    pub fn assess(&self) -> Result<PublicationAssessment, Error> {
        let permissions = self.permissions.as_ref().ok_or_else(invalid)?;
        let dynamic = self
            .dynamic_inputs
            .as_ref()
            .and_then(Value::as_object)
            .ok_or_else(invalid)?;
        if !permissions.apps.is_object() {
            return Err(invalid());
        }
        let publication = Publication {
            workflow: &self.workflow,
        };
        let mut errors = Vec::new();
        let mut unavailable_steps = Vec::new();
        for step in flat_steps(&self.workflow)? {
            let step_id = identifier(step, "id")?;
            let function = &step["function"];
            if function["type"] == "app"
                && function["app_id"]
                    .as_str()
                    .is_some_and(|app| permissions.apps[app]["appResolution"] == "denied")
            {
                errors.push(json!({"code":"workflow_has_restricted_app_step","message":"You cannot publish with a restricted step in your workflow","pointer":"","source_id":step_id}));
            }
            dynamic_errors(dynamic.get(&step_id), &mut errors)?;
            server_errors(step, &mut errors)?;
            if !truthy(&function["product_level_availability"]["is_available"]) {
                unavailable_steps.push(step.clone());
            }
        }
        if let Some(trigger) = &self.trigger {
            let kind = trigger["type"].as_str().ok_or_else(invalid)?;
            if kind == "scheduled"
                && !publication.has_been_published()
                && self.scheduled_start_is_past.ok_or_else(invalid)?
            {
                errors.push(json!({"code":"invalid_datetime","message":"Scheduled to start in the past. Select a time today or on a future date.","pointer":"","source_id":identifier(trigger,"id")?}));
            }
            if kind == "service" {
                dynamic_errors(dynamic.get(&identifier(trigger, "id")?), &mut errors)?;
                server_errors(trigger, &mut errors)?;
            } else if kind == "event" && trigger["subtype"] == "slack#/events/reaction_added" {
                server_errors(trigger, &mut errors)?;
            }
        }
        // PBT7.Qr is exactly [] in the retained source: no errors are dropped.
        let has_steps = recognized(&self.workflow) && !array(&self.workflow, "steps")?.is_empty();
        let unchanged_published = !positive_count(self.workflow.get("unpublished_change_count"))?
            && publication.has_been_published()
            && publication.is_published();
        let rank =
            |step: &Value| match step["function"]["product_level_availability"]["available_to"]
                .as_str()
            {
                Some("enterprise") => 4,
                Some("compliance") => 3,
                Some("plus") => 2,
                Some("std") => 1,
                _ => 0,
            };
        unavailable_steps.sort_by_key(|step| std::cmp::Reverse(rank(step)));
        Ok(PublicationAssessment {
            can_be_published: errors.is_empty()
                && has_steps
                && !permissions.is_publish_restricted
                && !unchanged_published,
            blocking_errors: errors,
            has_steps,
            publish_restricted: permissions.is_publish_restricted,
            unchanged_published,
            unavailable_steps,
        })
    }
}

fn recognized(workflow: &Value) -> bool {
    if let Some(id) = workflow["id"].as_str() {
        return id.strip_prefix("Wf").is_some_and(|tail| {
            tail.len() >= 7
                && tail
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        });
    }
    workflow.get("id").is_none()
        && workflow.get("workflow").is_none()
        && workflow.get("creation_source_type").is_none()
}
fn flat_steps(workflow: &Value) -> Result<Vec<&Value>, Error> {
    fn walk<'a>(steps: &'a [Value], out: &mut Vec<&'a Value>) -> Result<(), Error> {
        for step in steps {
            if !step.is_object() {
                return Err(invalid());
            }
            out.push(step);
            if let Some(children) = step.get("steps") {
                walk(children.as_array().ok_or_else(invalid)?, out)?;
            }
        }
        Ok(())
    }
    let mut result = Vec::new();
    walk(array(workflow, "steps")?, &mut result)?;
    Ok(result)
}
fn dynamic_errors(value: Option<&Value>, out: &mut Vec<Value>) -> Result<(), Error> {
    let Some(value) = value.filter(|v| !v.is_null()) else {
        return Ok(());
    };
    for entry in value.as_object().ok_or_else(invalid)?.values() {
        if let Some(error) = entry.get("error").filter(|v| truthy(v)) {
            out.push(error.clone());
        }
    }
    Ok(())
}
fn server_errors(source: &Value, out: &mut Vec<Value>) -> Result<(), Error> {
    if let Some(errors) = source.get("errors").filter(|v| !v.is_null()) {
        for error in errors.as_array().ok_or_else(invalid)? {
            let mut error = error.as_object().ok_or_else(invalid)?.clone();
            error.insert("source_id".into(), json!(identifier(source, "id")?));
            out.push(Value::Object(error));
        }
    }
    Ok(())
}
fn positive_count(value: Option<&Value>) -> Result<bool, Error> {
    match value {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(v)) => Ok(*v),
        Some(Value::Number(n)) => Ok(n.as_f64().ok_or_else(invalid)? > 0.0),
        Some(Value::String(s)) if s.trim().is_empty() => Ok(false),
        Some(Value::String(s)) => Ok(s.trim().parse::<f64>().map_err(|_| invalid())? > 0.0),
        _ => Err(invalid()), // JS collection coercion is outside native shape support.
    }
}
fn dynamic(value: &Value) -> bool {
    match value {
        Value::Object(map) => map
            .iter()
            .any(|(key, value)| (key == "dynamic_options" && truthy(value)) || dynamic(value)),
        Value::Array(rows) => rows.iter().any(dynamic),
        _ => false,
    }
}
fn literal(value: &Value) -> bool {
    match value {
        Value::String(s) => !s.contains("{{"),
        Value::Array(rows) => rows.iter().all(literal),
        Value::Object(map) => map.iter().all(|(k, v)| !k.contains("{{") && literal(v)),
        _ => true,
    }
}
