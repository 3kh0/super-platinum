use super::*;
use std::collections::BTreeSet;

/// A current observation is not proof of causation, nor a replacement receipt.
/// Calling `observe` never settles or changes an unknown or in-flight slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Observation {
    Matches,
    Different,
    Unavailable,
}
impl Editor {
    pub fn published(&self) -> bool {
        self.raw_workflow()
            .get("is_published")
            .is_some_and(trigger::js_truthy)
    }
    pub fn metadata_command(
        &self,
        name: &str,
        description: &str,
        icon_url: Option<&str>,
    ) -> Result<Command, Error> {
        validate_name(name).map_err(|_| invalid())?;
        let trigger_update = self
            .trigger()
            .map(|_| {
                TriggerUpdate::from_editor(self)?
                    .metadata(name, description)
                    .map(|t| t.arguments().clone())
            })
            .transpose()?;
        let command = Command::Metadata {
            name: name.into(),
            description: description.into(),
            icon_url: icon_url.map(str::to_owned),
            trigger_update,
        };
        // Includes exact no-change/trigger-identity admission.
        MutationPlan::new(self, command.clone())?;
        Ok(command)
    }
}
impl MutationPlan {
    pub fn observe(&self, current: &Editor) -> Observation {
        if self.original().workflow_id() != current.workflow_id()
            || self.original().raw_workflow()["app_id"] != current.raw_workflow()["app_id"]
        {
            return Observation::Unavailable;
        }
        let matched = match self.command() {
            Command::Metadata {
                name,
                description,
                icon_url,
                trigger_update,
            } => {
                let workflow = current.raw_workflow();
                let workflow_matches = workflow["title"] == json!(name)
                    && workflow["description"] == json!(description)
                    && icon_url
                        .as_ref()
                        .is_none_or(|url| workflow["icons"]["image_192"] == json!(url));
                let trigger_matches = match (trigger_update, current.trigger()) {
                    (None, None) => true,
                    (Some(expected), Some(actual)) => {
                        actual["id"] == expected["trigger_id"]
                            && actual["name"] == expected["name"]
                            && actual.get("description") == expected.get("description")
                    }
                    _ => false,
                };
                workflow_matches && trigger_matches
            }
            Command::Published(published) => {
                let Some(actual) = current.raw_workflow()["is_published"].as_bool() else {
                    return Observation::Unavailable;
                };
                actual == *published
            }
            Command::AddCollaborators(ids) | Command::RemoveCollaborators(ids) => {
                let Some(actual) = string_set(&current.raw_workflow()["collaborators"]) else {
                    return Observation::Unavailable;
                };
                ids.iter().all(|id| {
                    actual.contains(id.as_str())
                        == matches!(self.command(), Command::AddCollaborators(_))
                })
            }
            Command::CopyPermission(permission) => {
                current.copy_permissions()["permission_type"] == json!(permission)
            }
            Command::RunPermission { .. } => {
                let Some(actual) = current.run_permissions() else {
                    return Observation::Unavailable;
                };
                let expected = self.slots()[0].plan().arguments();
                if actual["permission_type"] != expected["permission_type"] {
                    false
                } else {
                    let mut matches = true;
                    for key in ["user_ids", "channel_ids", "team_ids", "org_ids"] {
                        if let Some(expected) = expected.get(key) {
                            let Some(want) = string_set(expected) else {
                                return Observation::Unavailable;
                            };
                            let empty = json!([]);
                            let Some(have) = string_set(actual.get(key).unwrap_or(&empty)) else {
                                return Observation::Unavailable;
                            };
                            matches &= want == have;
                        }
                    }
                    matches
                }
            }
            Command::Step(StepAction::Add { step, .. } | StepAction::Update { step, .. }) => {
                let Some(actual) = find(current.steps(), step["id"].as_str().unwrap_or("")) else {
                    return Observation::Different;
                };
                actual["inputs"] == step["inputs"]
                    && actual["function"]["id"] == step["function_id"]
            }
            Command::Step(StepAction::Delete { step_id }) => {
                find(current.steps(), step_id).is_none()
            }
            // A hierarchical move requires the source's expected-tree projection;
            // absence after deletion is checked by the owned delete receipt path.
            Command::Step(StepAction::Move { .. }) | Command::Delete => {
                return Observation::Unavailable;
            }
        };
        if matched {
            Observation::Matches
        } else {
            Observation::Different
        }
    }
}
fn string_set(value: &Value) -> Option<BTreeSet<&str>> {
    value.as_array()?.iter().map(Value::as_str).collect()
}
fn find<'a>(steps: &'a [Value], id: &str) -> Option<&'a Value> {
    for step in steps {
        if step["id"].as_str() == Some(id) {
            return Some(step);
        }
        if let Some(children) = step["steps"].as_array() {
            if let Some(step) = find(children, id) {
                return Some(step);
            }
        }
    }
    None
}
