//! Inline modal member search. Selection changes are local form edits, never actions.
use super::*;
use crate::message_buttons::inputs::user::{conversations, directory};
use std::sync::Arc;
use super_platinum_core::{
    block_actions::form::InputKind,
    slack::{
        Error,
        models::{Channel, User},
    },
};

#[derive(Clone)]
pub(crate) struct Ticket {
    context: Option<Arc<Context>>,
    scope: ModalScope,
    generation: u64,
    revision: u64,
    view: String,
    key: InputKey,
    field_revision: u64,
    raw: Value,
    self_user_id: String,
    filter: Option<conversations::Filter>,
}
impl Ticket {
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }
    pub(crate) fn capture(shell: &ShellState, key: &InputKey) -> Option<Self> {
        let session = shell.app_modal.as_ref()?;
        if !current_scope(shell, session) || shell.overlay != Some(Overlay::AppModal) {
            return None;
        }
        let form = session.current_form()?;
        // Unsupported interaction/visibility rules cannot accidentally become local-only edits.
        if !form.unsupported.is_empty() {
            return None;
        }
        let field = form.fields.get(key)?;
        if unavailable(shell, &field.raw).is_some() {
            return None;
        }
        if !matches!(
            field.kind,
            InputKind::User
                | InputKind::MultiUser { .. }
                | InputKind::MultiChannel { .. }
                | InputKind::MultiConversation { .. }
                | InputKind::Conversation
                | InputKind::Channel
        ) {
            return None;
        }
        let filter = if matches!(
            field.kind,
            InputKind::Conversation
                | InputKind::Channel
                | InputKind::MultiChannel { .. }
                | InputKind::MultiConversation { .. }
        ) {
            let filter = entity_filter(shell, &session.modal.scope().team, &field.raw)?;
            if entity_unavailable(&field.raw, &filter).is_some() {
                return None;
            }
            Some(filter)
        } else {
            None
        };
        let context = Context::capture(shell).map(Arc::new);
        Some(Self {
            context,
            filter,
            scope: session.modal.scope().clone(),
            generation: session.generation,
            revision: session.modal.revision(),
            view: form.view_id.clone(),
            key: key.clone(),
            field_revision: field.revision,
            raw: field.raw.clone(),
            self_user_id: shell
                .core
                .workspaces
                .get(&session.modal.scope().team)?
                .self_user_id
                .clone(),
        })
    }
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        if shell.overlay != Some(Overlay::AppModal)
            || unavailable(shell, &self.raw).is_some()
            || shell
                .core
                .workspaces
                .get(&self.scope.team)
                .is_none_or(|w| w.self_user_id != self.self_user_id)
            || self.filter.as_ref().is_some_and(|f| {
                entity_filter(shell, &self.scope.team, &self.raw).as_ref() != Some(f)
            })
            || !self
                .context
                .as_ref()
                .map_or(shell.core.transport.is_none(), |c| {
                    directory::context_current(shell, c)
                })
        {
            return false;
        }
        shell.app_modal.as_ref().is_some_and(|s| {
            current_scope(shell, s)
                && !s.locked()
                && s.modal.scope() == &self.scope
                && s.generation == self.generation
                && s.modal.revision() == self.revision
                && s.current_form().is_some_and(|f| {
                    f.view_id == self.view
                        && f.unsupported.is_empty()
                        && f.fields.get(&self.key).is_some_and(|v| {
                            v.revision == self.field_revision
                                && v.raw == self.raw
                                && matches!(
                                    v.kind,
                                    InputKind::User
                                        | InputKind::MultiUser { .. }
                                        | InputKind::MultiChannel { .. }
                                        | InputKind::MultiConversation { .. }
                                        | InputKind::Conversation
                                        | InputKind::Channel
                                )
                        })
                })
        })
    }
    fn same_field(&self, other: &Self) -> bool {
        self.generation == other.generation
            && self.revision == other.revision
            && self.view == other.view
            && self.key == other.key
            && self.field_revision == other.field_revision
            && self.raw == other.raw
            && self.scope == other.scope
            && self.self_user_id == other.self_user_id
    }
}
pub(crate) struct Picker {
    ticket: Ticket,
    serial: u64,
    query_revision: u64,
    query: String,
    candidates: Vec<directory::Choice>,
    loading: bool,
    error: Option<String>,
    agent_unavailable: bool,
}
pub(crate) struct SearchView {
    pub serial: u64,
    pub query_revision: u64,
    pub query: String,
    pub candidates: Vec<directory::Choice>,
    pub loading: bool,
    pub error: Option<String>,
    pub agent_unavailable: bool,
}
pub(crate) fn presentation(shell: &ShellState, ticket: &Ticket) -> Option<SearchView> {
    let p = shell.app_modal.as_ref()?.users.as_ref()?;
    (p.ticket.same_field(ticket) && p.ticket.current(shell)).then(|| SearchView {
        serial: p.serial,
        query_revision: p.query_revision,
        query: p.query.clone(),
        candidates: p.candidates.clone(),
        loading: p.loading,
        error: p.error.clone(),
        agent_unavailable: p.agent_unavailable,
    })
}
pub(crate) fn labels(
    shell: &ShellState,
    ticket: &Ticket,
    users: &[String],
) -> Vec<(String, String)> {
    let workspace = shell.core.workspaces.get(&ticket.scope.team);
    let session = shell.app_modal.as_ref();
    users
        .iter()
        .map(|id| {
            let label = workspace
                .filter(|_| ticket.filter.is_some())
                .and_then(|w| {
                    w.channels
                        .get(id)
                        .map(|c| conversations::channel_label(w, c))
                })
                .or_else(|| {
                    workspace
                        .filter(|w| w.users.contains_key(id))
                        .map(|w| w.display_name(id))
                })
                .or_else(|| session.and_then(|s| s.user_labels.get(id).cloned()))
                .unwrap_or_else(|| id.clone());
            (id.clone(), label)
        })
        .collect()
}
fn cached(shell: &ShellState, ticket: &Ticket, query: &str) -> Vec<directory::Choice> {
    shell
        .core
        .workspaces
        .get(&ticket.scope.team)
        .map_or_else(Vec::new, |w| {
            if let Some(filter) = &ticket.filter {
                conversations::candidates(
                    w,
                    filter,
                    w.users.values().map(|u| (u, false)),
                    w.channels.values().map(|c| (c, false)),
                    query,
                )
            } else {
                directory::candidates(w, w.users.values().map(|u| (u, false)), query)
            }
        })
}
pub(crate) fn open(mut state: Signal<ShellState>, ticket: Ticket) {
    open_local(&mut state.write(), ticket);
}
fn open_local(shell: &mut ShellState, ticket: Ticket) -> bool {
    if !ticket.current(shell) {
        return false;
    }
    if presentation(shell, &ticket).is_some() {
        return true;
    }
    let candidates = cached(shell, &ticket, "");
    let agent_unavailable = ticket.filter.as_ref().is_none_or(|f| f.allows("im"))
        && shell
            .core
            .workspaces
            .get(&ticket.scope.team)
            .is_some_and(|w| {
                w.users
                    .values()
                    .any(|u| u.extra.get("is_agentforce_bot") == Some(&Value::Bool(true)))
            });
    shell.app_modal.as_mut().unwrap().users = Some(Picker {
        ticket,
        serial: next_id(),
        query_revision: next_id(),
        query: String::new(),
        candidates,
        loading: false,
        error: None,
        agent_unavailable,
    });
    true
}
fn request_current(shell: &ShellState, serial: u64, query_revision: u64) -> bool {
    shell
        .app_modal
        .as_ref()
        .and_then(|s| s.users.as_ref())
        .is_some_and(|p| {
            p.serial == serial && p.query_revision == query_revision && p.ticket.current(shell)
        })
}
fn cooling(shell: &ShellState, ticket: &Ticket) -> bool {
    directory::cooling(shell, ticket.scope.account_epoch, &ticket.scope.team)
}
pub(crate) fn search(mut state: Signal<ShellState>, ticket: Ticket, query: String) {
    let request = {
        let mut shell = state.write();
        if !open_local(&mut shell, ticket.clone()) {
            return;
        }
        let choices = cached(&shell, &ticket, &query);
        let wait = cooling(&shell, &ticket);
        let p = shell.app_modal.as_mut().unwrap().users.as_mut().unwrap();
        if p.query == query && (p.loading || p.error.is_none()) {
            return;
        }
        p.query = query.clone();
        p.query_revision = next_id();
        p.candidates = choices;
        p.error = wait.then(|| {
            "Slack asked us to wait before searching again. Cached choices are shown.".into()
        });
        p.loading = !query.trim().is_empty() && ticket.context.is_some() && !wait;
        p.loading
            .then(|| (ticket.context.clone().unwrap(), p.serial, p.query_revision))
    };
    let Some((context, serial, revision)) = request else {
        return;
    };
    spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        {
            let mut shell = state.write();
            if !request_current(&shell, serial, revision)
                || !directory::context_current(&shell, &context)
            {
                return;
            }
            if cooling(&shell, &ticket) {
                let p = shell.app_modal.as_mut().unwrap().users.as_mut().unwrap();
                p.loading = false;
                p.error = Some(
                    "Slack asked us to wait before searching again. Cached choices are shown."
                        .into(),
                );
                return;
            }
        }
        let result = if let Some(filter) = &ticket.filter {
            directory::fetch_conversations(&context, &query, filter).await
        } else {
            directory::fetch_users(&context, &query)
                .await
                .map(|users| (users, vec![]))
        };
        let mut shell = state.write();
        if !directory::owner_current(&shell, &context) {
            return;
        }
        // Even a stale query carries an account-owned server cooldown.
        if let Err(Error::RateLimited {
            retry_after_secs: Some(seconds),
            ..
        }) = &result
        {
            cooldown(&mut shell, &ticket, *seconds);
        }
        complete_entities(&mut shell, serial, revision, result);
    });
}
fn cooldown(shell: &mut ShellState, ticket: &Ticket, seconds: u64) {
    let now = std::time::Instant::now();
    let deadline = now
        .checked_add(std::time::Duration::from_secs(seconds.max(1)))
        .unwrap_or(now + std::time::Duration::from_secs(86400));
    directory::remember_cooldown(
        shell,
        ticket.scope.account_epoch,
        &ticket.scope.team,
        deadline,
    );
}
#[cfg(test)]
fn complete(
    shell: &mut ShellState,
    serial: u64,
    revision: u64,
    result: Result<Vec<User>, Error>,
) -> bool {
    complete_entities(shell, serial, revision, result.map(|users| (users, vec![])))
}
fn complete_entities(
    shell: &mut ShellState,
    serial: u64,
    revision: u64,
    result: Result<(Vec<User>, Vec<Channel>), Error>,
) -> bool {
    if !request_current(shell, serial, revision) {
        return false;
    }
    let p = shell.app_modal.as_ref().unwrap().users.as_ref().unwrap();
    let ticket = p.ticket.clone();
    let query = p.query.clone();
    match result {
        Ok((users, channels)) => {
            let Some(w) = shell.core.workspaces.get(&ticket.scope.team) else {
                return false;
            };
            let choices = if let Some(filter) = &ticket.filter {
                conversations::candidates(
                    w,
                    filter,
                    users
                        .iter()
                        .map(|u| (u, true))
                        .chain(w.users.values().map(|u| (u, false))),
                    channels
                        .iter()
                        .map(|c| (c, true))
                        .chain(w.channels.values().map(|c| (c, false))),
                    &query,
                )
            } else {
                directory::candidates(
                    w,
                    users
                        .iter()
                        .map(|u| (u, true))
                        .chain(w.users.values().map(|u| (u, false))),
                    &query,
                )
            };
            let unavailable = users
                .iter()
                .any(|u| u.extra.get("is_agentforce_bot") == Some(&Value::Bool(true)));
            let session = shell.app_modal.as_mut().unwrap();
            super::channels::refresh(session, &channels);
            super::channels::refresh_users(session, &users);
            let p = session.users.as_mut().unwrap();
            p.candidates = choices;
            p.agent_unavailable |=
                ticket.filter.as_ref().is_none_or(|f| f.allows("im")) && unavailable;
            p.error = None;
            p.loading = false;
        }
        Err(error) => {
            let delay = match error {
                Error::RateLimited {
                    retry_after_secs: Some(s),
                    ..
                } => s.max(1),
                _ => 5,
            };
            cooldown(shell, &ticket, delay);
            let p = shell.app_modal.as_mut().unwrap().users.as_mut().unwrap();
            p.loading = false;
            p.error = Some("Could not search Slack. Cached choices are shown.".into());
        }
    }
    true
}
pub(crate) fn choose(
    mut state: Signal<ShellState>,
    ticket: Ticket,
    serial: u64,
    query_revision: u64,
    id: String,
) -> bool {
    choose_local(&mut state.write(), &ticket, serial, query_revision, &id)
}
fn choose_local(
    shell: &mut ShellState,
    ticket: &Ticket,
    serial: u64,
    query_revision: u64,
    id: &str,
) -> bool {
    if !ticket.current(shell) || !request_current(shell, serial, query_revision) {
        return false;
    }
    let p = shell.app_modal.as_ref().unwrap().users.as_ref().unwrap();
    if !p.ticket.same_field(ticket) {
        return false;
    }
    let Some(choice) = p.candidates.iter().find(|c| c.id == id).cloned() else {
        return false;
    };
    let Some(w) = shell.core.workspaces.get(&ticket.scope.team) else {
        return false;
    };
    if ticket.filter.as_ref().map_or_else(
        || w.users.get(id).is_some_and(|u| !directory::eligible(w, u)),
        |filter| !conversations::choice_current(w, filter, &choice),
    ) {
        return false;
    }
    let s = shell.app_modal.as_mut().unwrap();
    let form = s.current_form_mut().unwrap();
    let single = matches!(
        form.fields[&ticket.key].kind,
        InputKind::User | InputKind::Conversation | InputKind::Channel
    );
    let value = match form.fields[&ticket.key].value.clone() {
        InputValue::Channel(_) if single => InputValue::Channel(Some(id.into())),
        InputValue::Conversation(_) if single => InputValue::Conversation(Some(id.into())),
        InputValue::User(_) if single => InputValue::User(Some(id.into())),
        InputValue::Conversations(mut selected) if !single => {
            if let Some(index) = selected.iter().position(|v| v == id) {
                selected.remove(index);
            } else {
                if selected.len() >= 1000 {
                    return false;
                }
                selected.push(id.into());
            }
            InputValue::Conversations(selected)
        }
        InputValue::Channels(mut selected) if !single => {
            if let Some(index) = selected.iter().position(|v| v == id) {
                selected.remove(index);
            } else {
                if selected.len() >= 1000 {
                    return false;
                }
                selected.push(id.into());
            }
            InputValue::Channels(selected)
        }
        InputValue::Users(mut selected) if !single => {
            if let Some(index) = selected.iter().position(|v| v == id) {
                selected.remove(index);
            } else {
                if selected.len() >= 1000 {
                    return false;
                }
                selected.push(id.into());
            }
            InputValue::Users(selected)
        }
        _ => return false,
    };
    // Inline input controls keep over-limit selections visible and removable;
    // ModalForm validation prevents the final views.submit.
    if form.set_value(&ticket.key, value).is_err() {
        return false;
    }
    if let InputValue::Channels(selected) | InputValue::Conversations(selected) =
        &form.fields[&ticket.key].value
    {
        let selected = selected.clone();
        let proofs = s
            .channel_proofs
            .entry((ticket.view.clone(), ticket.key.clone()))
            .or_default();
        proofs.insert(id.into(), choice.clone());
        proofs.retain(|id, _| selected.contains(id));
    }
    if s.user_labels.len() >= 1000 && !s.user_labels.contains_key(id) {
        s.user_labels.pop_first();
    }
    s.user_labels.insert(id.into(), choice.label);
    if single {
        s.users = None;
    }
    true
}
pub(crate) fn remove(mut state: Signal<ShellState>, ticket: Ticket, id: String) -> bool {
    remove_local(&mut state.write(), &ticket, &id)
}
fn remove_local(shell: &mut ShellState, ticket: &Ticket, id: &str) -> bool {
    if !ticket.current(shell) {
        return false;
    }
    let form = shell
        .app_modal
        .as_mut()
        .unwrap()
        .current_form_mut()
        .unwrap();
    let field = &form.fields[&ticket.key];
    let value = match field.value.clone() {
        InputValue::Channel(Some(selected)) if field.optional && selected == id => {
            InputValue::Channel(None)
        }
        InputValue::Conversation(Some(selected)) if field.optional && selected == id => {
            InputValue::Conversation(None)
        }
        InputValue::User(Some(selected)) if field.optional && selected == id => {
            InputValue::User(None)
        }
        InputValue::Conversations(mut selected) if selected.iter().any(|v| v == id) => {
            selected.retain(|v| v != id);
            InputValue::Conversations(selected)
        }
        InputValue::Channels(mut selected) if selected.iter().any(|v| v == id) => {
            selected.retain(|v| v != id);
            InputValue::Channels(selected)
        }
        InputValue::Users(mut selected) if selected.iter().any(|v| v == id) => {
            selected.retain(|v| v != id);
            InputValue::Users(selected)
        }
        _ => return false,
    };
    form.set_value(&ticket.key, value).is_ok()
}
pub(crate) fn collapse(mut state: Signal<ShellState>, ticket: Ticket) {
    let mut shell = state.write();
    if shell
        .app_modal
        .as_ref()
        .and_then(|s| s.users.as_ref())
        .is_some_and(|p| p.ticket.same_field(&ticket))
    {
        shell.app_modal.as_mut().unwrap().users = None;
    }
}
pub(super) fn poll(shell: &mut ShellState) -> bool {
    if shell
        .app_modal
        .as_ref()
        .and_then(|s| s.users.as_ref())
        .is_some_and(|p| !p.ticket.current(shell))
    {
        shell.app_modal.as_mut().unwrap().users = None;
        true
    } else {
        false
    }
}

