//! Original comment-file display facts, separate from full-file navigation.
//! Raw response objects enter before typed defaults; retained messages never change.
use super::{Owner, Query, State as Comments, Ticket};
use crate::state::ShellState;
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};
use super_platinum_core::{
    canvas::comments::file_facts,
    slack::{Error, models::Message},
};

mod known;
#[cfg(test)]
mod tests;
pub(super) use known::known;

#[derive(Default)]
pub(super) struct State {
    identity: Arc<()>,
    owner: Option<Read>,
    store: file_facts::Store,
    stamps: BTreeMap<String, Read>,
    origins: BTreeMap<String, Origin>,
}
struct Origin {
    message: Message,
    first: String,
    block_id: String,
}

/// Captured before any response await, including the containing Canvas read.
#[derive(Clone)]
pub(super) struct Read {
    owner: Owner,
    socket: u64,
    events: u64,
}
impl Read {
    pub(super) fn capture(shell: &ShellState, owner: &Owner) -> Option<Self> {
        let read = Self {
            owner: owner.clone(),
            socket: shell.core.workspaces.get(&owner.scope.team)?.rt_generation,
            events: crate::file_events::snapshot_revision(shell),
        };
        read.current(shell).then_some(read)
    }
    fn current(&self, shell: &ShellState) -> bool {
        self.owner.current(shell)
            && shell
                .core
                .workspaces
                .get(&self.owner.scope.team)
                .is_some_and(|w| w.rt_generation == self.socket)
    }
    fn file_current(&self, shell: &ShellState, id: &str) -> bool {
        self.current(shell)
            && !crate::file_events::changed_since(shell, &self.owner.scope.team, id, self.events)
            && shell
                .core
                .workspaces
                .get(&self.owner.scope.team)
                .is_some_and(|w| !w.deleted_files.contains(id))
    }
}

pub(super) fn retire(state: &mut State) {
    state.origins.clear();
}
/// Only the existing final-capture/local-save failure remount may retain these
/// original origins. New serial retires every old click; no source is refreshed.
pub(super) fn remount(model: &mut Comments) {
    let origins = std::mem::take(&mut model.file_source.origins);
    model.next_serial();
    model.file_source.origins = origins;
}
fn unavailable() -> Error {
    Error::Api("canvas_comment_file_source_unavailable".into())
}
fn same_message(a: &Message, b: &Message) -> bool {
    serde_json::to_value(a)
        .ok()
        .zip(serde_json::to_value(b).ok())
        .is_some_and(|(a, b)| a == b)
}
fn visible_root(message: &Message) -> bool {
    use super_platinum_core::canvas::comments::display::{RootBody, root_body};
    matches!(root_body(message), Some(body) if !matches!(body, RootBody::Hidden))
}

/// Atomic display publication after page/identity/pagination admission. Failure
/// removes this read's override eligibility, not the accepted fallback page.
pub(super) fn publish(
    shell: &ShellState,
    model: &mut Comments,
    ticket: &Ticket,
    read: &Read,
    messages: &[Value],
) -> Result<(), Error> {
    if !model.accepts(shell, ticket) {
        return Err(unavailable());
    }
    model.file_source.origins.clear();
    if !read.current(shell)
        || read.owner.file != ticket.owner.file
        || read.owner.visit != ticket.owner.visit
        || read.owner.scope.team != ticket.owner.scope.team
    {
        return Err(unavailable());
    }
    let batch = file_facts::embedded_files(messages).map_err(|_| unavailable())?;
    if messages.len() > 200
        || batch.iter().any(|row| {
            row["id"]
                .as_str()
                .is_none_or(|id| !read.file_current(shell, id))
        })
    {
        return Err(unavailable());
    }
    let mut origins = BTreeMap::new();
    for raw in messages {
        let message: Message = serde_json::from_value(raw.clone()).map_err(|_| unavailable())?;
        if !visible_root(&message) {
            continue;
        }
        let Ok(Some(id)) = file_facts::first_file_id(raw) else {
            continue;
        };
        let ts = message
            .ts
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or_else(unavailable)?;
        let block_id = raw["blocks"]
            .as_array()
            .and_then(|v| v.first())
            .and_then(|v| v["block_id"].as_str())
            .map(str::to_owned)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        origins.insert(
            ts.to_owned(),
            Origin {
                message,
                first: id.into(),
                block_id,
            },
        );
    }
    if model
        .file_source
        .owner
        .as_ref()
        .is_none_or(|old| !old.current(shell))
    {
        model.file_source = State::default();
    }
    let state = &mut model.file_source;
    state
        .store
        .observe_batch(&batch)
        .map_err(|_| unavailable())?;
    state.owner = Some(read.clone());
    for row in &batch {
        state.stamps.insert(
            row["id"].as_str().ok_or_else(unavailable)?.into(),
            read.clone(),
        );
    }
    state.origins = origins;
    Ok(())
}

