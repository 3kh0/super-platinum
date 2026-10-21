//! Permissions manager edits. IDs are selected values, not eligibility grants.
//! The host retains canonical observations: an existing hydrated manager may
//! remain selected even when the picker would exclude it as a new candidate.
use super::*;
use std::collections::BTreeSet;
pub mod eligibility;

#[derive(Clone, Debug, PartialEq)]
pub struct Draft {
    original: Editor,
    self_id: String,
    selected: Vec<Option<String>>,
}
impl Draft {
    pub fn new(original: &Editor, self_id: &str) -> Result<Self, Error> {
        if !id(self_id) {
            return Err(invalid());
        }
        Ok(Self {
            original: original.clone(),
            self_id: self_id.into(),
            selected: collaborators(original.raw_workflow())?
                .into_iter()
                .map(Some)
                .collect(),
        })
    }
    /// None is an unresolved native token. This never promotes an ID to a
    /// canonical selectable member or discards an original filtered grant.
    pub fn select(&mut self, selected: Vec<Option<String>>) {
        self.selected = selected;
    }
    pub fn selected(&self) -> &[Option<String>] {
        &self.selected
    }
    pub fn resolved(&self) -> Result<Vec<String>, Error> {
        let ids = self
            .selected
            .iter()
            .cloned()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(invalid)?;
        if ids.is_empty() || ids.iter().any(|s| !id(s)) {
            return Err(invalid());
        }
        Ok(unique(ids))
    }
    pub fn self_removal(&self) -> bool {
        !self
            .selected
            .iter()
            .any(|s| s.as_deref() == Some(self.self_id.as_str()))
    }
    /// Source Set([...selectedManagers, ...previousNamedRunTargets]). Removing
    /// a manager does not remove an existing run target. No manager count cap.
    pub fn union_named_run(&self, previous: &[String]) -> Result<Vec<String>, Error> {
        if previous.iter().any(|s| !id(s)) {
            return Err(invalid());
        }
        Ok(unique(
            self.resolved()?.into_iter().chain(previous.iter().cloned()),
        ))
    }
    pub fn plan(&self) -> Result<Plan, Error> {
        let selected = self.resolved()?;
        let original = collaborators(self.original.raw_workflow())?;
        let added: Vec<_> = selected
            .iter()
            .filter(|s| !original.contains(s))
            .cloned()
            .collect();
        let removed: Vec<_> = original
            .iter()
            .filter(|s| !selected.contains(s))
            .cloned()
            .collect();
        let mut slots: Vec<Slot> = Vec::new();
        let mut expected = Vec::new();
        let mut interim = original;
        for command in [
            (!added.is_empty()).then(|| Command::AddCollaborators(added.clone())),
            (!removed.is_empty()).then(|| Command::RemoveCollaborators(removed.clone())),
        ]
        .into_iter()
        .flatten()
        {
            match &command {
                Command::AddCollaborators(ids) => interim.extend(ids.iter().cloned()),
                Command::RemoveCollaborators(ids) => interim.retain(|s| !ids.contains(s)),
                _ => unreachable!(),
            }
            let plan = MutationPlan::new(&self.original, command)?;
            let mut slot = plan.slots()[0].clone();
            slot.prerequisite = slots.len().checked_sub(1);
            slots.push(slot);
            expected.push(interim.clone());
        }
        let responses = vec![None; slots.len()];
        Ok(Plan {
            original: self.original.clone(),
            selected,
            self_removal: self.self_removal(),
            slots,
            expected,
            responses,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    original: Editor,
    selected: Vec<String>,
    self_removal: bool,
    slots: Vec<Slot>,
    expected: Vec<Vec<String>>,
    responses: Vec<Option<Value>>,
}
impl Plan {
    pub fn original(&self) -> &Editor {
        &self.original
    }
    pub fn selected(&self) -> &[String] {
        &self.selected
    }
    pub fn self_removal(&self) -> bool {
        self.self_removal
    }
    pub fn slots(&self) -> &[Slot] {
        &self.slots
    }
    /// Includes malformed/ambiguous raw replies whose outcome is Unknown.
    pub fn responses(&self) -> &[Option<Value>] {
        &self.responses
    }
    pub fn complete(&self) -> bool {
        self.slots
            .iter()
            .all(|s| matches!(s.outcome, Outcome::Succeeded(_)))
    }
    pub fn next(&self) -> Option<usize> {
        self.slots.iter().position(|s| {
            s.outcome == Outcome::Pending
                && s.prerequisite
                    .is_none_or(|p| matches!(self.slots[p].outcome, Outcome::Succeeded(_)))
        })
    }
    pub fn mark_sent(&mut self, index: usize) -> Result<(), Error> {
        if self.next() != Some(index) {
            return Err(invalid());
        }
        self.slots[index].outcome = Outcome::InFlight;
        Ok(())
    }
    pub fn settle_response(&mut self, index: usize, raw: Value) -> Result<(), Error> {
        if self
            .slots
            .get(index)
            .is_none_or(|s| s.outcome != Outcome::InFlight)
        {
            return Err(invalid());
        }
        let outcome = if raw.is_object() && raw["ok"] == json!(false) {
            Outcome::Rejected(raw.clone())
        } else if self.acknowledged(index, &raw) {
            Outcome::Succeeded(raw.clone())
        } else {
            Outcome::Unknown
        };
        self.responses[index] = Some(raw);
        self.slots[index].outcome = outcome;
        Ok(())
    }
    /// Only original dispatch evidence can establish not-sent. A lost response
    /// is Unknown and never opens the dependent remove slot or permits replay.
    pub fn settle_failure(&mut self, index: usize, not_sent: bool) -> Result<(), Error> {
        let slot = self.slots.get_mut(index).ok_or_else(invalid)?;
        if slot.outcome != Outcome::InFlight {
            return Err(invalid());
        }
        slot.outcome = if not_sent {
            Outcome::NotSent
        } else {
            Outcome::Unknown
        };
        Ok(())
    }
    fn acknowledged(&self, index: usize, raw: &Value) -> bool {
        success(raw).is_ok()
            && raw["decorated_workflow"]["id"] == self.original.raw_workflow()["id"]
            && raw["decorated_workflow"]["app_id"] == self.original.raw_workflow()["app_id"]
            && raw["decorated_workflow"]["steps"].is_array()
            && collaborators(&raw["decorated_workflow"])
                .is_ok_and(|ids| same_ids(&ids, &self.expected[index]))
    }
    /// The actual last acknowledged source publication, never a synthetic
    /// collaborator substitution. Missing optional fields preserve omission.
    pub fn acknowledged_editor(&self) -> Result<Editor, Error> {
        let mut latest = self.original.latest_response().clone();
        if let Some(raw) = self
            .slots
            .iter()
            .rev()
            .find_map(|slot| match &slot.outcome {
                Outcome::Succeeded(raw) => Some(raw),
                _ => None,
            })
        {
            for key in ["decorated_workflow", "available_data", "expression_data"] {
                if let Some(value) = raw.get(key) {
                    latest[key] = value.clone();
                } else {
                    latest.as_object_mut().ok_or_else(invalid)?.remove(key);
                }
            }
        }
        rebuild(&self.original, latest)
    }

    /// Readback remains independent of complete(). Partial/Unknown plans may
    /// match desired IDs, but can never become acknowledged or unsealed here.
    pub fn observation(
        &self,
        current: &Editor,
        desired_copy: Option<&str>,
        desired_run: Option<&RequestPlan>,
        restriction: Option<&Value>,
    ) -> Observation {
        let original = &self.original;
        // Desired comparison only, never publication or an ACK. Canonical order
        // is not authority: compare collaborator sets while retaining every
        // other field from the original independently acquired editor.
        let mut latest = original.latest_response().clone();
        let mut desired = self.selected.clone();
        desired.sort();
        latest["decorated_workflow"]["collaborators"] = json!(desired);
        let Ok(expected) = rebuild(original, latest) else {
            return Observation::Unavailable;
        };
        let Ok(mut observed_ids) = collaborators(current.raw_workflow()) else {
            return Observation::Unavailable;
        };
        observed_ids.sort();
        let mut actual_latest = current.latest_response().clone();
        actual_latest["decorated_workflow"]["collaborators"] = json!(observed_ids);
        let Ok(actual) = rebuild(current, actual_latest) else {
            return Observation::Unavailable;
        };
        if !collaborators(current.raw_workflow()).is_ok_and(|ids| same_ids(&ids, &self.selected)) {
            return Observation::Different;
        }
        expected.permissions_observation(&actual, desired_copy, desired_run, restriction)
    }
}

fn rebuild(original: &Editor, latest: Value) -> Result<Editor, Error> {
    let mut load = EditorLoad::new(original.workflow_id())?;
    for kind in [
        ReadKind::Latest,
        ReadKind::Restriction,
        ReadKind::Triggers,
        ReadKind::TriggerInfo,
        ReadKind::Preferences,
        ReadKind::CopyPermissions,
        ReadKind::RunPermissions,
    ] {
        if let Some(raw) = original.responses().get(&kind) {
            load.accept(
                kind,
                if kind == ReadKind::Latest {
                    latest.clone()
                } else {
                    raw.clone()
                },
            )?;
        }
    }
    load.editor()
}

fn unique(ids: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    ids.into_iter().filter(|s| seen.insert(s.clone())).collect()
}
fn collaborators(workflow: &Value) -> Result<Vec<String>, Error> {
    let ids = workflow["collaborators"].as_array().ok_or_else(invalid)?;
    let mut result = Vec::new();
    for value in ids {
        let value = value.as_str().filter(|s| id(s)).ok_or_else(invalid)?;
        if result.iter().any(|s| s == value) {
            return Err(invalid());
        }
        result.push(value.to_owned());
    }
    Ok(result)
}
fn same_ids(a: &[String], b: &[String]) -> bool {
    a.iter().collect::<BTreeSet<_>>() == b.iter().collect::<BTreeSet<_>>()
}

#[cfg(test)]
mod tests;
