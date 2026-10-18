//! Gfsi/NGtd's initial workflow-token handoff into the existing modal owner.
use super::*;

pub(in crate::app_home) struct Correlation {
    action: crate::app_modal::ActionToken,
    origin: Origin,
    cancelled: bool,
}

pub(crate) fn accepts_modal(
    s: &ShellState,
    raw: &super_platinum_core::slack::events::RawEvent,
) -> bool {
    let Some(token) = raw.rest.get("client_token").and_then(Value::as_str) else {
        return true;
    };
    s.app_home
        .workflow_correlations
        .iter()
        .find(|c| c.action.token == token)
        .is_none_or(|c| {
            !c.cancelled
                && c.origin.current(s)
                && crate::app_modal::action_scope_current(s, &c.action)
        })
    // Once the modal has opened, its existing scope/view/revision owns later
    // modal submissions. This guard governs only the initial Home handoff.
}

pub(super) fn register(s: &mut ShellState, origin: &Origin, metadata: &Metadata) -> Option<String> {
    if !origin.current(s) {
        return None;
    }
    let action = crate::app_modal::register_action_with_context(s, metadata.app(), None, None)?;
    let token = action.token.clone();
    // Match the existing bounded modal-token queue. Eviction explicitly retires
    // the old token instead of dropping its stricter Home ownership guard.
    if s.app_home.workflow_correlations.len() >= 10 {
        let old = s.app_home.workflow_correlations.remove(0);
        crate::app_modal::reject_action(s, &old.action);
    }
    s.app_home.workflow_correlations.push(Correlation {
        action,
        origin: origin.clone(),
        cancelled: false,
    });
    Some(token)
}

pub(super) fn reject(s: &mut ShellState, token: &str) {
    if let Some(i) = s
        .app_home
        .workflow_correlations
        .iter()
        .position(|c| c.action.token == token)
    {
        let mut c = s.app_home.workflow_correlations.remove(i);
        crate::app_modal::reject_action(s, &c.action);
        c.cancelled = true;
        s.app_home.workflow_correlations.insert(i, c);
    }
}

pub(super) fn cancel(s: &mut ShellState, origin: Option<&Origin>) {
    let tokens: Vec<String> = s
        .app_home
        .workflow_correlations
        .iter()
        .filter(|c| {
            origin.is_none_or(|o| {
                c.origin.owner.home_visit == o.owner.home_visit
                    && c.origin.workflow.same_action(&o.workflow)
            })
        })
        .map(|c| c.action.token.clone())
        .collect();
    for token in tokens {
        reject(s, &token);
    }
}

pub(super) async fn metadata(
    state: Signal<ShellState>,
    origin: &Origin,
) -> Result<Metadata, Error> {
    let request = {
        let s = state.peek();
        if !origin.current(&s) || cooling(&s, &origin.owner.ctx) {
            return Err(Error::Api("home_workflow_retired".into()));
        }
        origin.workflow.metadata_request(
            &origin.owner.ctx.client,
            &origin.owner.ctx.session,
            s.app_home.surface.as_ref().unwrap().view.as_ref().unwrap(),
        )?
    };
    let raw = io::read(state, origin, request).await?;
    if !origin.current(&state.peek()) {
        return Err(Error::Api("home_workflow_retired".into()));
    }
    origin.workflow.metadata(raw)
}
