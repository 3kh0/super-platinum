//! Read followed discussions inside an original, enabled channel Threads tab.
use super::{Inventory, ShellState, Visit};
use dioxus::prelude::*;
use serde_json::Value;
use std::{sync::Arc, time::Instant};
use super_platinum_core::{
    channel_threads::{self, Accepted, Page, Read},
    slack::Error,
};
pub(crate) mod fixture;
mod gate;
mod limits;
pub(super) use limits::Limits;
pub(crate) use limits::visible as limit_status;
#[cfg(test)]
mod io;
pub(crate) mod previous;
mod read;
pub(crate) mod recovery;
#[cfg(test)]
mod tests;

/// Existing system/custom inventory row plus the original positive gate.
/// Cloning or navigation never acquires a replacement assignment owner.
#[derive(Clone)]
pub(crate) struct Route {
    pub tab: String,
    gate: Arc<gate::Gate>,
}
impl PartialEq for Route {
    fn eq(&self, other: &Self) -> bool {
        self.tab == other.tab && Arc::ptr_eq(&self.gate, &other.gate)
    }
}
impl Eq for Route {}
impl std::fmt::Debug for Route {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChannelThreadsRoute")
            .field("tab", &self.tab)
            .finish_non_exhaustive()
    }
}
impl Route {
    pub fn capture(shell: &ShellState, base: &Inventory, id: &str) -> Option<Self> {
        if base.row(id)?["type"] != "threads" {
            return None;
        }
        Some(Self {
            tab: id.into(),
            gate: gate::Gate::capture(shell, base)?,
        })
    }
    pub fn channel(&self) -> &str {
        &self.gate.channel
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        self.gate.current(shell)
            && self
                .gate
                .base
                .row(&self.tab)
                .is_some_and(|r| r["type"] == "threads")
    }
}
#[derive(Default)]
pub(crate) struct State {
    route: Option<Route>,
    origin: Arc<()>,
    pending: Option<Arc<()>>,
    pending_scope: Option<read::Pending>,
    sequence: u64,
    accepted: Option<Arc<Accepted>>,
    local: Option<previous::Context>,
    pub notice: String,
    pub retry_at: Option<Instant>,
    pub rate_blocked: bool,
    pub presentation: Option<fixture::Presentation>,
}
impl State {
    pub fn accepted(&self) -> Option<&Accepted> {
        self.accepted.as_deref()
    }
    pub fn loading(&self) -> bool {
        self.pending.is_some()
    }
    pub fn loaded(&self) -> bool {
        self.accepted.is_some()
    }
}
fn selected(shell: &ShellState) -> Option<Route> {
    let super::selection::Route::Threads(route) = super::selection::current(shell)? else {
        return None;
    };
    route.current(shell).then_some(route)
}
pub(crate) fn visible(shell: &ShellState) -> Option<&State> {
    let r = selected(shell)?;
    let s = &shell.channel_tabs.threads;
    (s.route.as_ref() == Some(&r)).then_some(s)
}
#[derive(Clone)]
pub(crate) struct Source {
    route: Route,
    visit: Visit,
    origin: Arc<()>,
    sequence: u64,
    thread_generation: u64,
    shortcut_generation: u64,
}
impl Source {
    pub fn capture(shell: &ShellState) -> Option<Self> {
        let route = selected(shell)?;
        let s = visible(shell)?;
        Some(Self {
            route,
            visit: Visit::capture(shell)?,
            origin: s.origin.clone(),
            sequence: s.sequence,
            thread_generation: shell.thread_generation,
            shortcut_generation: shell.shortcut_generation,
        })
    }
    fn visit_current(&self, shell: &ShellState) -> bool {
        scene_current(
            shell,
            &self.route,
            &self.origin,
            self.thread_generation,
            self.shortcut_generation,
            self.visit.native_read_current(shell),
        )
    }
    fn read_check(&self) -> Option<super::owner::NativeReadCheck> {
        let native = self.visit.native_read_check()?;
        let route = self.route.clone();
        let origin = self.origin.clone();
        let thread = self.thread_generation;
        let shortcut = self.shortcut_generation;
        Some(Arc::new(move |shell| {
            scene_current(shell, &route, &origin, thread, shortcut, native(shell))
        }))
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        self.visit_current(shell)
            && shell.channel_tabs.threads.sequence == self.sequence
            && !shell.channel_tabs.threads.loading()
    }
    pub fn start(&self, state: Signal<ShellState>) {
        if self.current(&state.peek()) {
            dioxus::core::spawn_forever(read::run(state, self.clone()));
        }
    }
    pub fn open(&self, mut state: Signal<ShellState>, root: String) {
        let scope = {
            let mut s = state.write();
            if !self.current(&s) || !has_root(&s, &self.route, &root) {
                return;
            }
            s.begin_thread(root.clone(), None);
            s.remember_surface();
            s.thread_scope()
        };
        #[cfg(test)]
        {
            io::opened(state, &root, &scope);
            return;
        }
        #[cfg(not(test))]
        load_opened_thread(state, self.route.gate.channel.clone(), root, scope);
    }
}
fn scene_current(
    shell: &ShellState,
    route: &Route,
    origin: &Arc<()>,
    thread: u64,
    shortcut: u64,
    native: bool,
) -> bool {
    native
        && route.current(shell)
        && shell.overlay.is_none()
        && shell.conversation_visible()
        && selected(shell).as_ref() == Some(route)
        && shell.thread_generation == thread
        && shell.shortcut_generation == shortcut
        && Arc::ptr_eq(origin, &shell.channel_tabs.threads.origin)
        && shell.channel_tabs.threads.route.as_ref() == Some(route)
}
fn has_root(shell: &ShellState, route: &Route, root: &str) -> bool {
    visible(shell)
        .and_then(|s| s.accepted.as_ref())
        .is_some_and(|a| a.discussions().iter().any(|d| d.root() == root))
        && canonical_root(shell, route, root).is_some()
}
fn canonical_root<'a>(
    shell: &'a ShellState,
    route: &Route,
    root: &str,
) -> Option<&'a super_platinum_core::slack::models::Message> {
    let key = (
        route.gate.team.clone(),
        route.gate.channel.clone(),
        root.into(),
    );
    if shell
        .core
        .thread_subscriptions
        .active(shell.core.account_epoch, &key)
        == Some(false)
    {
        return None;
    }
    let bag = shell.core.threads.get(&key)?;
    if bag.was_deleted(root)
        || shell
            .core
            .workspaces
            .get(&key.0)
            .and_then(|w| w.messages.get(&key.1))
            .is_some_and(|b| b.was_deleted(root))
    {
        return None;
    }
    bag.messages.iter().find(|m| {
        m.ts.as_deref() == Some(root)
            && m.channel.as_deref() == Some(&route.gate.channel)
            && !channel_threads::hidden(m)
    })
}
/// Mount action, after rendering. It never manufactures canonical/gate facts.
pub(crate) fn enter(mut state: Signal<ShellState>, visit: &Visit, route: &Route) {
    {
        let mut s = state.write();
        if !visit.native_read_current(&s)
            || !route.current(&s)
            || selected(&s).as_ref() != Some(route)
        {
            return;
        }
        if s.channel_tabs.threads.route.as_ref() != Some(route) {
            s.channel_tabs.threads = State {
                route: Some(route.clone()),
                ..Default::default()
            };
        }
    }
    if let Some(source) = Source::capture(&state.peek()) {
        source.start(state);
    }
}

