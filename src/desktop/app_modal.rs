//! App-view lifecycle. All mutation responses are scoped and never replayed.
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{
    scheduled::Context,
    state::{Overlay, ShellState},
};
use dioxus::prelude::*;
use serde_json::Value;
use super_platinum_core::{
    block_actions::{
        form::{FormError, InputKey, InputValue, ModalForm},
        modal::{ModalChange, ModalScope, ModalState, ViewedConversation},
    },
    slack::{api, events::RtEvent},
};

pub(crate) mod channel_fixture;
mod channels;
pub(crate) mod check;
pub(crate) mod conversation_fixture;
pub(crate) mod external;
pub(crate) mod fixture;
pub(crate) mod multi_channel_fixture;
pub(crate) mod multi_conversation_fixture;
pub(crate) mod multi_external_check;
pub(crate) mod multi_external_fixture;
mod origin;
mod requests;
#[cfg(test)]
mod tests;
pub(crate) mod users;
pub(crate) mod users_check;
pub(crate) mod users_fixture;

pub(crate) struct Session {
    pub(crate) modal: ModalState,
    forms: BTreeMap<String, ModalForm>,
    generation: u64,
    reset_revision: u64,
    busy: bool,
    uncertain: bool,
    loading: bool,
    conflict: bool,
    view_error: Option<String>,
    fixture: bool,
    pub(crate) fixture_attempts: usize,
    external_picker: Option<external::Picker>,
    external_proofs: BTreeMap<(String, InputKey), external::Catalog>,
    users: Option<users::Picker>,
    user_labels: BTreeMap<String, String>,
    channel_proofs: BTreeMap<
        (String, super_platinum_core::block_actions::form::InputKey),
        crate::message_buttons::inputs::channels::Proofs,
    >,
    // Origin evidence outlives selected chips; per-field proofs are pruned on edits.
    viewed_proofs: crate::message_buttons::inputs::channels::Proofs,
    users_fixture: Option<users_fixture::Recorder>,
}
impl Session {
    fn new(scope: ModalScope) -> Self {
        Self {
            modal: ModalState::new(scope),
            forms: BTreeMap::new(),
            generation: next_id(),
            reset_revision: next_id(),
            busy: false,
            uncertain: false,
            loading: false,
            conflict: false,
            view_error: None,
            fixture: false,
            fixture_attempts: 0,
            external_picker: None,
            external_proofs: BTreeMap::new(),
            users: None,
            user_labels: BTreeMap::new(),
            channel_proofs: BTreeMap::new(),
            viewed_proofs: BTreeMap::new(),
            users_fixture: None,
        }
    }
    pub(crate) fn fixture(view: Value) -> Self {
        Self::fixture_with_origin(view, None)
    }
    pub(crate) fn fixture_with_origin(view: Value, origin: Option<String>) -> Self {
        Self::fixture_with_context(view, origin, None)
    }
    fn fixture_with_context(
        view: Value,
        origin: Option<String>,
        viewed: Option<ViewedConversation>,
    ) -> Self {
        let scope = ModalScope {
            account_epoch: 0,
            team: "T1".into(),
            realtime_generation: 0,
        };
        let mut session = Self::new(scope.clone());
        session.fixture = true;
        session.modal.enqueue_with_context(
            &scope,
            "fixture".into(),
            "A_FIXTURE".into(),
            origin,
            viewed,
            0,
            60_000,
        );
        let event=serde_json::from_value(serde_json::json!({"type":"view_opened","view_id":view["id"],"app_id":"A_FIXTURE","view_type":"modal","client_token":"fixture","view":view})).expect("fixture event");
        session.modal.apply(&scope, &event, 1);
        session.sync_forms();
        session
    }
    pub(crate) fn current_form(&self) -> Option<&ModalForm> {
        self.forms.get(&self.modal.stack().last()?.id)
    }
    pub(crate) fn current_form_mut(&mut self) -> Option<&mut ModalForm> {
        let id = self.modal.stack().last()?.id.clone();
        self.forms.get_mut(&id)
    }
    fn locked(&self) -> bool {
        self.busy || self.uncertain || self.loading || self.conflict
    }
    fn sync_forms(&mut self) {
        self.forms
            .retain(|id, _| self.modal.stack().iter().any(|v| &v.id == id));
        self.external_proofs
            .retain(|(view, _), _| self.forms.contains_key(view));
        self.channel_proofs
            .retain(|(view, _), _| self.forms.contains_key(view));
        self.loading = self.modal.stack().last().is_some_and(|v| v.needs_content);
        // A hidden parent's remote changes must not block or invalidate the
        // active child. Retain its draft and reconcile the latest raw payload
        // when Back or a successful submission actually reveals that parent.
        let Some(view) = self.modal.stack().last() else {
            return;
        };
        if view.needs_content {
            return;
        }
        let Some(payload) = view.payload.as_ref() else {
            return;
        };
        let result = if let Some(form) = self.forms.get_mut(&view.id) {
            if form.raw_view == *payload {
                return;
            }
            form.reconcile(payload)
        } else {
            ModalForm::parse_with_context(
                payload,
                view.origin_conversation.as_deref(),
                view.viewed_conversation.as_ref(),
            )
            .map(|form| {
                self.forms.insert(view.id.clone(), form);
            })
        };
        if let Err(error) = result {
            self.conflict = true;
            self.view_error = Some(error.to_string());
        }
    }
}

