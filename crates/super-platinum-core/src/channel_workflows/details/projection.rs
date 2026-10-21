use super::*;

/// Optional acquired workflow is correlated to the original info reply. The
/// projection never treats preview/bookmark labels as fresh details metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct Projection {
    trigger: Trigger,
    workflow: Option<Workflow>,
}
impl Projection {
    pub fn new(trigger: &Trigger, workflow: Option<&Workflow>) -> Result<Self, Failure> {
        if workflow.is_some_and(|w| trigger.workflow_id() != Some(w.id())) {
            return Err(Failure::malformed(
                workflow.expect("present mismatched workflow").raw(),
            ));
        }
        Ok(Self {
            trigger: trigger.clone(),
            workflow: workflow.cloned(),
        })
    }
    pub fn trigger(&self) -> &Trigger {
        &self.trigger
    }
    pub fn workflow(&self) -> Option<&Workflow> {
        self.workflow.as_ref()
    }
    pub fn title(&self) -> Option<&str> {
        self.metadata("name", "title")
    }
    pub fn description(&self) -> Option<&str> {
        self.metadata("description", "description")
    }
    fn metadata(&self, trigger_key: &str, workflow_key: &str) -> Option<&str> {
        let trigger = self.trigger.trigger();
        nonempty(trigger.get(trigger_key))
            .or_else(|| nonempty(trigger.get("workflow")?.get(workflow_key)))
            .or_else(|| nonempty(self.workflow.as_ref()?.workflow().get(workflow_key)))
    }
    /// Exact server type, not an invented human label or a run permission.
    pub fn trigger_type(&self) -> Option<&str> {
        nonempty(self.trigger.trigger().get("type"))
    }
    pub fn workflow_id(&self) -> Option<&str> {
        self.trigger.workflow_id()
    }
    pub fn app(&self) -> Option<&Value> {
        self.trigger
            .trigger()
            .pointer("/workflow/app")
            .filter(|v| v.is_object())
    }
    pub fn app_id(&self) -> Option<&str> {
        nonempty(self.trigger.trigger().pointer("/workflow/app_id"))
            .or_else(|| nonempty(self.app()?.get("id")))
    }
    pub fn owning_team(&self) -> Option<&str> {
        nonempty(self.trigger.trigger().get("owning_team_id"))
    }
    pub fn collaborators(&self) -> Option<&[Value]> {
        self.workflow
            .as_ref()
            .and_then(|w| w.workflow().get("collaborators"))
            .and_then(Value::as_array)
            .or_else(|| {
                self.trigger
                    .trigger()
                    .pointer("/workflow_details/collaborators")
                    .and_then(Value::as_array)
            })
            .map(Vec::as_slice)
    }
    /// wcWH chooses the complete acquired workflow over inline details. An
    /// observed empty steps array stays empty; absent steps remain unknown.
    pub fn workflow_details(&self) -> Option<&Value> {
        self.workflow.as_ref().map(Workflow::workflow).or_else(|| {
            self.trigger
                .trigger()
                .get("workflow_details")
                .filter(|v| v.is_object())
        })
    }
    pub fn raw_steps(&self) -> Option<&[Value]> {
        self.workflow_details()?
            .get("steps")?
            .as_array()
            .map(Vec::as_slice)
    }
}
fn nonempty(value: Option<&Value>) -> Option<&str> {
    value.and_then(Value::as_str).filter(|s| !s.is_empty())
}
