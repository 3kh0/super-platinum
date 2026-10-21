//! Desired publication/synchronization observation; never receipt settlement.
use super::*;
impl FollowUpPlan {
    pub fn publication_observation(&self, current: &Editor) -> Observation {
        let Command::Published(published) = self.primary.command() else {
            return Observation::Unavailable;
        };
        let raw = current.raw_workflow();
        if raw["id"] != self.workflow["id"] || raw["app_id"] != self.workflow["app_id"] {
            return Observation::Unavailable;
        }
        let Some(actual) = raw["is_published"].as_bool() else {
            return Observation::Unavailable;
        };
        if actual != *published
            || raw.get("input_parameters") != self.workflow.get("input_parameters")
        {
            return Observation::Different;
        }
        let matches = match (self.primary.original().trigger(), current.trigger()) {
            (None, None) => true,
            (Some(original), Some(actual)) => {
                // Rejected/lost synchronization leaves self.trigger at the old
                // receipt. Observe the actual request target, never that old
                // baseline, and never infer ACK from matching server state.
                let desired = self
                    .stages
                    .iter()
                    .filter(|s| s.phase == FollowUpPhase::TriggerInputSynchronization)
                    .flat_map(|s| &s.slots)
                    .next()
                    .map(|s| &s.plan.arguments()["inputs"])
                    .unwrap_or(&original["inputs"]);
                if !desired.is_object() || !actual["inputs"].is_object() {
                    return Observation::Unavailable;
                }
                actual["id"] == original["id"]
                    && actual["workflow"]["workflow_id"] == original["workflow"]["workflow_id"]
                    && actual["inputs"] == *desired
            }
            _ => false,
        };
        if matches {
            Observation::Matches
        } else {
            Observation::Different
        }
    }
}
