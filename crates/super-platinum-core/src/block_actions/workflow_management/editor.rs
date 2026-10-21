use super::*;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReadKind {
    Latest,
    Restriction,
    Triggers,
    TriggerInfo,
    Preferences,
    CopyPermissions,
    RunPermissions,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ReadRequest {
    pub kind: ReadKind,
    pub plan: RequestPlan,
}
#[derive(Clone, Debug, PartialEq)]
pub struct EditorLoad {
    workflow: String,
    responses: BTreeMap<ReadKind, Value>,
}
impl EditorLoad {
    pub fn new(workflow: &str) -> Result<Self, Error> {
        if !id(workflow) {
            return Err(invalid());
        }
        Ok(Self {
            workflow: workflow.into(),
            responses: BTreeMap::new(),
        })
    }
    pub fn workflow_id(&self) -> &str {
        &self.workflow
    }
    /// The owner deduplicates in-flight requests; this does not send or retry.
    /// Restriction is independent and does not block editor acquisition in 8dyy.
    pub fn requests(&self) -> Vec<ReadRequest> {
        let mut result = Vec::new();
        let mut add = |kind, method, args| {
            if !self.responses.contains_key(&kind) {
                result.push(ReadRequest {
                    kind,
                    plan: RequestPlan::new(method, "load-workflow-editor", args),
                });
            }
        };
        add(
            ReadKind::Restriction,
            "functions.workflows.getIsRestricted",
            json!({"workflow_id":self.workflow}),
        );
        add(
            ReadKind::Latest,
            "functions.workflows.getLatest",
            json!({"workflow_id":self.workflow}),
        );
        let Some(latest) = self.responses.get(&ReadKind::Latest) else {
            return result;
        };
        add(
            ReadKind::Triggers,
            "workflows.triggers.list",
            json!({"app_id":latest["decorated_workflow"]["app_id"]}),
        );
        let Some(triggers) = self.responses.get(&ReadKind::Triggers) else {
            return result;
        };
        if let Some(trigger) = triggers["triggers"]
            .as_array()
            .expect("decoded triggers")
            .first()
        {
            add(
                ReadKind::TriggerInfo,
                "workflows.triggers.info",
                json!({"trigger_id":trigger["id"]}),
            );
            if !self.responses.contains_key(&ReadKind::TriggerInfo) {
                return result;
            }
        }
        add(
            ReadKind::Preferences,
            "workflows.triggers.prefs.get",
            json!({"workflow_id":self.workflow}),
        );
        // Source acquires external preferences before the permission pair.
        if !self.responses.contains_key(&ReadKind::Preferences) {
            return result;
        }
        add(
            ReadKind::CopyPermissions,
            "functions.workflows.permissions.copy.list",
            json!({"workflow_id":self.workflow}),
        );
        if let Some(info) = self.responses.get(&ReadKind::TriggerInfo) {
            add(
                ReadKind::RunPermissions,
                "workflows.triggers.permissions.list",
                json!({"trigger_id":info["trigger"]["id"]}),
            );
        }
        result
    }
    pub fn accept(&mut self, kind: ReadKind, raw: Value) -> Result<(), Error> {
        if !self.requests().iter().any(|r| r.kind == kind) {
            return Err(invalid());
        }
        success(&raw)?;
        match kind {
            ReadKind::Latest => {
                let workflow = &raw["decorated_workflow"];
                if identifier(workflow, "id")? != self.workflow {
                    return Err(invalid());
                }
                identifier(workflow, "app_id")?;
                array(workflow, "steps")?;
            }
            ReadKind::Triggers => {
                for trigger in array(&raw, "triggers")? {
                    identifier(trigger, "id")?;
                }
            }
            ReadKind::TriggerInfo => {
                let wanted = &self.responses[&ReadKind::Triggers]["triggers"][0]["id"];
                if &raw["trigger"]["id"] != wanted {
                    return Err(invalid());
                }
                identifier(&raw["trigger"], "id")?;
                // The response carries its own workflow identity; never infer it
                // from an About app or a trigger ID supplied by the UI.
                if raw["trigger"]["workflow"]["workflow_id"].as_str() != Some(&self.workflow)
                    || raw["trigger"]["workflow"]["app_id"]
                        != self.responses[&ReadKind::Latest]["decorated_workflow"]["app_id"]
                {
                    return Err(invalid());
                }
            }
            ReadKind::Restriction => {
                if !raw["is_restricted"].is_boolean()
                    || !raw["restricted_by_connector_steps_only"].is_boolean()
                {
                    return Err(invalid());
                }
            }
            ReadKind::CopyPermissions | ReadKind::RunPermissions => {
                if raw["permission_type"].as_str().is_none_or(str::is_empty) {
                    return Err(invalid());
                }
            }
            ReadKind::Preferences => {}
        }
        self.responses.insert(kind, raw);
        Ok(())
    }
    pub fn responses(&self) -> &BTreeMap<ReadKind, Value> {
        &self.responses
    }
    pub fn editor(&self) -> Result<Editor, Error> {
        if !self.responses.contains_key(&ReadKind::CopyPermissions)
            || self.responses.contains_key(&ReadKind::TriggerInfo)
                && !self.responses.contains_key(&ReadKind::RunPermissions)
        {
            return Err(invalid());
        }
        Ok(Editor { load: self.clone() })
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Editor {
    load: EditorLoad,
}
impl Editor {
    /// Native write preflight: exact acquired planning inputs, excluding only
    /// transport envelope metadata. This is a conflict check, never a receipt.
    /// Unknown domain fields remain significant, including future preferences.
    pub fn same_revision(&self, other: &Self) -> bool {
        fn payload(raw: &Value) -> Value {
            let Some(mut object) = raw.as_object().cloned() else {
                return raw.clone();
            };
            for key in ["ok", "error", "response_metadata", "request_id"] {
                object.remove(key);
            }
            Value::Object(object)
        }
        self.workflow_id() == other.workflow_id()
            && self.load.responses.len() == other.load.responses.len()
            && self.load.responses.iter().all(|(kind, raw)| {
                other
                    .load
                    .responses
                    .get(kind)
                    .is_some_and(|current| payload(raw) == payload(current))
            })
    }
    /// Desired copy access with every other acquired domain input unchanged.
    /// This observation never changes a mutation receipt or proves delivery.
    pub fn copy_permission_observation(&self, current: &Self, desired: &str) -> super::Observation {
        if !matches!(desired, "everyone" | "collaborators")
            || self.workflow_id() != current.workflow_id()
        {
            return super::Observation::Unavailable;
        }
        let mut expected = self.clone();
        expected
            .load
            .responses
            .get_mut(&ReadKind::CopyPermissions)
            .expect("loaded copy permissions")["permission_type"] = json!(desired);
        if expected.same_revision(current) {
            super::Observation::Matches
        } else {
            super::Observation::Different
        }
    }
    pub fn workflow_id(&self) -> &str {
        &self.load.workflow
    }
    pub fn raw_workflow(&self) -> &Value {
        &self.load.responses[&ReadKind::Latest]["decorated_workflow"]
    }
    pub fn latest_response(&self) -> &Value {
        &self.load.responses[&ReadKind::Latest]
    }
    pub fn trigger(&self) -> Option<&Value> {
        self.load
            .responses
            .get(&ReadKind::TriggerInfo)
            .map(|r| &r["trigger"])
    }
    pub fn external_preferences(&self) -> &Value {
        &self.load.responses[&ReadKind::Preferences]
    }
    pub fn copy_permissions(&self) -> &Value {
        &self.load.responses[&ReadKind::CopyPermissions]
    }
    pub fn run_permissions(&self) -> Option<&Value> {
        self.load.responses.get(&ReadKind::RunPermissions)
    }
    pub fn restriction(&self) -> Option<&Value> {
        self.load.responses.get(&ReadKind::Restriction)
    }
    pub fn responses(&self) -> &BTreeMap<ReadKind, Value> {
        &self.load.responses
    }
    /// Raw step acquisition is not the function-variable/connector capability
    /// producer. Neither a successful read nor `is_restricted:false` grants edit.
    pub fn steps(&self) -> &[Value] {
        self.raw_workflow()["steps"]
            .as_array()
            .expect("decoded steps")
    }
    /// CIuT.MP reads decorated workflow.collaborators, never About owner_ids.
    pub fn is_collaborator(&self, user: &str) -> bool {
        id(user)
            && self.raw_workflow()["collaborators"]
                .as_array()
                .is_some_and(|ids| ids.iter().any(|value| value.as_str() == Some(user)))
    }
    /// AjOk's view-only provider is true if MckU finds any unavailable step,
    /// including nested steps and missing product-level availability data.
    pub fn has_unavailable_steps(&self) -> bool {
        fn unavailable(steps: &[Value]) -> bool {
            steps.iter().any(|step| {
                step.pointer("/function/product_level_availability/is_available")
                    != Some(&Value::Bool(true))
                    || step["steps"]
                        .as_array()
                        .is_some_and(|nested| unavailable(nested))
            })
        }
        unavailable(self.steps())
    }
}

#[cfg(test)]
mod copy_tests;

mod permissions_observation;
