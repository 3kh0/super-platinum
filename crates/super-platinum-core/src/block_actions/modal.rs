//! In-memory app-modal correlation. No network requests or renderer handles.
//!
//! The captured Slack NGtd handler correlates `view_opened` with Gfsi's ten
//! newest team tokens and checks elapsed milliseconds plus previous-view ID.
//! Callers supply monotonic milliseconds and a local token lifetime. Local
//! resource limits below are client budgets, not claims about Slack limits.

use std::collections::VecDeque;

use serde_json::Value;

use crate::slack::api::BlockViewResponseAction;
use crate::slack::events::RawEvent;

const TOKEN_LIMIT: usize = 10;
const STACK_LIMIT: usize = 10;
const OPENS_PER_TOKEN: usize = 32;

/// Renderer attestation of the conversation actually visible at activation.
/// Never construct this from an app payload or a source message's channel alone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewedConversation(String);
impl ViewedConversation {
    pub fn capture(id: &str) -> Option<Self> {
        (super::form::valid_message_conversation(id)
            && matches!(id.as_bytes().first(), Some(b'C' | b'G')))
        .then(|| Self(id.to_owned()))
    }
    pub fn id(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModalScope {
    pub account_epoch: u64,
    pub team: String,
    pub realtime_generation: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModalView {
    pub id: String,
    pub app_id: String,
    pub kind: String,
    pub previous_view_id: Option<String>,
    /// Conversation captured from the initiating action, inherited by this
    /// view's stack. Navigation after dispatch must never change this origin.
    pub origin_conversation: Option<String>,
    pub viewed_conversation: Option<ViewedConversation>,
    /// Exact server view, without lossy block parsing. An open/update frame can
    /// omit it; the caller must fetch content before presenting an input form.
    pub payload: Option<Value>,
    pub needs_content: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModalChange {
    Ignored,
    Opened,
    Updated,
}

#[derive(Debug)]
struct Token {
    value: String,
    app_id: String,
    origin_conversation: Option<String>,
    viewed_conversation: Option<ViewedConversation>,
    issued_ms: u64,
    expires_ms: u64,
    opened: Vec<String>,
}

#[derive(Debug)]
pub struct ModalState {
    scope: ModalScope,
    tokens: VecDeque<Token>,
    stack: Vec<ModalView>,
    revision: u64,
}

impl ModalState {
    pub fn new(scope: ModalScope) -> Self {
        Self {
            scope,
            tokens: VecDeque::new(),
            stack: Vec::new(),
            revision: 0,
        }
    }

    pub fn scope(&self) -> &ModalScope {
        &self.scope
    }
    pub fn stack(&self) -> &[ModalView] {
        &self.stack
    }
    /// Generation of the active view and stack identity. Hidden-parent payload
    /// updates are retained but do not invalidate work on the current child.
    /// Returning to a parent always advances this generation before hydration.
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn token_count(&self) -> usize {
        self.tokens.len()
    }

    /// Complete a views.get for the exact active view/revision that requested
    /// it. An active-view update, account switch or stack change invalidates it.
    pub fn hydrate_current(
        &mut self,
        scope: &ModalScope,
        expected_view_id: &str,
        expected_revision: u64,
        payload: Value,
    ) -> bool {
        if !self.matches_current(scope, expected_view_id, expected_revision)
            || !self.valid_current_payload(&payload)
        {
            return false;
        }
        let current = self.stack.last_mut().expect("checked active view");
        current.payload = Some(payload);
        current.needs_content = false;
        self.revision = self.revision.wrapping_add(1);
        true
    }

    /// Apply a decoded acknowledgement without inventing a realtime event.
    /// The caller must retain the modal on rejection or unknown response action.
    pub fn apply_submission(
        &mut self,
        scope: &ModalScope,
        expected_view_id: &str,
        expected_revision: u64,
        action: &BlockViewResponseAction,
    ) -> bool {
        if !self.matches_current(scope, expected_view_id, expected_revision) {
            return false;
        }
        match action {
            BlockViewResponseAction::Absent => {
                // Successful submission pops one view even when clear_on_close
                // is set: that flag governs dismissal, not submission.
                self.stack.pop();
                self.tokens.clear();
            }
            BlockViewResponseAction::Clear => {
                self.stack.clear();
                self.tokens.clear();
            }
            BlockViewResponseAction::Update(payload) => {
                if !self.valid_current_payload(payload) {
                    return false;
                }
                let current = self.stack.last_mut().expect("checked active view");
                current.payload = Some(payload.clone());
                current.needs_content = false;
            }
            BlockViewResponseAction::Push(payload) => {
                let current = self.stack.last().expect("checked active view");
                let Some(id) = payload.get("id").and_then(nonempty) else {
                    return false;
                };
                if self.stack.len() >= STACK_LIMIT
                    || self.stack.iter().any(|view| view.id == id)
                    || payload.get("app_id").and_then(nonempty) != Some(current.app_id.as_str())
                    || payload.get("previous_view_id").and_then(nonempty) != Some(expected_view_id)
                    || !payload.get("blocks").is_some_and(Value::is_array)
                    || !valid_view(
                        payload,
                        id,
                        &current.app_id,
                        &current.kind,
                        Some(expected_view_id),
                        &self.stack[0].id,
                    )
                {
                    return false;
                }
                self.stack.push(ModalView {
                    id: id.to_owned(),
                    app_id: current.app_id.clone(),
                    kind: current.kind.clone(),
                    previous_view_id: Some(expected_view_id.to_owned()),
                    origin_conversation: current.origin_conversation.clone(),
                    viewed_conversation: current.viewed_conversation.clone(),
                    payload: Some(payload.clone()),
                    needs_content: false,
                });
                // Retain valid correlation for subsequent app opens, but an
                // echo of this already-applied push must not open it again.
                for token in &mut self.tokens {
                    if token.app_id == self.stack.last().expect("pushed view").app_id
                        && !token.opened.iter().any(|seen| seen == id)
                        && token.opened.len() < OPENS_PER_TOKEN
                    {
                        token.opened.push(id.to_owned());
                    }
                }
            }
        }
        self.revision = self.revision.wrapping_add(1);
        true
    }

    fn matches_current(&self, scope: &ModalScope, id: &str, revision: u64) -> bool {
        scope == &self.scope
            && revision == self.revision
            && self.stack.last().is_some_and(|view| view.id == id)
    }

    fn valid_current_payload(&self, payload: &Value) -> bool {
        self.stack.last().is_some_and(|view| {
            payload.get("blocks").is_some_and(Value::is_array)
                && valid_view(
                    payload,
                    &view.id,
                    &view.app_id,
                    &view.kind,
                    view.previous_view_id.as_deref(),
                    &self.stack[0].id,
                )
        })
    }

    /// Explicit session/reconnect replacement; events never change the scope.
    pub fn reset_scope(&mut self, scope: ModalScope) {
        if self.scope != scope {
            self.scope = scope;
            self.tokens.clear();
            self.stack.clear();
            self.revision = self.revision.wrapping_add(1);
        }
    }

    /// Record a locally initiated action before dispatch. The token format is
    /// owned by the caller; this reducer never manufactures a wire token.
    pub fn enqueue(
        &mut self,
        scope: &ModalScope,
        token: String,
        app_id: String,
        now_ms: u64,
        lifetime_ms: u64,
    ) -> bool {
        self.enqueue_with_origin(scope, token, app_id, None, now_ms, lifetime_ms)
    }

    /// Capture the initiating action's conversation before dispatch. This is
    /// view lineage, not account/realtime scope or the current UI selection.
    pub fn enqueue_with_origin(
        &mut self,
        scope: &ModalScope,
        token: String,
        app_id: String,
        origin_conversation: Option<String>,
        now_ms: u64,
        lifetime_ms: u64,
    ) -> bool {
        self.enqueue_with_context(
            scope,
            token,
            app_id,
            origin_conversation,
            None,
            now_ms,
            lifetime_ms,
        )
    }

    pub fn enqueue_with_context(
        &mut self,
        scope: &ModalScope,
        token: String,
        app_id: String,
        origin_conversation: Option<String>,
        viewed_conversation: Option<ViewedConversation>,
        now_ms: u64,
        lifetime_ms: u64,
    ) -> bool {
        if scope != &self.scope
            || token.trim().is_empty()
            || app_id.trim().is_empty()
            || lifetime_ms == 0
            || origin_conversation
                .as_deref()
                .is_some_and(|id| !super::form::valid_message_conversation(id))
            || self.tokens.iter().any(|old| old.value == token)
        {
            return false;
        }
        let Some(expires_ms) = now_ms.checked_add(lifetime_ms) else {
            return false;
        };
        self.tokens.retain(|old| now_ms < old.expires_ms);
        self.tokens.push_front(Token {
            value: token,
            app_id,
            origin_conversation,
            viewed_conversation,
            issued_ms: now_ms,
            expires_ms,
            opened: Vec::new(),
        });
        self.tokens.truncate(TOKEN_LIMIT);
        true
    }

    /// A definitive dispatch rejection may retire its correlation. A transport
    /// timeout is not proof of rejection and must not be passed here as one.
    pub fn reject(&mut self, scope: &ModalScope, token: &str) {
        if scope == &self.scope {
            self.tokens.retain(|entry| entry.value != token);
        }
    }

    pub fn apply(&mut self, scope: &ModalScope, event: &RawEvent, now_ms: u64) -> ModalChange {
        if scope != &self.scope {
            return ModalChange::Ignored;
        }
        match event.kind.as_str() {
            "view_opened" => self.open(event, now_ms),
            "view_updated" => self.update(event),
            _ => ModalChange::Ignored,
        }
    }

    fn open(&mut self, event: &RawEvent, now_ms: u64) -> ModalChange {
        let Some(id) = field(event, "view_id") else {
            return ModalChange::Ignored;
        };
        let Some(app_id) = field(event, "app_id") else {
            return ModalChange::Ignored;
        };
        let Some(kind) = field(event, "view_type").filter(|kind| modal_kind(kind)) else {
            return ModalChange::Ignored;
        };
        let Some(token) = field(event, "client_token") else {
            return ModalChange::Ignored;
        };
        let Some(index) = self.tokens.iter().position(|entry| entry.value == token) else {
            return ModalChange::Ignored;
        };
        let entry = &self.tokens[index];
        let Some(elapsed) = now_ms.checked_sub(entry.issued_ms) else {
            return ModalChange::Ignored;
        };
        if entry.app_id != app_id
            || now_ms >= entry.expires_ms
            || entry.opened.iter().any(|seen| seen == id)
            || entry.opened.len() >= OPENS_PER_TOKEN
            || self.stack.len() >= STACK_LIMIT
            || self.stack.iter().any(|view| view.id == id)
        {
            return ModalChange::Ignored;
        }
        if let Some(timeout) = event.rest.get("timeout_range") {
            let Some(timeout) = timeout.as_u64() else {
                return ModalChange::Ignored;
            };
            if elapsed >= timeout {
                return ModalChange::Ignored;
            }
        }
        let Some(previous) = optional_id(event.rest.get("previous_view_id")) else {
            return ModalChange::Ignored;
        };
        if previous != self.stack.last().map(|view| view.id.as_str())
            || self
                .stack
                .last()
                .is_some_and(|view| view.app_id != app_id || view.kind != kind)
        {
            return ModalChange::Ignored;
        }
        let origin_conversation = if let Some(parent) = self.stack.last() {
            // An origin-less compatibility token can inherit its exact parent.
            // An explicitly different origin cannot join an existing lineage.
            if entry.origin_conversation.is_some()
                && entry.origin_conversation != parent.origin_conversation
            {
                return ModalChange::Ignored;
            }
            parent.origin_conversation.clone()
        } else {
            entry.origin_conversation.clone()
        };
        let viewed_conversation = if let Some(parent) = self.stack.last() {
            if entry.viewed_conversation.is_some()
                && entry.viewed_conversation != parent.viewed_conversation
            {
                return ModalChange::Ignored;
            }
            parent.viewed_conversation.clone()
        } else {
            entry.viewed_conversation.clone()
        };
        let payload = event.rest.get("view").filter(|value| !value.is_null());
        let root = self.stack.first().map_or(id, |view| view.id.as_str());
        if payload.is_some_and(|value| !valid_view(value, id, app_id, kind, previous, root)) {
            return ModalChange::Ignored;
        }
        self.stack.push(ModalView {
            id: id.into(),
            app_id: app_id.into(),
            kind: kind.into(),
            previous_view_id: previous.map(str::to_owned),
            origin_conversation,
            viewed_conversation,
            payload: payload.cloned(),
            needs_content: payload.is_none(),
        });
        self.tokens[index].opened.push(id.into());
        self.revision = self.revision.wrapping_add(1);
        ModalChange::Opened
    }

    fn update(&mut self, event: &RawEvent) -> ModalChange {
        let Some(id) = field(event, "view_id") else {
            return ModalChange::Ignored;
        };
        let Some(app_id) = field(event, "app_id") else {
            return ModalChange::Ignored;
        };
        let Some(index) = self
            .stack
            .iter()
            .position(|view| view.id == id && view.app_id == app_id)
        else {
            return ModalChange::Ignored;
        };
        let current = &self.stack[index];
        let payload = event.rest.get("view").filter(|value| !value.is_null());
        if payload.is_some_and(|value| {
            !valid_view(
                value,
                id,
                app_id,
                &current.kind,
                current.previous_view_id.as_deref(),
                &self.stack[0].id,
            )
        }) {
            return ModalChange::Ignored;
        }
        // `view_updated` has no ordering sequence in the captured handler.
        // Transport order is authoritative; opaque hashes are not sortable.
        if let Some(payload) = payload {
            self.stack[index].payload = Some(payload.clone());
        }
        self.stack[index].needs_content = payload.is_none();
        if index + 1 == self.stack.len() {
            self.revision = self.revision.wrapping_add(1);
        }
        ModalChange::Updated
    }

    /// Local back/close only. This does not send views.close or model a
    /// nonexistent `view_closed` event. Drop tokens so delayed opens cannot
    /// resurrect a locally dismissed modal. `clear_on_close` clears the stack.
    pub fn pop_local(&mut self, scope: &ModalScope) -> bool {
        if scope != &self.scope || self.stack.is_empty() {
            return false;
        }
        let clear = self
            .stack
            .last()
            .and_then(|view| view.payload.as_ref())
            .and_then(|view| view.get("clear_on_close"))
            .and_then(Value::as_bool)
            == Some(true);
        if clear {
            self.stack.clear();
        } else {
            self.stack.pop();
        }
        self.tokens.clear();
        self.revision = self.revision.wrapping_add(1);
        true
    }

    pub fn clear_local(&mut self, scope: &ModalScope) {
        if scope == &self.scope {
            self.stack.clear();
            self.tokens.clear();
            self.revision = self.revision.wrapping_add(1);
        }
    }
}

fn field<'a>(event: &'a RawEvent, key: &str) -> Option<&'a str> {
    nonempty(event.rest.get(key)?)
}
fn nonempty(value: &Value) -> Option<&str> {
    value.as_str().filter(|value| !value.trim().is_empty())
}
fn modal_kind(kind: &str) -> bool {
    matches!(
        kind,
        "modal" | "app_notification_subscription_configuration"
    )
}
/// Missing/null/empty previous IDs denote a root. Other malformed types fail.
fn optional_id(value: Option<&Value>) -> Option<Option<&str>> {
    match value {
        None | Some(Value::Null) => Some(None),
        Some(Value::String(value)) => Some((!value.is_empty()).then_some(value.as_str())),
        _ => None,
    }
}
fn valid_view(
    value: &Value,
    id: &str,
    app: &str,
    kind: &str,
    previous: Option<&str>,
    root: &str,
) -> bool {
    if !value.is_object()
        || value.get("id").and_then(nonempty) != Some(id)
        || value.get("type").and_then(nonempty) != Some(kind)
    {
        return false;
    }
    if value
        .get("app_id")
        .is_some_and(|value| nonempty(value) != Some(app))
    {
        return false;
    }
    if let Some(value) = value.get("previous_view_id")
        && optional_id(Some(value)) != Some(previous)
    {
        return false;
    }
    if let Some(value) = value.get("root_view_id")
        && nonempty(value) != Some(root)
    {
        return false;
    }
    true
}

#[cfg(test)]
mod origin_tests;
#[cfg(test)]
mod tests;