#[derive(Clone)]
pub(super) struct Token {
    owner: Owner,
    query: Query,
    serial: u64,
    identity: Arc<()>,
    original: Arc<Message>,
    stamp: Read,
    observed: file_facts::Observation,
    name: String,
    url: String,
    block_id: String,
}
impl Token {
    pub(super) fn current(&self, shell: &ShellState, model: &Comments) -> bool {
        self.owner.current(shell)
            && !model.loading
            && model.serial == self.serial
            && model.query.as_ref() == Some(&self.query)
            && Arc::ptr_eq(&self.identity, &model.file_source.identity)
            && self.stamp.file_current(shell, self.id())
            && self.observed.current(&model.file_source.store)
            && model
                .rows
                .iter()
                .any(|row| same_message(&row.message, &self.original))
            && model
                .file_source
                .origins
                .get(self.message_ts())
                .is_some_and(|origin| {
                    origin.first == self.id() && same_message(&origin.message, &self.original)
                })
            && match &self.query {
                Query::Replies(ts) if ts == self.message_ts() => model
                    .root
                    .as_ref()
                    .is_some_and(|root| same_message(root, &self.original)),
                _ => true,
            }
    }
    pub(super) fn context(&self, shell: &ShellState) -> Option<crate::scheduled::Context> {
        self.owner.scope.request(shell)
    }
    pub(super) fn id(&self) -> &str {
        self.observed.id()
    }
    pub(super) fn team(&self) -> &str {
        &self.owner.scope.team
    }
    pub(super) fn name(&self) -> &str {
        &self.name
    }
    pub(super) fn url(&self) -> &str {
        &self.url
    }
    pub(super) fn block_id(&self) -> &str {
        &self.block_id
    }
    pub(super) fn message_ts(&self) -> &str {
        self.original.ts.as_deref().unwrap_or_default()
    }
}

pub(super) fn token(shell: &ShellState, model: &Comments, message: &Message) -> Option<Token> {
    if !visible_root(message) {
        return None;
    }
    let state = &model.file_source;
    let origin = state.origins.get(message.ts.as_deref()?)?;
    if !same_message(message, &origin.message) {
        return None;
    }
    let observed = state.store.get(&origin.first)?;
    let link = observed.link()?;
    let token = Token {
        owner: model.owner.clone()?,
        query: model.query.clone()?,
        serial: model.serial,
        identity: state.identity.clone(),
        original: Arc::new(message.clone()),
        stamp: state.stamps.get(&origin.first)?.clone(),
        name: link.name.into(),
        url: link.permalink.into(),
        observed,
        block_id: origin.block_id.clone(),
    };
    token.current(shell, model).then_some(token)
}

