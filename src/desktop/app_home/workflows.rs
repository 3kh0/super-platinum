//! Home workflow trigger ownership. Confirmation does not grant replay authority.
use super::*;
use actions::Key;
use serde_json::Value;
use super_platinum_core::block_actions::{
    BlockElementMetadata, BlockElementSource,
    home::workflow::{Metadata, Preflight, TripResult, Workflow},
};

mod canvas_picker;
mod correlation;
mod entity_agents;
mod entity_channels;
mod entity_external;
mod entity_local_conversations;
mod entity_members;
pub(super) mod entity_names;
mod entity_normalization;
mod entity_picker;
mod entity_posting;
mod entity_search;
#[cfg(feature = "protected-test-support")]
pub(crate) mod fixture;
mod io;
pub(super) mod meeting_names;
mod native_lease;
pub(crate) mod navigation_history;
pub(super) mod ranking_cache;
mod ranking_renderer;
mod salesforce;
pub(super) mod salesforce_dispatch;
mod salesforce_observation;
mod salesforce_panel;
pub(crate) mod view;
pub(super) use correlation::Correlation;
pub(crate) use correlation::accepts_modal;
#[derive(Clone)]
struct Origin {
    owner: Owner,
    key: Key,
    workflow: Workflow,
    input_revision: Option<u64>,
}
impl Origin {
    fn current(&self, s: &ShellState) -> bool {
        self.owner.current(s)
            && s.core
                .session
                .as_ref()
                .and_then(|session| session.workspaces.get(&self.owner.ctx.session.team_id))
                .is_some_and(|current| {
                    current.url == self.owner.ctx.session.url
                        && current.enterprise_id == self.owner.ctx.session.enterprise_id
                })
            && inputs::ready(s)
            && inputs::revision(s) == self.input_revision
            && s.overlay.is_none()
            && s.app_home.surface.as_ref().is_some_and(|h| {
                h.id == self.key.visit
                    && h.revision == self.key.revision
                    && !h.loading
                    && h.confirmation.is_none()
                    && h.calendar_confirmation.is_none()
                    && h.view.as_ref().is_some_and(|v| self.workflow.same_view(v))
            })
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Queued,
    Sent,
    Uncertain,
}
pub(super) struct Attempt {
    id: u64,
    origin: Origin,
    phase: Phase,
    cancelled: bool,
    lease: Option<tokio::sync::oneshot::Sender<()>>,
    token: Option<String>,
}
pub(super) struct Dialog {
    id: u64,
    dismissed: bool,
    origin: Origin,
    pub(super) review: Preflight,
    pub(super) metadata: Option<Metadata>,
    pub(super) busy: bool,
    pub(super) notice: Option<String>,
    page: view::Page,
    entity_query: u64,
    entity_validation: Option<u64>,
    entity: Option<entity_picker::Picker>,
    entity_selected: std::collections::BTreeMap<(bool, String), entity_picker::Selection>,
    salesforce_query: u64,
    salesforce_observation: Option<salesforce_observation::Observation>,
    salesforce_view: Option<(String, String)>,
    canvas: Option<canvas_picker::Picker>,
    canvas_selected: std::collections::BTreeMap<
        String,
        super_platinum_core::block_actions::home::workflow::CanvasChoice,
    >,
}
#[derive(Clone)]
enum Operation {
    Precheck,
    Start(Preflight),
    Connect {
        review: Preflight,
        metadata: Metadata,
        function: String,
        provider: String,
        observation: Option<salesforce_observation::Observation>,
    },
    Select {
        review: Preflight,
        metadata: Metadata,
        function: String,
        provider: String,
        token: String,
    },
}
fn capture(s: &ShellState, key: Key, m: &BlockElementMetadata) -> Option<Origin> {
    if !matches!(&m.source,BlockElementSource::UnsupportedControl{kind} if kind=="workflow_button")
    {
        return None;
    }
    let h = s.app_home.surface.as_ref()?;
    if locked(s) || cooling(s, &h.owner.ctx) {
        return None;
    }
    let workflow = Workflow::capture(
        h.view.as_ref()?,
        m.block_id.as_deref()?,
        m.action_id.as_deref()?,
        &h.owner.ctx.session.user_id,
    )
    .ok()?;
    let origin = Origin {
        owner: h.owner.clone(),
        key,
        workflow,
        input_revision: inputs::revision(s),
    };
    if !origin.current(s)
        || s.app_home.workflow_attempts.iter().any(|a| {
            a.origin.owner.ctx.epoch == origin.owner.ctx.epoch
                && a.origin.owner.ctx.session.team_id == origin.owner.ctx.session.team_id
                && a.origin.workflow.same_action(&origin.workflow)
        })
    {
        return None;
    }
    (s.app_home
        .workflow_attempts
        .iter()
        .filter(|a| a.origin.owner.ctx.epoch == origin.owner.ctx.epoch)
        .count()
        < 64)
        .then_some(origin)
}
pub(crate) fn enabled(s: &ShellState, key: Key, m: &BlockElementMetadata) -> bool {
    capture(s, key, m).is_some()
}
pub(crate) fn click(mut state: Signal<ShellState>, key: Key, m: &BlockElementMetadata) -> bool {
    if !matches!(&m.source,BlockElementSource::UnsupportedControl{kind} if kind=="workflow_button")
    {
        return false;
    }
    let origin = { capture(&state.peek(), key, m) };
    if let Some(origin) = origin {
        if resume_connection_review(&mut state.write(), &origin) {
            return true;
        }
        if salesforce_dispatch::unresolved(&state.peek(), &origin) {
            return true;
        }
        launch(state, origin, Operation::Precheck, None)
    }
    true
}
pub(crate) fn locked(s: &ShellState) -> bool {
    s.app_home
        .workflow
        .as_ref()
        .is_some_and(|d| !d.dismissed && d.origin.current(s))
        || s.app_home
            .workflow_attempts
            .iter()
            .any(|a| !a.cancelled && a.phase != Phase::Uncertain && a.origin.current(s))
}
pub(crate) fn cancel(mut state: Signal<ShellState>, id: u64) {
    dismiss(&mut state.write(), id);
}
fn dismiss(s: &mut ShellState, id: u64) {
    if s.app_home.workflow.as_ref().is_some_and(|d| d.id == id) {
        salesforce_dispatch::cancel(s, Some(id));
        let origin = s.app_home.workflow.as_ref().unwrap().origin.clone();
        correlation::cancel(s, Some(&origin));
        for a in &mut s.app_home.workflow_attempts {
            if a.origin.workflow.same_action(&origin.workflow)
                && a.origin.owner.home_visit == origin.owner.home_visit
            {
                a.cancelled = true;
                if let Some(lease) = a.lease.take() {
                    let _ = lease.send(());
                }
            }
        }
        if salesforce_dispatch::unresolved(s, &origin) {
            let d = s.app_home.workflow.as_mut().unwrap();
            d.dismissed = true;
            d.salesforce_view = None;
            d.busy = false;
        } else {
            s.app_home.workflow = None;
        }
    }
    // A sent/uncertain attempt remains, even after the review panel is dismissed.
}
pub(crate) fn leave(s: &mut ShellState) {
    salesforce_dispatch::cancel(s, None);
    correlation::cancel(s, None);
    for a in &mut s.app_home.workflow_attempts {
        a.cancelled = true;
        if let Some(lease) = a.lease.take() {
            let _ = lease.send(());
        }
    }
    s.app_home.workflow = None;
}
fn dialog_current(s: &ShellState, id: u64) -> bool {
    s.app_home
        .workflow
        .as_ref()
        .is_some_and(|d| d.id == id && !d.dismissed && d.origin.current(s))
}
/// Reopening the same still-current Home action restores the retained review,
/// never reissues trip/authentication. A settled unknown connection requires an
/// explicit fresh list read and disposition before another write or Start.
fn resume_connection_review(s: &mut ShellState, origin: &Origin) -> bool {
    let resumable = s.app_home.workflow.as_ref().is_some_and(|d| {
        d.dismissed
            && d.origin.current(s)
            && d.origin.workflow.same_action(&origin.workflow)
            && d.origin.owner.home_visit == origin.owner.home_visit
            && d.origin.input_revision == origin.input_revision
    });
    if !resumable {
        return false;
    }
    let d = s.app_home.workflow.as_mut().unwrap();
    d.dismissed = false;
    d.page = view::Page::Connections;
    d.notice = Some("The earlier connection request was retained. Open its settings to check the current connection; it will not be sent again.".into());
    true
}
fn busy(s: &ShellState, id: u64) -> bool {
    s.app_home.workflow.as_ref().is_none_or(|d| {
        d.id != id || d.busy || d.entity_validation.is_some() || d.salesforce_view.is_some()
    }) || salesforce_dispatch::active(s, id)
}
pub(crate) fn set(mut state: Signal<ShellState>, id: u64, name: &str, value: String, custom: bool) {
    let mut s = state.write();
    if !dialog_current(&s, id) || busy(&s, id) {
        return;
    }
    let d = s.app_home.workflow.as_mut().unwrap();
    if d.busy {
        return;
    }
    // Entity inputs are selected from retained, owned search results. A text
    // callback never promotes an arbitrary ID to a workflow context choice.
    if !custom
        || !d.review.custom().iter().any(|f| {
            f.name == name
                && f.kind == super_platinum_core::block_actions::home::workflow::InputType::Text
        })
    {
        return;
    }
    let result = if custom {
        d.review.set_custom(name, value)
    } else {
        d.review.set_context(name, value)
    };
    if result.is_err() {
        d.notice = Some("This choice is not available for this workflow.".into());
    } else {
        d.entity_query = next();
        d.entity = None;
    }
}
pub(crate) fn review_connections(mut state: Signal<ShellState>, id: u64) {
    let mut s = state.write();
    if !dialog_current(&s, id) || busy(&s, id) {
        return;
    }
    let d = s.app_home.workflow.as_mut().unwrap();
    if !d.busy {
        let _ = d.review.review_connections();
    }
}
pub(crate) fn start(state: Signal<ShellState>, id: u64) {
    entity_picker::start(state, id);
}
fn start_checked(state: Signal<ShellState>, id: u64) {
    let action = {
        let s = state.peek();
        s.app_home
            .workflow
            .as_ref()
            .filter(|d| {
                d.id == id
                    && d.origin.current(&s)
                    && !busy(&s, id)
                    && !salesforce_dispatch::unresolved(&s, &d.origin)
                    && d.review.ready()
                    && entity_picker::ready(&s, id, &d.review)
            })
            .map(|d| (d.origin.clone(), Operation::Start(d.review.clone())))
    };
    if let Some((origin, op)) = action {
        launch(state, origin, op, Some(id));
    }
}
pub(crate) fn connect(
    state: Signal<ShellState>,
    id: u64,
    function: String,
    provider: String,
    token: Option<String>,
) {
    if token.is_none() {
        let needs = {
            let s = state.peek();
            s.app_home
                .workflow
                .as_ref()
                .filter(|d| d.id == id && d.origin.current(&s) && !busy(&s, id))
                .is_some_and(|d| {
                    d.review.connections().iter().any(|a| {
                        a.function == function
                            && a.provider == provider
                            && salesforce_observation::possible_app(&a.app)
                    })
                })
        };
        if needs {
            salesforce_panel::open(state, id, function, provider);
            return;
        }
    }
    connect_generic(state, id, function, provider, token);
}
fn connect_generic(
    state: Signal<ShellState>,
    id: u64,
    function: String,
    provider: String,
    token: Option<String>,
) {
    let action = {
        let s = state.peek();
        s.app_home
            .workflow
            .as_ref()
            .filter(|d| d.id == id && d.origin.current(&s) && !busy(&s, id))
            .and_then(|d| {
                d.metadata.clone().map(|metadata| {
                    let op = if let Some(token) = token {
                        Operation::Select {
                            review: d.review.clone(),
                            metadata,
                            function,
                            provider,
                            token,
                        }
                    } else {
                        Operation::Connect {
                            review: d.review.clone(),
                            metadata,
                            function,
                            provider,
                            observation: d.salesforce_observation.clone(),
                        }
                    };
                    (d.origin.clone(), op)
                })
            })
    };
    if let Some((origin, op)) = action {
        launch(state, origin, op, Some(id));
    }
}
fn launch(
    mut state: Signal<ShellState>,
    origin: Origin,
    operation: Operation,
    dialog: Option<u64>,
) {
    let id = next();
    {
        let mut s = state.write();
        if !origin.current(&s)
            || cooling(&s, &origin.owner.ctx)
            || dialog.is_some_and(|d| !dialog_current(&s, d))
        {
            return;
        }
        s.app_home
            .workflow_attempts
            .retain(|a| a.origin.owner.ctx.epoch == origin.owner.ctx.epoch);
        if s.app_home.workflow_attempts.len() >= 64
            || s.app_home.workflow_attempts.iter().any(|a| {
                a.origin.owner.ctx.session.team_id == origin.owner.ctx.session.team_id
                    && a.origin.workflow.same_action(&origin.workflow)
            })
        {
            return;
        }
        if let Some(d) = s.app_home.workflow.as_mut() {
            d.busy = true;
            d.notice = None;
        }
        s.app_home.workflow_attempts.push(Attempt {
            id,
            origin: origin.clone(),
            phase: Phase::Queued,
            cancelled: false,
            lease: None,
            token: None,
        });
    }
    dioxus::core::spawn_forever(async move {
        // The workflow's actual app owns any view_opened response. Resolve it
        // before registering the existing modal token; the Home app may differ.
        let metadata = if matches!(&operation, Operation::Precheck | Operation::Start(_)) {
            match correlation::metadata(state, &origin).await {
                Ok(metadata) => Some(metadata),
                Err(error) => {
                    let mut s = state.write();
                    failure(&mut s, &origin.owner.ctx, &error);
                    unsent(&mut s, id, dialog);
                    return;
                }
            }
        } else {
            None
        };
        let Some(lease) = inputs::native::lock(state, origin.key).await else {
            unsent(&mut state.write(), id, dialog);
            return;
        };
        let (request, release_complete) = {
            let mut s = state.write();
            if !origin.current(&s)
                || cooling(&s, &origin.owner.ctx)
                || dialog.is_some_and(|d| !dialog_current(&s, d))
                || matches!(&operation, Operation::Start(p) if dialog.is_none_or(|id|!entity_picker::ready(&s,id,p)))
            {
                let _ = lease.send(());
                unsent(&mut s, id, dialog);
                return;
            }
            let token = if let Some(metadata) = &metadata {
                let Some(token) = correlation::register(&mut s, &origin, metadata) else {
                    let _ = lease.send(());
                    unsent(&mut s, id, dialog);
                    return;
                };
                Some(token)
            } else {
                None
            };
            let current = s.app_home.surface.as_ref().unwrap().view.as_ref().unwrap();
            let ctx = &origin.owner.ctx;
            let request = match &operation {
                Operation::Precheck => origin.workflow.precheck(
                    &ctx.client,
                    &ctx.session,
                    current,
                    token.as_deref().unwrap(),
                ),
                Operation::Start(p) => p.prepare_start(
                    &ctx.client,
                    &ctx.session,
                    current,
                    token.as_deref().unwrap(),
                ),
                Operation::Connect {
                    review,
                    metadata,
                    function,
                    provider,
                    observation,
                } => review.connect_request_with_context(
                    &ctx.client,
                    &ctx.session,
                    current,
                    metadata,
                    function,
                    provider,
                    observation
                        .as_ref()
                        .filter(|o| o.current(&s))
                        .map(|o| o.native().context()),
                ),
                Operation::Select {
                    review,
                    metadata,
                    function,
                    provider,
                    token,
                } => review.select_connection_request(
                    &ctx.client,
                    &ctx.session,
                    current,
                    metadata,
                    function,
                    provider,
                    token,
                ),
            };
            let Ok(request) = request else {
                if let Some(token) = &token {
                    correlation::reject(&mut s, token);
                }
                let _ = lease.send(());
                unsent(&mut s, id, dialog);
                return;
            };
            let Some(a) = s
                .app_home
                .workflow_attempts
                .iter_mut()
                .find(|a| a.id == id && a.phase == Phase::Queued && !a.cancelled)
            else {
                if let Some(token) = &token {
                    correlation::reject(&mut s, token);
                }
                let _ = lease.send(());
                return;
            };
            a.phase = Phase::Sent;
            let (release, complete) = native_lease::retain(lease);
            a.lease = Some(release);
            a.token = token;
            (request, complete)
        };
        let result = io::once(state, &origin, request).await;
        let authorization = if matches!(&operation, Operation::Connect { .. }) {
            result
                .as_ref()
                .ok()
                .and_then(|raw| Preflight::authorization_url(raw).ok())
        } else {
            None
        };
        let (lease, cancelled) = {
            let mut s = state.write();
            let Some(a) = s.app_home.workflow_attempts.iter_mut().find(|a| a.id == id) else {
                return;
            };
            (a.lease.take(), a.cancelled)
        };
        let metadata_needed = settle(&mut state.write(), id, &origin, &operation, dialog, result);
        if let Some(url) = authorization {
            let current = {
                let s = state.peek();
                !cancelled
                    && lease.is_some()
                    && origin.current(&s)
                    && dialog.is_some_and(|id| dialog_current(&s, id))
            };
            if current {
                crate::slack_links::open(state, &url);
            }
        }
        if let Some(lease) = lease {
            let _ = lease.send(());
        }
        if !release_complete
            .current_after(|| {
                let s = state.peek();
                origin.current(&s) && dialog.is_none_or(|id| dialog_current(&s, id))
            })
            .await
        {
            return;
        }
        if let Some(dialog) = metadata_needed {
            if let Some(metadata) = metadata {
                let mut s = state.write();
                if dialog_current(&s, dialog) {
                    s.app_home.workflow.as_mut().unwrap().metadata = Some(metadata);
                }
            }
            salesforce_observation::prime(state, dialog);
        }
    });
}
fn unsent(s: &mut ShellState, id: u64, dialog: Option<u64>) {
    if let Some(token) = s
        .app_home
        .workflow_attempts
        .iter()
        .find(|a| a.id == id)
        .and_then(|a| a.token.clone())
    {
        correlation::reject(s, &token);
    }
    s.app_home.workflow_attempts.retain(|a| a.id != id);
    if let Some(d) = s
        .app_home
        .workflow
        .as_mut()
        .filter(|d| Some(d.id) == dialog)
    {
        d.busy = false;
        d.notice =
            Some("The action was not sent. Review the current Home before trying again.".into());
    }
}
fn settle(
    s: &mut ShellState,
    id: u64,
    origin: &Origin,
    op: &Operation,
    dialog: Option<u64>,
    result: Result<Value, Error>,
) -> Option<u64> {
    let cancelled = s
        .app_home
        .workflow_attempts
        .iter()
        .find(|a| a.id == id)
        .is_none_or(|a| a.cancelled);
    if let Err(error) = &result {
        failure(s, &origin.owner.ctx, error);
    }
    let definitely_unsent = matches!(&result, Err(Error::RateLimited { .. }))
        || matches!(&result,Err(Error::Api(e)) if e=="block_interaction_not_sent_offline");
    let classified = match (op, &result) {
        (Operation::Precheck | Operation::Start(_), Ok(raw)) => {
            origin.workflow.response(raw.clone())
        }
        (_, Ok(raw)) if raw["ok"] == false => Ok(TripResult::Rejected),
        (Operation::Connect { .. }, Ok(raw)) => {
            Preflight::authorization_url(raw).map(|_| TripResult::Accepted {
                slow: false,
                contains_ai: false,
            })
        }
        (_, Ok(raw)) if raw["ok"] == true => Ok(TripResult::Accepted {
            slow: false,
            contains_ai: false,
        }),
        _ => Err(Error::Api("workflow_result_unconfirmed".into())),
    };
    // A precheck review/rejection cannot later authorize a modal. Accepted or
    // uncertain dispatch keeps its exact token until consumption/expiry.
    if definitely_unsent
        || matches!(
            &classified,
            Ok(TripResult::Review(_) | TripResult::Rejected)
        )
    {
        if let Some(token) = s
            .app_home
            .workflow_attempts
            .iter()
            .find(|a| a.id == id)
            .and_then(|a| a.token.clone())
        {
            correlation::reject(s, &token);
        }
    }
    if classified.is_ok() || definitely_unsent {
        s.app_home.workflow_attempts.retain(|a| a.id != id);
    } else if let Some(a) = s.app_home.workflow_attempts.iter_mut().find(|a| a.id == id) {
        a.phase = Phase::Uncertain;
    }
    if cancelled || !origin.current(s) || dialog.is_some_and(|d| !dialog_current(s, d)) {
        return None;
    }
    if let Some(d) = s
        .app_home
        .workflow
        .as_mut()
        .filter(|d| Some(d.id) == dialog)
    {
        d.busy = false;
    }
    match classified {
        Ok(TripResult::Review(review)) => {
            let id = next();
            s.app_home.workflow = Some(Dialog {
                id,
                dismissed: false,
                origin: origin.clone(),
                review,
                metadata: None,
                busy: false,
                notice: None,
                page: view::Page::Summary,
                entity_query: 0,
                entity_validation: None,
                entity: None,
                entity_selected: Default::default(),
                salesforce_query: 0,
                salesforce_observation: None,
                salesforce_view: None,
                canvas: None,
                canvas_selected: Default::default(),
            });
            Some(id)
        }
        Ok(TripResult::Accepted { slow, .. }) => {
            match op {
                Operation::Connect { .. } => {
                    if let Some(d) = s.app_home.workflow.as_mut() {
                        d.notice = Some(
                            "Continue connecting your account in the browser, then return here."
                                .into(),
                        );
                    }
                }
                Operation::Select {
                    function,
                    provider,
                    token,
                    ..
                } => {
                    if let (Some(d), Ok(raw)) = (s.app_home.workflow.as_mut(), &result) {
                        let _ = d.review.selected_connection(function, provider, token, raw);
                    }
                }
                _ => {
                    s.app_home.workflow = None;
                    s.app_home.surface.as_mut().unwrap().notice = Some(
                        if slow {
                            "Workflow started. Its steps are still running."
                        } else {
                            "Workflow started."
                        }
                        .into(),
                    );
                }
            }
            None
        }
        Ok(TripResult::Rejected) => {
            if let Some(d) = s.app_home.workflow.as_mut() {
                d.notice = Some("Slack declined this workflow action.".into());
            } else {
                s.app_home.surface.as_mut().unwrap().notice =
                    Some("Slack declined this workflow action.".into());
            }
            None
        }
        Err(_) => {
            let text = if definitely_unsent {
                "The action was not sent. Wait for connectivity or the rate limit before trying again."
            } else {
                "Slack has not confirmed this action. It will not be sent again."
            };
            if let Some(d) = s.app_home.workflow.as_mut() {
                d.notice = Some(text.into());
            } else {
                s.app_home.surface.as_mut().unwrap().notice = Some(text.into());
            }
            None
        }
    }
}
pub(crate) fn realtime(s: &mut ShellState, team: &str, event: &Value) {
    salesforce_dispatch::realtime(s, team, event);
    let Some(d) = s
        .app_home
        .workflow
        .as_ref()
        .filter(|d| d.origin.owner.ctx.session.team_id == team && d.origin.current(s))
    else {
        return;
    };
    let origin = d.origin.clone();
    let current = s.app_home.surface.as_ref().unwrap().view.clone().unwrap();
    if let Some(d) = s.app_home.workflow.as_mut() {
        let _ = d
            .review
            .external_auth_completed(event, &current, &origin.owner.ctx.session);
    }
}