pub(crate) struct Presentation {
    pub form: Option<ModalForm>,
    pub title: String,
    pub submit_label: Option<String>,
    pub close_label: String,
    pub can_back: bool,
    pub busy: bool,
    pub uncertain: bool,
    pub loading: bool,
    pub locked: bool,
    pub view_error: Option<String>,
    pub reset_revision: u64,
}
pub(crate) fn presentation(shell: &ShellState) -> Option<Presentation> {
    let session = shell.app_modal.as_ref()?;
    if !current_scope(shell, session) || session.modal.stack().is_empty() {
        return None;
    }
    let form = session.current_form().cloned();
    let unavailable = users::unavailable_context(shell);
    Some(Presentation {
        title: form
            .as_ref()
            .map_or("App form", |f| f.title.as_str())
            .into(),
        submit_label: form
            .as_ref()
            .and_then(|f| f.submit_label.clone())
            .filter(|_| !session.conflict),
        close_label: form
            .as_ref()
            .and_then(|f| f.close_label.clone())
            .unwrap_or_else(|| "Close".into()),
        can_back: session.modal.stack().len() > 1,
        form,
        busy: session.busy,
        uncertain: session.uncertain,
        loading: session.loading,
        locked: session.locked() || unavailable.is_some(),
        view_error: unavailable
            .map(str::to_owned)
            .or_else(|| session.view_error.clone()),
        reset_revision: session.reset_revision,
    })
}
fn scope(shell: &ShellState) -> Option<ModalScope> {
    let team = shell.core.active_team.as_ref()?;
    Some(ModalScope {
        account_epoch: shell.core.account_epoch,
        team: team.clone(),
        realtime_generation: shell.core.workspaces.get(team)?.rt_generation,
    })
}
fn current_scope(shell: &ShellState, session: &Session) -> bool {
    session.fixture && crate::fixture::is_fixture()
        || scope(shell).as_ref() == Some(session.modal.scope())
}
fn next_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}
fn token() -> String {
    static LAST: AtomicU64 = AtomicU64::new(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let result = LAST
        .try_update(Ordering::SeqCst, Ordering::SeqCst, |old| {
            Some(now.max(old.saturating_add(1)))
        })
        .unwrap();
    format!("web-{}", now.max(result.saturating_add(1)))
}
fn now_ms() -> u64 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_millis() as u64
}

pub(crate) fn set_value(mut state: Signal<ShellState>, key: InputKey, value: InputValue) {
    let mut shell = state.write();
    let valid = shell
        .app_modal
        .as_ref()
        .is_some_and(|s| current_scope(&shell, s));
    if !valid {
        return;
    }
    let session = shell.app_modal.as_mut().unwrap();
    if session.locked() {
        return;
    }
    if let Some(form) = session.current_form_mut() {
        let _ = form.set_value(&key, value);
    }
}
pub(crate) fn set_field_error(mut state: Signal<ShellState>, key: InputKey, error: String) {
    let mut shell = state.write();
    let valid = shell
        .app_modal
        .as_ref()
        .is_some_and(|s| current_scope(&shell, s));
    if !valid {
        return;
    }
    let session = shell.app_modal.as_mut().unwrap();
    if session.locked() {
        return;
    }
    if let Some(field) = session
        .current_form_mut()
        .and_then(|f| f.fields.get_mut(&key))
    {
        field.error = Some(error);
        field.dirty = true;
    }
}
pub(crate) fn close(mut state: Signal<ShellState>) {
    dismiss_local(&mut state.write());
    flush_closes(state);
}
pub(crate) fn back(mut state: Signal<ShellState>) {
    if state.read().app_modal.as_ref().is_some_and(|s| !s.locked()) {
        back_local(&mut state.write());
        flush_closes(state);
        load_if_needed(state);
    }
}