/// An original first-file occurrence, absent or invalidated by a file event. This is
/// display acquisition custody only: it cannot activate a link or seal a reply.
#[derive(Clone)]
pub(super) struct Cold {
    owner: Owner,
    query: Query,
    serial: u64,
    identity: Arc<()>,
    original: Arc<Message>,
    first: String,
    read: Read,
    predecessor: Option<file_facts::Observation>,
}
impl Cold {
    pub(super) fn id(&self) -> &str {
        &self.first
    }
    pub(super) fn ts(&self) -> &str {
        self.original.ts.as_deref().unwrap_or_default()
    }
    pub(super) fn scope(&self) -> &super::Scope {
        &self.owner.scope
    }
    pub(super) fn context(&self, shell: &ShellState) -> Option<crate::scheduled::Context> {
        self.owner.scope.request(shell)
    }
    /// Excludes the file-event/absence check so a still-owned failed attempt can
    /// display retirement without ever granting it another network request.
    pub(super) fn source_current(&self, shell: &ShellState, model: &Comments) -> bool {
        self.owner.current(shell)
            && !model.loading
            && model.serial == self.serial
            && model.query.as_ref() == Some(&self.query)
            && Arc::ptr_eq(&self.identity, &model.file_source.identity)
            && model
                .rows
                .iter()
                .any(|row| same_message(&row.message, &self.original))
            && model
                .file_source
                .origins
                .get(self.ts())
                .is_some_and(|origin| {
                    origin.first == self.first && same_message(&origin.message, &self.original)
                })
            && match &self.query {
                Query::Replies(ts) if ts == self.ts() => model
                    .root
                    .as_ref()
                    .is_some_and(|root| same_message(root, &self.original)),
                _ => true,
            }
    }
    pub(super) fn current(&self, shell: &ShellState, model: &Comments) -> bool {
        self.source_current(shell, model) && self.read.file_current(shell, self.id())
    }
    pub(super) fn absent(&self, shell: &ShellState, model: &Comments) -> bool {
        self.current(shell, model) && model.file_source.store.get(self.id()).is_none()
    }
    pub(super) fn known(&self) -> bool {
        self.predecessor.is_some()
    }
    pub(super) fn event_revision(&self) -> u64 {
        self.read.events
    }
    pub(super) fn demanded(&self, shell: &ShellState, model: &Comments) -> bool {
        self.current(shell, model)
            && match &self.predecessor {
                Some(original) => original.current(&model.file_source.store),
                None => model.file_source.store.get(self.id()).is_none(),
            }
    }
    pub(super) fn publish(
        &self,
        shell: &ShellState,
        model: &mut Comments,
        file: &Value,
    ) -> Result<(), Error> {
        if !self.demanded(shell, model) || file["id"].as_str() != Some(self.id()) {
            return Err(unavailable());
        }
        // No message, read serial, control, editor binding or reply state changes.
        model
            .file_source
            .store
            .observe_batch(std::slice::from_ref(file))
            .map_err(|_| unavailable())?;
        model
            .file_source
            .stamps
            .insert(self.first.clone(), self.read.clone());
        Ok(())
    }
}

pub(super) fn cold(shell: &ShellState, model: &Comments, message: &Message) -> Option<Cold> {
    if !visible_root(message) {
        return None;
    }
    let state = &model.file_source;
    let origin = state.origins.get(message.ts.as_deref()?)?;
    if !same_message(message, &origin.message) || state.store.get(&origin.first).is_some() {
        return None;
    }
    let cold = Cold {
        owner: model.owner.clone()?,
        query: model.query.clone()?,
        serial: model.serial,
        identity: state.identity.clone(),
        original: Arc::new(message.clone()),
        first: origin.first.clone(),
        read: state.owner.clone()?,
        predecessor: None,
    };
    cold.absent(shell, model).then_some(cold)
}

/// Passive current source evidence; never used to construct a click authority.
pub(in crate::canvas) fn snapshot(shell: &ShellState, model: &Comments) -> Value {
    let state = &model.file_source;
    let rows: Vec<_> = state
        .stamps
        .iter()
        .filter_map(|(id, stamp)| {
            let observed = state.store.get(id)?;
            Some(
                json!({"id":id,"source":observed.source(),"effective":observed.effective(),
            "predecessor":observed.predecessor(),"revision":observed.revision(),
            "current":stamp.file_current(shell,id)&&observed.current(&state.store),
            "link":observed.link().map(|l|json!({"name":l.name,"url":l.permalink}))}),
            )
        })
        .collect();
    let origins: Vec<_> = state
        .origins
        .iter()
        .map(|(ts, origin)| json!({"ts":ts,"firstId":origin.first,"blockId":origin.block_id}))
        .collect();
    json!({"serial":model.serial,"revision":state.store.revision(),
        "ownerCurrent":state.owner.as_ref().is_some_and(|o|o.current(shell)),
        "rows":rows,"origins":origins})
}
