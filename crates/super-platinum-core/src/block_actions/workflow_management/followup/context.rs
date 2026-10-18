//! 1njB.buildInteractivityPointerUpdates/buildUserContextUpdates and 8ppH.
use super::super::trigger::required_parameters;
use super::super::*;
use crate::state::sidebar_name_cmp;

struct Variable {
    id: String,
    kind: String,
    owner: String,
    source: String,
    label: String,
    reference: Option<String>,
}
struct Flat<'a> {
    step: &'a Value,
    parents: Vec<&'a Value>,
}
fn flatten<'a>(steps: &'a [Value], parents: &[&'a Value], out: &mut Vec<Flat<'a>>) {
    for step in steps {
        out.push(Flat {
            step,
            parents: parents.to_vec(),
        });
        if let Some(children) = step["steps"].as_array() {
            let mut parents = parents.to_vec();
            parents.push(step);
            flatten(children, &parents, out);
        }
    }
}
fn variables(workflow: &Value, available: &Value, trigger: &Value) -> Result<Vec<Variable>, Error> {
    let mut flat = Vec::new();
    flatten(array(workflow, "steps")?, &[], &mut flat);
    let mut result = Vec::new();
    if let Some(data) = available.as_object() {
        for (id, value) in data {
            let owner = value["source_id"].as_str().unwrap_or("");
            if !flat.iter().any(|s| s.step["id"].as_str() == Some(owner))
                && !(value["source"] == json!("variables") && !owner.is_empty())
            {
                continue;
            }
            result.push(Variable {
                id: id.clone(),
                kind: value["type"].as_str().unwrap_or("").into(),
                owner: owner.into(),
                source: value["source"].as_str().unwrap_or("").into(),
                label: label(value),
                reference: None,
            });
        }
    }
    result.sort_by(|a, b| sidebar_name_cmp(&a.label, &b.label));
    let mut triggers = Vec::new();
    if let Some(data) = trigger["available_data"].as_object() {
        for (reference, value) in data {
            let name = value["name"].as_str().unwrap_or("");
            let owner = trigger["id"].as_str().unwrap_or("");
            triggers.push(Variable {
                id: format!("{{{{inputs.{owner}__{name}}}}}"),
                kind: value["type"].as_str().unwrap_or("").into(),
                owner: owner.into(),
                source: "trigger".into(),
                label: label(value),
                reference: Some(reference.clone()),
            });
        }
    }
    triggers.sort_by(|a, b| sidebar_name_cmp(&a.label, &b.label));
    result.extend(triggers);
    Ok(result)
}
fn label(value: &Value) -> String {
    value["title"]
        .as_str()
        .filter(|s| !s.is_empty())
        .or_else(|| value["name"].as_str())
        .unwrap_or("")
        .into()
}
fn choose<'a>(
    variables: &'a [Variable],
    predicate: impl Fn(&Variable) -> bool,
) -> Result<Option<&'a Variable>, Error> {
    let values = variables
        .iter()
        .filter(|v| predicate(v))
        .collect::<Vec<_>>();
    // The web comparator may be replaced by the captured locale. With no
    // actual locale observation in this acquisition, do not pick among multiple
    // eligible contexts using a guessed collation order.
    if values.len() > 1 {
        return Err(invalid());
    }
    Ok(values.first().copied())
}
fn parameters<'a>(step: &'a Value, kind: &str) -> Vec<&'a Value> {
    step["function"]["input_parameters"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|p| p["type"].as_str() == Some(kind))
        .collect()
}
fn update(
    workflow: &Value,
    step: &Value,
    parameters: &[&Value],
    variable: &Variable,
    trigger: &Value,
    with_parameters: bool,
    reason: &'static str,
) -> Result<RequestPlan, Error> {
    let mut inputs = step["inputs"].as_object().cloned().ok_or_else(invalid)?;
    for parameter in parameters {
        let name = parameter["name"].as_str().ok_or_else(invalid)?;
        inputs.insert(
            name.into(),
            json!({"value":variable.id,"hidden":false,"locked":false}),
        );
    }
    let mut args = json!({"workflow_id":identifier(workflow,"id")?,"step":{"id":step["id"],"function_id":identifier(&step["function"],"id")?,"inputs":inputs}});
    if with_parameters {
        if let Some(reference) = &variable.reference {
            let mut generated = required_parameters(trigger, std::slice::from_ref(reference))?;
            generated.extend(
                workflow["input_parameters"]
                    .as_object()
                    .cloned()
                    .ok_or_else(invalid)?,
            );
            args["input_parameters"] = json!(generated);
        }
    }
    Ok(RequestPlan::new(
        "functions.workflows.steps.update",
        reason,
        args,
    ))
}
pub(super) fn interactivity(
    workflow: &Value,
    available: &Value,
    trigger: &Value,
) -> Result<Vec<RequestPlan>, Error> {
    let vars = variables(workflow, available, trigger)?;
    let mut flat = Vec::new();
    flatten(array(workflow, "steps")?, &[], &mut flat);
    let mut result = Vec::new();
    for (index, item) in flat.iter().enumerate() {
        let required = parameters(item.step, "slack#/types/interactivity");
        if required.is_empty() {
            continue;
        }
        let prior = index.checked_sub(1).map(|n| flat[n].step);
        let after_case = prior.is_some_and(|s| s["step_type"] == json!("case"));
        let switch = item
            .parents
            .iter()
            .rev()
            .find(|s| s["step_type"] == json!("switch"));
        let prior = if after_case {
            switch.and_then(|parent| {
                flat.iter()
                    .position(|s| s.step["id"] == parent["id"])
                    .and_then(|n| n.checked_sub(1))
                    .map(|n| flat[n].step)
            })
        } else {
            prior
        };
        let chosen = choose(&vars, |v| {
            if v.kind != "slack#/types/interactivity" {
                return false;
            }
            match prior {
                None => v.source == "trigger",
                Some(previous) => {
                    v.owner == previous["id"].as_str().unwrap_or("")
                        && (previous["function"]["callback_id"] != json!("open_form")
                            || switch.is_some() && after_case
                            || matches!(
                                item.step["function"]["callback_id"].as_str(),
                                Some("send_slackbot_prompt" | "open_3p_agent_conversation")
                            ))
                }
            }
        })?;
        let Some(variable) = chosen else {
            continue;
        };
        if required.iter().all(|p| {
            p["name"]
                .as_str()
                .is_some_and(|name| item.step["inputs"][name]["value"] == json!(variable.id))
        }) {
            continue;
        }
        let needs_context =
            prior.is_none() && variable.source == "trigger" && variable.reference.is_some();
        result.push(update(
            workflow,
            item.step,
            &required,
            variable,
            trigger,
            result.is_empty() && needs_context,
            "refresh-interactivity",
        )?);
    }
    Ok(result)
}
pub(super) fn user_context(
    workflow: &Value,
    available: &Value,
    trigger: &Value,
) -> Result<Vec<RequestPlan>, Error> {
    let vars = variables(workflow, available, trigger)?;
    let mut result = Vec::new();
    fn walk<'a>(
        steps: &'a [Value],
        preceding: &[&'a Value],
        workflow: &Value,
        trigger: &Value,
        vars: &[Variable],
        result: &mut Vec<RequestPlan>,
    ) -> Result<(), Error> {
        let mut preceding = preceding.to_vec();
        for step in steps {
            let required = parameters(step, "slack#/types/user_context");
            if !required.is_empty() {
                let mut chosen = None;
                for previous in preceding.iter().rev() {
                    if previous["function"]["type"] == json!("builtin") {
                        chosen = choose(vars, |v| {
                            v.kind == "slack#/types/user_context"
                                && v.owner == previous["id"].as_str().unwrap_or("")
                        })?;
                        if chosen.is_some() {
                            break;
                        }
                    }
                }
                if chosen.is_none() {
                    chosen = choose(vars, |v| {
                        v.kind == "slack#/types/user_context" && v.source == "trigger"
                    })?;
                }
                if let Some(variable) = chosen {
                    result.push(update(
                        workflow,
                        step,
                        &required,
                        variable,
                        trigger,
                        variable.source == "trigger" && variable.reference.is_some(),
                        "refresh-user-context",
                    )?);
                }
            }
            preceding.push(step);
            if let Some(children) = step["steps"].as_array() {
                walk(children, &preceding, workflow, trigger, vars, result)?;
            }
        }
        Ok(())
    }
    walk(
        array(workflow, "steps")?,
        &[],
        workflow,
        trigger,
        &vars,
        &mut result,
    )?;
    Ok(result)
}