pub(crate) struct CloseTicket {
    context: Context,
    scope: ModalScope,
    view_id: String,
    root: Option<String>,
    token: String,
}
pub(crate) fn dismiss_local(shell: &mut ShellState) {
    close_local(shell, true);
}
pub(crate) fn back_local(shell: &mut ShellState) {
    close_local(shell, false);
}
fn close_local(shell: &mut ShellState, all: bool) {
    let context = Context::capture(shell);
    let valid = shell
        .app_modal
        .as_ref()
        .is_some_and(|s| current_scope(shell, s));
    let Some(session) = shell.app_modal.as_mut() else {
        shell.overlay = None;
        return;
    };
    if !valid {
        shell.app_modal = None;
        shell.overlay = None;
        return;
    }
    if let (Some(view), Some(root), Some(context)) = (
        session.modal.stack().last(),
        session.modal.stack().first(),
        context,
    ) {
        if !session.fixture {
            shell.app_modal_closes.push(CloseTicket {
                context,
                scope: session.modal.scope().clone(),
                view_id: view.id.clone(),
                root: (all
                    || view
                        .payload
                        .as_ref()
                        .and_then(|v| v.get("clear_on_close"))
                        .and_then(Value::as_bool)
                        == Some(true))
                .then(|| root.id.clone()),
                token: token(),
            });
        }
    }
    if all {
        session.modal.clear_local(&session.modal.scope().clone());
    } else {
        session.modal.pop_local(&session.modal.scope().clone());
    }
    session.generation = next_id();
    session.reset_revision = next_id();
    session.busy = false;
    session.uncertain = false;
    session.conflict = false;
    session.view_error = None;
    session.sync_forms();
    shell.overlay = (!session.modal.stack().is_empty()).then_some(Overlay::AppModal);
}
pub(crate) fn flush_closes(mut state: Signal<ShellState>) {
    let tickets = std::mem::take(&mut state.write().app_modal_closes);
    for ticket in tickets {
        dioxus::core::spawn_forever(requests::close(state, ticket));
    }
}
/// Reconnection/account replacement invalidates tokens and any in-flight UI work.
pub(crate) fn poll(shell: &mut ShellState) -> bool {
    let users_changed =
        crate::message_buttons::shortcuts::poll(shell) | users::poll(shell) | external::poll(shell);
    if shell
        .app_modal
        .as_ref()
        .is_some_and(|s| !current_scope(shell, s))
    {
        shell.app_modal = None;
        if shell.overlay == Some(Overlay::AppModal) {
            shell.overlay = None;
        }
        return true;
    }
    if shell.overlay != Some(Overlay::AppModal)
        && shell
            .app_modal
            .as_ref()
            .is_some_and(|s| !s.modal.stack().is_empty())
    {
        let replacement = shell.overlay;
        dismiss_local(shell);
        shell.overlay = replacement;
        return true;
    }
    users_changed
}
pub(crate) fn realtime(
    shell: &mut ShellState,
    team: &str,
    generation: u64,
    event: &RtEvent,
) -> bool {
    let RtEvent::Unknown(raw) = event else {
        return false;
    };
    if !crate::app_home::accepts_modal(shell, raw) {
        return false;
    }
    if !matches!(raw.kind.as_str(), "view_opened" | "view_updated") {
        return false;
    }
    let Some(scope) =
        scope(shell).filter(|s| s.team == team && s.realtime_generation == generation)
    else {
        return false;
    };
    poll(shell);
    let Some(session) = shell.app_modal.as_mut() else {
        return false;
    };
    let old = session.modal.stack().last().map(|v| v.id.clone());
    let old_revision = session.modal.revision();
    if session.modal.apply(&scope, raw, now_ms()) == ModalChange::Ignored {
        return false;
    }
    if session.modal.revision() == old_revision {
        // A hidden ancestor changed. The reducer retained its latest payload
        // or hydration requirement; current-child requests remain authoritative.
        return false;
    }
    session.generation = next_id();
    // A push/update can arrive before its HTTP acknowledgement. The event is
    // authoritative; the old response cannot then mutate this new revision.
    if session.busy {
        session.uncertain = true;
    }
    session.busy = false;
    if old != session.modal.stack().last().map(|v| v.id.clone()) {
        session.reset_revision = next_id();
        session.uncertain = false;
    }
    session.sync_forms();
    shell.overlay = Some(Overlay::AppModal);
    session.loading
}
pub(crate) fn load_if_needed(state: Signal<ShellState>) {
    if state
        .read()
        .app_modal
        .as_ref()
        .is_some_and(|s| s.loading && !s.fixture)
    {
        load(state);
    }
}
pub(crate) fn load(state: Signal<ShellState>) {
    dioxus::core::spawn_forever(requests::load(state));
}
pub(crate) fn submit(state: Signal<ShellState>) {
    dioxus::core::spawn_forever(requests::submit(state));
}

