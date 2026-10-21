use super::*;
use crate::app_home::inputs::native::Release;

pub(super) struct Attempt {
    id: u64,
    document: u64,
    lease: read::Lease,
    revision: u64,
    title: String,
    description: String,
    policy: policy::Snapshot,
    plan: MutationPlan,
    // Even malformed callbacks and uncertain transport outcomes remain separate
    // from semantic slot classification; observation never rewrites either.
    replies: Vec<Option<Reply>>,
    finished: bool,
    resolved: bool,
    release: Option<Release>,
}
enum Reply {
    Raw(Value),
    Failure(&'static str),
}
pub(super) fn exact(s: &ShellState, id: u64) -> bool {
    let Some(a) = s
        .app_home
        .about
        .management
        .attempts
        .iter()
        .find(|a| a.id == id)
    else {
        return false;
    };
    a.lease.current(s)
        && document(s, a.lease.visit).is_some_and(|d| {
            d.id == a.document
                && d.pending == Some(id)
                && d.revision == a.revision
                && d.title == a.title
                && d.description == a.description
        })
}
pub(super) fn save(mut state: Signal<ShellState>, visit: u64) {
    let id = {
        let mut s = state.write();
        if !editable(&s, visit) {
            return;
        }
        let d = document(&s, visit).unwrap();
        if !d.dirty() {
            return;
        }
        let Ok(command) = d.editor.metadata_command(&d.title, &d.description, None) else {
            return;
        };
        let Ok(plan) = MutationPlan::new(&d.editor, command) else {
            return;
        };
        if !plan.follow_ups().is_empty() {
            return;
        }
        let (document, revision, title, description) =
            (d.id, d.revision, d.title.clone(), d.description.clone());
        if s.app_home.about.management.attempts.len() >= 128 {
            document_mut(&mut s, visit).unwrap().notice =
                Some("Retained workflow changes need review before another save.".into());
            return;
        }
        let policy = s
            .app_home
            .about
            .management
            .surface
            .as_ref()
            .unwrap()
            .policy
            .clone()
            .unwrap();
        let Some(lease) = read::Lease::begin(&mut s, visit) else {
            return;
        };
        let id = next();
        document_mut(&mut s, visit).unwrap().pending = Some(id);
        let replies = (0..plan.slots().len()).map(|_| None).collect();
        s.app_home.about.management.attempts.push(Attempt {
            id,
            document,
            lease,
            revision,
            title,
            description,
            policy,
            plan,
            replies,
            finished: false,
            resolved: false,
            release: None,
        });
        id
    };
    dioxus::core::spawn_forever(async move {
        run(state, id).await;
    });
}
async fn run(mut state: Signal<ShellState>, id: u64) {
    let Some(release) = native::lock(state, id).await else {
        finish(
            &mut state.write(),
            id,
            "The native text changed. No update was sent.",
        );
        return;
    };
    let (lease, baseline, old_policy) = {
        let mut s = state.write();
        let a = s
            .app_home
            .about
            .management
            .attempts
            .iter_mut()
            .find(|a| a.id == id)
            .unwrap();
        a.release = Some(release);
        (a.lease.clone(), a.plan.original().clone(), a.policy.clone())
    };
    let preflight = async {
        let policy = policy::acquire(state, &lease).await?;
        let fresh = read::load_editor(state, &lease, baseline.workflow_id()).await?;
        if !exact(&state.peek(), id)
            || !policy.current(&state.peek())
            || !policy.same(&old_policy)
            || !policy.can_create()
            || !allowed_editor(&fresh, &lease.owner.ctx.session.user_id)
            || !baseline.same_revision(&fresh)
        {
            return Err(unavailable());
        }
        Ok(())
    }
    .await;
    if preflight.is_err() {
        finish(
            &mut state.write(),
            id,
            "Workflow or permissions changed. No update was sent; your draft is retained.",
        );
        return;
    }
    let count = {
        state
            .peek()
            .app_home
            .about
            .management
            .attempts
            .iter()
            .find(|a| a.id == id)
            .unwrap()
            .plan
            .slots()
            .len()
    };
    for index in 0..count {
        let request = {
            let mut s = state.write();
            if !exact(&s, id) {
                break;
            }
            let a = s
                .app_home
                .about
                .management
                .attempts
                .iter_mut()
                .find(|a| a.id == id)
                .unwrap();
            if a.plan.mark_sent(index).is_err() {
                break;
            }
            a.plan.slots()[index]
                .plan()
                .request(&lease.owner.ctx.client, &lease.owner.ctx.session)
        };
        let result = if fixture::enabled(&state.peek()) {
            fixture::mutation(&mut state.write(), &request)
        } else if crate::fixture::is_fixture() {
            Err(Error::Api("workflow_management_not_sent_fixture".into()))
        } else {
            lease
                .owner
                .ctx
                .transport
                .execute_workflow_management(request)
                .await
        };
        let mut s = state.write();
        if let Err(Error::RateLimited { retry_after_secs }) = &result {
            navigation::retain_limit(&mut s, &lease.owner.ctx, *retry_after_secs);
        }
        settle(&mut s, id, index, result);
    }
    finish(
        &mut state.write(),
        id,
        "Update responses retained. Check the current workflow before making another change.",
    );
    if visible(&state.peek(), lease.visit) {
        read::editor(state, lease.visit, baseline.workflow_id().into());
    }
}
fn settle(s: &mut ShellState, id: u64, index: usize, result: Result<Value, Error>) {
    let Some(a) = s
        .app_home
        .about
        .management
        .attempts
        .iter_mut()
        .find(|a| a.id == id)
    else {
        return;
    };
    let (outcome, reply) = match result {
        Ok(raw) => {
            let outcome = if raw["ok"] == true && raw.get("error").is_none_or(Value::is_null) {
                Outcome::Succeeded(raw.clone())
            } else if raw["ok"] == false {
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
    };
    a.replies[index] = Some(reply);
    if a.plan.settle(index, outcome).is_err() {
        let _ = a.plan.settle(index, Outcome::Unknown);
    }
}
fn finish(s: &mut ShellState, id: u64, notice: &str) {
    let Some(a) = s
        .app_home
        .about
        .management
        .attempts
        .iter_mut()
        .find(|a| a.id == id)
    else {
        return;
    };
    a.finished = true;
    if let Some(release) = a.release.take() {
        release.release();
    }
    let lease = a.lease.clone();
    let doc = a.document;
    let no_send = a
        .plan
        .slots()
        .iter()
        .all(|slot| matches!(slot.outcome(), Outcome::Pending | Outcome::NotSent));
    if no_send {
        a.resolved = true;
    }
    if let Some(d) = s
        .app_home
        .about
        .management
        .documents
        .iter_mut()
        .find(|d| d.id == doc && d.pending == Some(id))
    {
        d.notice = Some(notice.into());
        // Native lock release waits for this render's editing eligibility.
        d.revision = next();
        if no_send {
            d.pending = None;
        }
    }
    lease.finish(s, None);
}

#[cfg(test)]
mod tests;
pub(super) fn observe(s: &mut ShellState, visit: u64) {
    if delete::owns(s, visit) || unpublish::owns(s, visit) || permissions::owns(s, visit) {
        return;
    }
    let Some(d) = document(s, visit) else {
        return;
    };
    let (Some(id), Some(observed)) = (d.pending, d.observed.clone()) else {
        return;
    };
    let Some(a) = s
        .app_home
        .about
        .management
        .attempts
        .iter_mut()
        .find(|a| a.id == id)
    else {
        return;
    };
    let matched = a.plan.observe(&observed) == Observation::Matches;
    let completed = a.finished && a.plan.primary_succeeded() && matched;
    if completed {
        a.resolved = true;
    }
    let d = document_mut(s, visit).unwrap();
    if completed {
        d.adopt(observed);
        d.pending = None;
        d.notice = Some("Workflow details saved.".into());
    } else {
        d.notice=Some(if matched{"The current workflow matches your changes, but delivery remains unconfirmed. No update will be replayed."}
        else{"The current workflow differs. Your proposed changes and each update response are retained."}.into());
    }
}
pub(super) fn use_current(mut state: Signal<ShellState>, visit: u64) {
    let mut s = state.write();
    if permissions::owns(&s, visit)
        || permissions::confirming(&s, visit)
        || unpublish::owns(&s, visit)
        || unpublish::confirming(&s, visit)
        || delete::owns(&s, visit)
        || delete::confirming(&s, visit)
        || !admitted(&s, visit)
        || s.app_home
            .about
            .management
            .surface
            .as_ref()
            .unwrap()
            .read
            .is_some()
    {
        return;
    }
    let Some(d) = document(&s, visit) else {
        return;
    };
    let Some(observed) = d.observed.clone() else {
        return;
    };
    let pending = d.pending;
    if let Some(id) = pending {
        let Some(a) = s
            .app_home
            .about
            .management
            .attempts
            .iter_mut()
            .find(|a| a.id == id && a.finished)
        else {
            return;
        };
        a.resolved = true;
    }
    let d = document_mut(&mut s, visit).unwrap();
    d.adopt(observed);
    d.pending = None;
    d.notice = Some("Using the current workflow. Earlier update responses remain retained.".into());
}
pub(super) fn expected(s: &ShellState, id: u64) -> Option<(u64, u64, u64, String, String)> {
    if !exact(s, id) {
        return None;
    }
    let a = s
        .app_home
        .about
        .management
        .attempts
        .iter()
        .find(|a| a.id == id)?;
    Some((
        a.lease.visit,
        a.document,
        a.revision,
        a.title.clone(),
        a.description.clone(),
    ))
}
pub(super) fn outcomes(s: &ShellState, visit: u64) -> Vec<String> {
    if permissions::owns(s, visit) {
        return permissions::outcomes(s, visit);
    }
    if unpublish::owns(s, visit) {
        return unpublish::outcomes(s, visit);
    }
    if delete::owns(s, visit) {
        return delete::outcomes(s, visit);
    }
    let Some(id) = document(s, visit).and_then(|d| d.pending) else {
        return vec![];
    };
    let Some(a) = s
        .app_home
        .about
        .management
        .attempts
        .iter()
        .find(|a| a.id == id)
    else {
        return vec![];
    };
    a.plan
        .slots()
        .iter()
        .map(|slot| {
            format!(
                "{}: {}",
                if slot.plan().method() == "functions.workflows.update" {
                    "Workflow"
                } else {
                    "Trigger"
                },
                match slot.outcome() {
                    Outcome::Pending | Outcome::NotSent => "Not sent",
                    Outcome::InFlight => "Waiting for response",
                    Outcome::Succeeded(_) => "Acknowledged",
                    Outcome::Rejected(_) => "Rejected",
                    Outcome::Unknown => "Unconfirmed",
                }
            )
        })
        .collect()
}
