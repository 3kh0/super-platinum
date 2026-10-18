//! Dependent source operations use actual primary/follow-up responses. No
//! transport is run here; unknown outcomes never authorize another stage.
mod context;
mod permissions;
use super::*;
pub use permissions::{ConnectorApps, EditPermissions};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FollowUpPhase {
    TriggerInputSynchronization,
    Interactivity,
    UserContext,
    Restriction,
    ConnectorPermissions,
    Complete,
}
#[derive(Clone, Debug, PartialEq)]
pub struct FollowUpStage {
    phase: FollowUpPhase,
    workflow: Value,
    available: Value,
    slots: Vec<Slot>,
}
impl FollowUpStage {
    pub fn phase(&self) -> FollowUpPhase {
        self.phase
    }
    pub fn workflow(&self) -> &Value {
        &self.workflow
    }
    pub fn available_data(&self) -> &Value {
        &self.available
    }
    pub fn slots(&self) -> &[Slot] {
        &self.slots
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct FollowUpPlan {
    primary: MutationPlan,
    workflow: Value,
    available: Value,
    expression: Option<Value>,
    trigger: Option<Value>,
    stages: Vec<FollowUpStage>,
    phase: FollowUpPhase,
    restriction: Option<Value>,
    connectors: Option<ConnectorApps>,
}
impl FollowUpPlan {
    pub fn new(primary: &MutationPlan) -> Result<Self, Error> {
        if !primary.primary_succeeded() {
            return Err(invalid());
        }
        let mut result = Self {
            primary: primary.clone(),
            workflow: primary.original().raw_workflow().clone(),
            available: primary.original().latest_response()["available_data"].clone(),
            expression: primary
                .original()
                .latest_response()
                .get("expression_data")
                .cloned(),
            trigger: primary.original().trigger().cloned(),
            stages: Vec::new(),
            phase: FollowUpPhase::Complete,
            restriction: None,
            connectors: None,
        };
        let needs_workflow = matches!(primary.command(), Command::Published(_) | Command::Step(_));
        if needs_workflow {
            let Outcome::Succeeded(raw) = primary.slots()[0].outcome() else {
                return Err(invalid());
            };
            result.adopt_workflow(raw)?;
            // 8dyy's step callers retain primary expression_data; J/ee only
            // fold workflow and available_data from follow-up responses.
            result.expression = raw.get("expression_data").cloned();
        }
        let phase = match primary.command() {
            Command::Published(_) if result.trigger.is_some() => {
                FollowUpPhase::TriggerInputSynchronization
            }
            Command::Step(_) if result.trigger.is_some() => FollowUpPhase::Interactivity,
            Command::Step(StepAction::Add { .. } | StepAction::Delete { .. })
            | Command::RunPermission { .. } => FollowUpPhase::Restriction,
            _ => FollowUpPhase::Complete,
        };
        result.enter(phase)?;
        Ok(result)
    }
    pub fn primary(&self) -> &MutationPlan {
        &self.primary
    }
    pub fn phase(&self) -> FollowUpPhase {
        self.phase
    }
    pub fn stages(&self) -> &[FollowUpStage] {
        &self.stages
    }
    pub fn current_workflow(&self) -> &Value {
        &self.workflow
    }
    pub fn current_trigger(&self) -> Option<&Value> {
        self.trigger.as_ref()
    }
    pub fn current_available_data(&self) -> &Value {
        &self.available
    }
    pub fn current_expression_data(&self) -> Option<&Value> {
        self.expression.as_ref()
    }
    pub fn restriction(&self) -> Option<&Value> {
        self.restriction.as_ref()
    }
    pub fn connectors(&self) -> Option<&ConnectorApps> {
        self.connectors.as_ref()
    }
    pub fn slots(&self) -> &[Slot] {
        self.stages
            .last()
            .filter(|stage| stage.phase == self.phase)
            .map(|stage| stage.slots.as_slice())
            .unwrap_or(&[])
    }
    pub fn ready(&self) -> Vec<usize> {
        self.slots()
            .iter()
            .enumerate()
            .filter(|(_, s)| s.outcome == Outcome::Pending)
            .map(|(i, _)| i)
            .collect()
    }
    pub fn mark_sent(&mut self, index: usize) -> Result<(), Error> {
        if !self.ready().contains(&index) {
            return Err(invalid());
        }
        self.stages.last_mut().ok_or_else(invalid)?.slots[index].outcome = Outcome::InFlight;
        Ok(())
    }
    pub fn settle(&mut self, index: usize, outcome: Outcome) -> Result<(), Error> {
        let slot = self
            .stages
            .last()
            .filter(|stage| stage.phase == self.phase)
            .and_then(|stage| stage.slots.get(index))
            .ok_or_else(invalid)?;
        if slot.outcome != Outcome::InFlight {
            return Err(invalid());
        }
        match &outcome {
            Outcome::Succeeded(raw) => {
                success(raw)?;
                match self.phase {
                    FollowUpPhase::Interactivity | FollowUpPhase::UserContext => {
                        self.validate_workflow(raw)?
                    }
                    FollowUpPhase::TriggerInputSynchronization => {
                        if raw["trigger"]["id"] != self.trigger.as_ref().ok_or_else(invalid)?["id"]
                            || raw["trigger"]["workflow"]["workflow_id"].as_str()
                                != Some(self.primary.original().workflow_id())
                        {
                            return Err(invalid());
                        }
                    }
                    FollowUpPhase::Restriction => {
                        if !raw["is_restricted"].is_boolean()
                            || !raw["restricted_by_connector_steps_only"].is_boolean()
                        {
                            return Err(invalid());
                        }
                    }
                    FollowUpPhase::ConnectorPermissions => {
                        ConnectorApps::decode(raw.clone())?;
                    }
                    FollowUpPhase::Complete => return Err(invalid()),
                }
            }
            Outcome::Rejected(raw) if raw.is_object() && raw["ok"] == json!(false) => {}
            Outcome::Unknown | Outcome::NotSent => {}
            _ => return Err(invalid()),
        }
        self.stages.last_mut().unwrap().slots[index].outcome = outcome;
        Ok(())
    }
    /// Fold all-settled results in request order, as 8dyy.J/ee do. Known failures
    /// remain recorded; unknown sends stop progression and cannot be retried.
    pub fn advance(&mut self) -> Result<(), Error> {
        let mut next = self.clone();
        next.advance_inner()?;
        *self = next;
        Ok(())
    }
    fn advance_inner(&mut self) -> Result<(), Error> {
        if self.phase == FollowUpPhase::Complete {
            return Err(invalid());
        }
        if self.slots().iter().any(|s| {
            matches!(
                s.outcome,
                Outcome::Pending | Outcome::InFlight | Outcome::Unknown
            )
        }) {
            return Err(invalid());
        }
        let receipts = self
            .slots()
            .iter()
            .filter_map(|slot| {
                if let Outcome::Succeeded(raw) = &slot.outcome {
                    Some(raw.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        for raw in receipts {
            match self.phase {
                FollowUpPhase::Interactivity | FollowUpPhase::UserContext => {
                    self.adopt_workflow(&raw)?
                }
                FollowUpPhase::TriggerInputSynchronization => {
                    self.trigger = Some(raw["trigger"].clone())
                }
                FollowUpPhase::Restriction => self.restriction = Some(raw),
                FollowUpPhase::ConnectorPermissions => {
                    self.connectors = Some(ConnectorApps::decode(raw)?)
                }
                FollowUpPhase::Complete => return Err(invalid()),
            }
        }
        self.enter(self.next_phase())
    }
    pub fn fully_succeeded(&self) -> bool {
        self.phase == FollowUpPhase::Complete
            && self.stages.iter().all(|stage| {
                stage
                    .slots
                    .iter()
                    .all(|s| matches!(s.outcome, Outcome::Succeeded(_)))
            })
    }
    fn validate_workflow(&self, raw: &Value) -> Result<(), Error> {
        success(raw)?;
        let workflow = &raw["decorated_workflow"];
        if workflow["id"].as_str() != Some(self.primary.original().workflow_id())
            || workflow["app_id"] != self.primary.original().raw_workflow()["app_id"]
            || !raw["available_data"].is_object()
        {
            return Err(invalid());
        }
        array(workflow, "steps")?;
        Ok(())
    }
    fn adopt_workflow(&mut self, raw: &Value) -> Result<(), Error> {
        self.validate_workflow(raw)?;
        self.workflow = raw["decorated_workflow"].clone();
        self.available = raw["available_data"].clone();
        Ok(())
    }
    fn next_phase(&self) -> FollowUpPhase {
        match self.phase {
            FollowUpPhase::Interactivity => FollowUpPhase::UserContext,
            FollowUpPhase::UserContext
                if matches!(
                    self.primary.command(),
                    Command::Step(StepAction::Add { .. } | StepAction::Delete { .. })
                ) =>
            {
                FollowUpPhase::Restriction
            }
            FollowUpPhase::Restriction
                if matches!(
                    self.primary.command(),
                    Command::Step(StepAction::Delete { .. })
                ) =>
            {
                FollowUpPhase::ConnectorPermissions
            }
            _ => FollowUpPhase::Complete,
        }
    }
    fn enter(&mut self, mut phase: FollowUpPhase) -> Result<(), Error> {
        // Empty stages are actual source no-ops; a fixed finite phase graph.
        for _ in 0..6 {
            self.phase = phase;
            if phase == FollowUpPhase::Complete {
                return Ok(());
            }
            let plans = match phase {
                FollowUpPhase::TriggerInputSynchronization => TriggerUpdate::synchronize(
                    &self.workflow,
                    self.trigger.as_ref().ok_or_else(invalid)?,
                )?
                .into_iter()
                .collect(),
                FollowUpPhase::Interactivity => context::interactivity(
                    &self.workflow,
                    &self.available,
                    self.trigger.as_ref().ok_or_else(invalid)?,
                )?,
                FollowUpPhase::UserContext => context::user_context(
                    &self.workflow,
                    &self.available,
                    self.trigger.as_ref().ok_or_else(invalid)?,
                )?,
                FollowUpPhase::Restriction => vec![RequestPlan::new(
                    "functions.workflows.getIsRestricted",
                    match self.primary.command() {
                        Command::Step(StepAction::Add { .. }) => "add-workflow-step",
                        Command::Step(StepAction::Delete { .. }) => "remove-workflow-step",
                        _ => "set-trigger-run-permission",
                    },
                    json!({"workflow_id":self.primary.original().workflow_id()}),
                )],
                FollowUpPhase::ConnectorPermissions => vec![ConnectorApps::request()],
                FollowUpPhase::Complete => unreachable!(),
            };
            let empty = plans.is_empty();
            self.stages.push(FollowUpStage {
                phase,
                workflow: self.workflow.clone(),
                available: self.available.clone(),
                slots: plans
                    .into_iter()
                    .map(|plan| Slot {
                        plan,
                        prerequisite: None,
                        outcome: Outcome::Pending,
                    })
                    .collect(),
            });
            if !empty {
                return Ok(());
            }
            phase = self.next_phase();
        }
        Err(invalid())
    }
}