pub(crate) fn apply_fixture_update(mut state: Signal<ShellState>, view: Value) {
    let mut shell = state.write();
    let Some(session) = shell
        .app_modal
        .as_mut()
        .filter(|s| s.fixture && crate::fixture::is_fixture())
    else {
        return;
    };
    let scope = session.modal.scope().clone();
    let event=serde_json::from_value(serde_json::json!({"type":"view_updated","view_id":view["id"],"app_id":"A_FIXTURE","view":view})).expect("fixture update");
    let old_revision = session.modal.revision();
    if session.modal.apply(&scope, &event, now_ms()) != ModalChange::Ignored
        && session.modal.revision() != old_revision
    {
        session.generation = next_id();
        session.sync_forms();
    }
}

/// Register before sending a validated message action. The empty session has no
/// overlay until a correctly correlated realtime view_opened arrives.
pub(crate) struct ActionToken {
    pub(crate) token: String,
    scope: ModalScope,
}
pub(crate) fn register_action(shell: &mut ShellState, app_id: &str) -> Option<ActionToken> {
    register_action_with_origin(shell, app_id, None)
}
pub(crate) fn register_action_with_origin(
    shell: &mut ShellState,
    app_id: &str,
    origin_conversation: Option<String>,
) -> Option<ActionToken> {
    register_action_with_context(shell, app_id, origin_conversation, None)
}

pub(crate) fn register_viewed_action(
    shell: &mut ShellState,
    app_id: &str,
    source_conversation: Option<String>,
) -> Option<ActionToken> {
    let viewed = origin::capture(shell);
    register_action_with_context(shell, app_id, source_conversation, viewed)
}

pub(crate) fn capture_shortcut_viewed(shell: &ShellState) -> Option<ViewedConversation> {
    origin::capture(shell)
}

pub(crate) fn register_action_with_context(
    shell: &mut ShellState,
    app_id: &str,
    origin_conversation: Option<String>,
    viewed: Option<ViewedConversation>,
) -> Option<ActionToken> {
    // Overlay replacement can precede the periodic lifecycle tick. Retire a
    // displaced stack now so it cannot silently swallow a new button click.
    poll(shell);
    let scope = scope(shell)?;
    if shell
        .app_modal
        .as_ref()
        .is_some_and(|s| s.modal.scope() != &scope)
    {
        shell.app_modal = None;
    }
    let session = shell
        .app_modal
        .get_or_insert_with(|| Session::new(scope.clone()));
    if !session.modal.stack().is_empty() {
        return None;
    }
    let token = token();
    session
        .modal
        .enqueue_with_context(
            &scope,
            token.clone(),
            app_id.into(),
            origin_conversation,
            viewed,
            now_ms(),
            60_000,
        )
        .then_some(ActionToken { token, scope })
}
pub(crate) fn action_scope_current(shell: &ShellState, action: &ActionToken) -> bool {
    scope(shell).as_ref() == Some(&action.scope)
        && shell
            .app_modal
            .as_ref()
            .is_some_and(|session| session.modal.scope() == &action.scope)
}
pub(crate) fn reject_action(shell: &mut ShellState, action: &ActionToken) {
    if let Some(session) = shell.app_modal.as_mut() {
        session.modal.reject(&action.scope, &action.token);
    }
}

pub(crate) fn home_fixture_open(
    shell: &mut ShellState,
    team: &str,
    generation: u64,
    event: &RtEvent,
) {
    if !crate::fixture::is_fixture() && !cfg!(test) {
        return;
    }
    realtime(shell, team, generation, event);
    if let Some(session) = shell.app_modal.as_mut().filter(|s| {
        s.modal
            .stack()
            .last()
            .is_some_and(|v| v.id == "V_HOME_FORM" && v.app_id == "A_HOME")
    }) {
        session.fixture = true;
    }
}
pub(crate) fn home_fixture_complete(
    shell: &mut ShellState,
    client_token: &str,
    response: api::BlockViewSubmission,
) {
    if !crate::fixture::is_fixture() && !cfg!(test) {
        return;
    }
    let Some(session) = shell.app_modal.as_ref().filter(|s| s.fixture) else {
        return;
    };
    let Some(view) = session.modal.stack().last().map(|v| v.id.clone()) else {
        return;
    };
    let revision = session.modal.revision();
    requests::complete(
        shell,
        &view,
        revision,
        client_token,
        Ok(api::BlockViewResponse::Accepted(response)),
    );
}
