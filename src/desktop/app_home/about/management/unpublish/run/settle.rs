use super::*;
pub(in crate::app_home::about::management::unpublish) fn settle(
    s: &mut ShellState,
    id: u64,
    primary: bool,
    result: Result<Value, Error>,
) {
    let Some(a) = attempt_mut(s, id) else { return };
    if primary {
        if a.replies[0].is_some() || !matches!(a.plan.slots()[0].outcome(), Outcome::InFlight) {
            return;
        }
    } else if a.followup.as_ref().is_none_or(|p| {
        p.slots()
            .first()
            .is_none_or(|s| !matches!(s.outcome(), Outcome::InFlight))
    }) {
        return;
    }
    let (mut outcome, reply) = classify(result);
    if primary {
        // A generic ok is not a decorated publication response. Validate the
        // complete new workflow before it can grant synchronization authority.
        if matches!(outcome, Outcome::Succeeded(_)) {
            let mut trial = a.plan.clone();
            let valid = trial.settle(0, outcome.clone()).is_ok()
                && protocol::FollowUpPlan::new(&trial)
                    .is_ok_and(|p| p.current_workflow()["is_published"] == false);
            if !valid {
                outcome = Outcome::Unknown;
            }
        }
        if a.plan.settle(0, outcome).is_err() {
            let _ = a.plan.settle(0, Outcome::Unknown);
        }
        a.replies[0] = Some(reply);
        if a.plan.primary_succeeded() {
            a.followup = protocol::FollowUpPlan::new(&a.plan).ok();
            a.follow_replies = a
                .followup
                .as_ref()
                .map(|p| {
                    p.stages()
                        .iter()
                        .map(|s| s.slots().iter().map(|_| None).collect())
                        .collect()
                })
                .unwrap_or_default();
        }
    } else {
        let p = a.followup.as_mut().unwrap();
        let stage = p.stages().len() - 1;
        if a.follow_replies[stage][0].is_some() {
            return;
        }
        if p.settle(0, outcome).is_err() {
            let _ = p.settle(0, Outcome::Unknown);
        }
        a.follow_replies[stage][0] = Some(reply);
        let _ = p.advance(); // Unknown stays stopped; rejected remains non-success.
    }
}
fn classify(result: Result<Value, Error>) -> (Outcome, Reply) {
    match result {
        Ok(raw) => {
            let outcome = if raw.is_object()
                && raw["ok"] == true
                && raw.get("error").is_none_or(Value::is_null)
            {
                Outcome::Succeeded(raw.clone())
            } else if raw.is_object() && raw["ok"] == false {
                Outcome::Rejected(raw.clone())
            } else {
                Outcome::Unknown
            };
            (outcome, Reply::Raw(raw))
        }
        Err(Error::Offline(_)) => (Outcome::NotSent, Reply::Failure("offline before dispatch")),
        Err(Error::Api(code)) if code.starts_with("workflow_management_not_sent_") => {
            (Outcome::NotSent, Reply::Failure("refused before dispatch"))
        }
        Err(_) => (Outcome::Unknown, Reply::Failure("delivery unconfirmed")),
    }
}
