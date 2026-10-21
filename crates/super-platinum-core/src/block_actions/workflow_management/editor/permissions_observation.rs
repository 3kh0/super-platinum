use super::*;
use std::collections::BTreeSet;

const TARGETS: [&str; 4] = ["user_ids", "channel_ids", "team_ids", "org_ids"];

fn targets(raw: &Value, key: &str) -> Option<Value> {
    let Some(value) = raw.get(key) else {
        return Some(json!([]));
    };
    let values = value.as_array()?;
    let mut result = BTreeSet::new();
    for value in values {
        let value = value.as_str().filter(|v| id(v))?;
        result.insert(value);
    }
    Some(json!(result))
}

impl Editor {
    /// Compare the complete acquired settings with the desired copy/run state.
    /// This is readback, never proof of delivery and never a mutation receipt.
    /// `acknowledged_restriction` is the separately acquired raw refresh result;
    /// callers must retain its original request/owner, not infer it from a write.
    pub fn permissions_observation(
        &self,
        current: &Self,
        desired_copy: Option<&str>,
        desired_run: Option<&RequestPlan>,
        acknowledged_restriction: Option<&Value>,
    ) -> super::super::Observation {
        use super::super::Observation;
        if self.workflow_id() != current.workflow_id() {
            return Observation::Unavailable;
        }
        let mut expected = self.clone();
        let mut actual = current.clone();
        if let Some(copy) = desired_copy {
            if !matches!(copy, "everyone" | "collaborators") {
                return Observation::Unavailable;
            }
            expected
                .load
                .responses
                .get_mut(&ReadKind::CopyPermissions)
                .unwrap()["permission_type"] = json!(copy);
        }
        if let Some(run) = desired_run {
            let args = run.arguments();
            if run.method() != "workflows.triggers.permissions.set"
                || run.reason() != "set-trigger-run-permission"
                || self.trigger().is_none()
                || args["trigger_id"] != self.trigger().unwrap()["id"]
                || !matches!(
                    args["permission_type"].as_str(),
                    Some("everyone" | "app_collaborators" | "named_entities")
                )
                || args.as_object().is_none_or(|o| {
                    o.keys().any(|k| {
                        !TARGETS.contains(&k.as_str())
                            && !matches!(k.as_str(), "trigger_id" | "permission_type")
                    })
                })
            {
                return Observation::Unavailable;
            }
            let Some(want) = expected.load.responses.get_mut(&ReadKind::RunPermissions) else {
                return Observation::Unavailable;
            };
            let Some(have) = actual.load.responses.get_mut(&ReadKind::RunPermissions) else {
                return Observation::Unavailable;
            };
            want["permission_type"] = args["permission_type"].clone();
            // Every effective list participates, including lists absent from the
            // request. Otherwise unexpected targets could pass the comparison.
            for key in TARGETS {
                let (Some(wanted), Some(observed)) = (targets(args, key), targets(have, key))
                else {
                    return Observation::Unavailable;
                };
                want[key] = wanted;
                have[key] = observed;
            }
        }
        if let Some(raw) = acknowledged_restriction {
            // EditorLoad::accept is an acquisition gate, so an already loaded
            // kind cannot be accepted twice. Validate the independent refresh
            // through a fresh load, then replace only its exact expected raw row.
            let Ok(mut refresh) = EditorLoad::new(self.workflow_id()) else {
                return Observation::Unavailable;
            };
            if refresh.accept(ReadKind::Restriction, raw.clone()).is_err() {
                return Observation::Unavailable;
            }
            expected
                .load
                .responses
                .insert(ReadKind::Restriction, raw.clone());
        }
        if expected.same_revision(&actual) {
            Observation::Matches
        } else {
            Observation::Different
        }
    }
}

#[cfg(test)]
mod tests;
