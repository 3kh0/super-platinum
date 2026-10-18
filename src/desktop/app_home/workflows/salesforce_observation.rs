//! Acquire actual experiment observations for the native Salesforce service.
//! A renderer URL, organization field or inferred default never chooses a branch.
use super::*;
use super_platinum_core::block_actions::home::workflow::salesforce::native::NativeContext;

const SUPPORT: &str = "sfdc_sales_home_user_config_workflow_support";
const JWT: &str = "slackforce_connected_app_deprecation_use_org_jwt";

#[derive(Clone)]
pub(super) struct Observation {
    origin: Origin,
    native: NativeContext,
    assignments: Value,
}
impl Observation {
    #[cfg(test)]
    pub(super) fn fixture(origin: Origin, assignments: Value) -> Result<Self, Error> {
        let native = decode(&origin, &assignments)?;
        Ok(Self {
            origin,
            native,
            assignments,
        })
    }
    pub(super) fn native(&self) -> &NativeContext {
        &self.native
    }
    pub(super) fn current(&self, s: &ShellState) -> bool {
        self.origin.current(s)
            && decode(&self.origin, &self.assignments).is_ok_and(|native| native == self.native)
    }
    pub(super) fn same(&self, other: &Self) -> bool {
        self.native == other.native
            && self.origin.owner.home_visit == other.origin.owner.home_visit
            && self.origin.workflow == other.origin.workflow
            && self.origin.input_revision == other.origin.input_revision
    }
}
pub(super) fn needs_context(review: &Preflight) -> bool {
    // Complete Tin+.Ei output set from the retained commercial/gov/dev branches.
    review.connections().iter().any(|a| possible_app(&a.app))
}
pub(super) fn possible_app(app: &str) -> bool {
    matches!(
        app,
        "A04T99UKKQE" | "A014JPFMT8T" | "A1W53KSJQ" | "A07846N4L"
    )
}
pub(super) fn prime(mut state: Signal<ShellState>, dialog: u64) {
    let needed = state
        .peek()
        .app_home
        .workflow
        .as_ref()
        .is_some_and(|d| d.id == dialog && needs_context(&d.review));
    if !needed {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let Ok(observation) = read(state, dialog).await else {
            return;
        };
        let mut s = state.write();
        if observation.current(&s) {
            if let Some(d) = s
                .app_home
                .workflow
                .as_mut()
                .filter(|d| d.id == dialog && d.salesforce_view.is_none())
            {
                d.salesforce_observation = Some(observation);
            }
        }
    });
}
pub(super) async fn read(mut state: Signal<ShellState>, dialog: u64) -> Result<Observation, Error> {
    let (origin, review, generation, request) = {
        let mut s = state.write();
        if !dialog_current(&s, dialog) {
            return Err(unavailable());
        }
        let d = s.app_home.workflow.as_ref().unwrap();
        if cooling(&s, &d.origin.owner.ctx) {
            return Err(unavailable());
        }
        let origin = d.origin.clone();
        let review = d.review.clone();
        let generation = next();
        let request = api::list_experiments(&origin.owner.ctx.client, &origin.owner.ctx.session);
        s.app_home.workflow.as_mut().unwrap().salesforce_query = generation;
        (origin, review, generation, request)
    };
    let result = io::read(state, &origin, request).await;
    {
        let mut s = state.write();
        if let Err(error) = &result {
            failure(&mut s, &origin.owner.ctx, error);
        }
        if !origin.current(&s)
            || !s.app_home.workflow.as_ref().is_some_and(|d| {
                d.id == dialog && d.salesforce_query == generation && d.review == review
            })
        {
            return Err(unavailable());
        }
    }
    let assignments = result?;
    let native = decode(&origin, &assignments)?;
    Ok(Observation {
        origin,
        native,
        assignments,
    })
}
fn decode(origin: &Origin, raw: &Value) -> Result<NativeContext, Error> {
    let (support, jwt) = groups(raw)?;
    NativeContext::observe(
        &origin.owner.ctx.client,
        &origin.owner.ctx.session,
        support,
        jwt,
    )
    .map_err(|_| unavailable())
}
fn groups(raw: &Value) -> Result<(&Value, &Value), Error> {
    if raw.get("ok") != Some(&Value::Bool(true)) {
        return Err(unavailable());
    }
    let assignments = raw
        .get("assignments")
        .and_then(Value::as_object)
        .ok_or_else(unavailable)?;
    let group = |key| {
        assignments
            .get(key)
            .and_then(|v| v.get("group"))
            .filter(|v| v.as_str().is_some_and(|s| !s.is_empty() && s.len() <= 128))
            .ok_or_else(unavailable)
    };
    Ok((group(SUPPORT)?, group(JWT)?))
}
fn unavailable() -> Error {
    Error::Api("home_workflow_salesforce_observations_unavailable".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn branch_observations_preserve_exact_groups_and_never_default_missing_assignment() {
        let raw = serde_json::json!({"ok":true,"assignments":{SUPPORT:{"group":"on"},JWT:{"group":"control"}}});
        let (support, jwt) = groups(&raw).unwrap();
        assert_eq!(support, "on");
        assert_eq!(jwt, "control");
        for raw in [
            serde_json::json!({"ok":true,"assignments":{SUPPORT:{"group":"on"}}}),
            serde_json::json!({"ok":true,"assignments":{SUPPORT:{"group":true},JWT:{"group":"on"}}}),
            serde_json::json!({"ok":false,"assignments":{SUPPORT:{"group":"on"},JWT:{"group":"on"}}}),
        ] {
            assert!(groups(&raw).is_err());
        }
    }
}