/// The row keeps the originally rendered route, never recaptures its gate.
pub(crate) fn open(state: Signal<ShellState>, visit: &Visit, route: Route) {
    if visit.native_read_current(&state.peek()) && route.current(&state.peek()) {
        crate::channel_resources::select_route(
            state,
            super::selection::Route::Threads(route),
            Some(visit.clone()),
            None,
        );
    }
}
pub(crate) struct Loaded<'a> {
    pub root: &'a super_platinum_core::slack::models::Message,
    pub replies: Vec<&'a super_platinum_core::slack::models::Message>,
    pub earlier: bool,
    pub previous: Option<previous::Action>,
}
pub(crate) fn loaded(shell: &ShellState) -> Vec<Loaded<'_>> {
    let Some(route) = selected(shell) else {
        return Vec::new();
    };
    let Some(accepted) = visible(shell).and_then(|s| s.accepted.as_ref()) else {
        return Vec::new();
    };
    accepted
        .discussions()
        .iter()
        .filter_map(|d| {
            let root = canonical_root(shell, &route, d.root())?;
            let key = (
                route.gate.team.clone(),
                route.gate.channel.clone(),
                d.root().into(),
            );
            let bag = shell.core.threads.get(&key)?;
            let read = bag
                .last_read
                .as_deref()
                .or_else(|| root.extra.get("last_read").and_then(Value::as_str));
            let min = previous::minimum(shell, d.root())
                .cloned()
                .unwrap_or_else(|| {
                    channel_threads::minimum_visible(root, d.timestamps(), read, d.reached_start())
                });
            let mut earlier = !d.reached_start();
            let mut replies = d
                .timestamps()
                .iter()
                .filter_map(|ts| {
                    if min.as_ref().is_some_and(|m| ts < m) {
                        earlier = true;
                        return None;
                    }
                    if bag.was_deleted(ts)
                        || shell
                            .core
                            .workspaces
                            .get(&key.0)
                            .and_then(|w| w.messages.get(&key.1))
                            .is_some_and(|b| b.was_deleted(ts))
                    {
                        return None;
                    }
                    bag.messages
                        .iter()
                        .find(|m| m.ts.as_ref() == Some(ts) && !channel_threads::hidden(m))
                })
                .collect::<Vec<_>>();
            if replies.len() > 100 && !previous::expanded(shell, d.root()) {
                earlier = true;
                replies.drain(..replies.len() - 100);
            }
            Some(Loaded {
                root,
                replies,
                earlier,
                previous: previous::capture(shell, d.root()),
            })
        })
        .collect()
}

// Kept compiled in test builds as well; only the scoped offline handoff executes there.
fn load_opened_thread(state: Signal<ShellState>, channel: String, root: String, scope: String) {
    dioxus::core::spawn_forever(crate::bootstrap::load_started_thread(
        state, channel, root, None, scope,
    ));
}