#[cfg(test)]
mod single_tests;
#[cfg(test)]
mod tests;

pub(crate) fn unavailable(shell: &ShellState, raw: &Value) -> Option<&'static str> {
    // Modal defaults require a separately attested viewed conversation. The
    // ordinary message renderer ignores this flag and supplies no such policy.
    if raw["type"] == "multi_conversations_select" && raw["default_to_current_conversation"] == true
    {
        if let Some(reason) = super::origin::unavailable(shell, raw) {
            return Some(reason);
        }
    }
    if !matches!(
        raw.get("type").and_then(Value::as_str),
        Some("conversations_select" | "multi_conversations_select" | "channels_select")
    ) {
        return None;
    }
    let team = &shell.app_modal.as_ref()?.modal.scope().team;
    let filter = entity_filter(shell, team, raw)?;
    entity_unavailable(raw, &filter)
}
pub(super) fn unavailable_context(shell: &ShellState) -> Option<&'static str> {
    shell
        .app_modal
        .as_ref()?
        .current_form()?
        .fields
        .values()
        .find_map(|field| unavailable(shell, &field.raw))
}
fn entity_unavailable(raw: &Value, filter: &conversations::Filter) -> Option<&'static str> {
    // Captured cdYm B13221618 derives enterprise mode via tpNE.k(state),
    // S3788507. Normal ViewBlocks B12690471/B12690746 supplies serviceTeamId from
    // viewContextTeamId || appInstalledTeamId. Raw-element guards cannot model it.
    if raw.get("type").and_then(Value::as_str) == Some("channels_select")
        && filter.enterprise_id.is_some()
    {
        Some("Channel selection is unavailable for this Enterprise Grid context.")
    } else {
        conversations::unavailable(filter)
    }
}

#[cfg(test)]
mod conversation_tests;

// cdYm has public-only, nonarchived channel discovery; no members or origin fallback.
fn entity_filter(shell: &ShellState, team: &str, raw: &Value) -> Option<conversations::Filter> {
    if matches!(
        raw.get("type").and_then(Value::as_str),
        Some("channels_select" | "multi_channels_select")
    ) {
        Some(conversations::Filter {
            spec: super_platinum_core::block_actions::form::ConversationFilter {
                include: Some(vec!["public".into()]),
                ..Default::default()
            },
            enterprise_id: shell
                .core
                .session
                .as_ref()
                .and_then(|s| s.workspaces.get(team))
                .and_then(|w| w.enterprise_id.clone()),
            ..Default::default()
        })
    } else {
        conversations::workspace_filter(shell, team, raw)
    }
}
#[cfg(test)]
mod channel_tests;

#[cfg(test)]
mod multi_channel_tests;

#[cfg(test)]
mod multi_conversation_tests;
