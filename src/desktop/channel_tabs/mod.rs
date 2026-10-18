//! Account-owned channel tab operations; no mutation replay on refresh/remount.
pub(crate) mod fixture;
pub(crate) mod folder;
pub(crate) mod knowledge;
mod owner;
pub(crate) mod selection;
use owner::Owner;
mod run;
use crate::state::ShellState;
use dioxus::prelude::*;
use std::{collections::BTreeMap, rc::Rc, time::Instant};
use super_platinum_core::channel_tabs::{Action, Intent, Inventory, Permission};

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Key {
    account: String,
    team: String,
    user: String,
    channel: String,
}
#[derive(Default)]
pub(crate) struct Root {
    entries: BTreeMap<Key, Entry>,
    navigation: selection::Navigation,
    next: u64,
    fixture: Option<fixture::Capability>,
}
#[derive(Default)]
pub(crate) struct Entry {
    pub(crate) folder: folder::State,
    pub(crate) inventory: Option<Inventory>,
    pub(crate) permission: Option<Permission>,
    pub(crate) busy: bool,
    pub(crate) pending: Option<Intent>,
    pub(crate) knowledge: Option<super_platinum_core::channel_tabs::KnowledgeRemoval>,
    serial: u64,
    pub(crate) notice: String,
    retry_at: Option<Instant>,
    pub(crate) cooling: bool,
}
pub(crate) fn carry(previous: &mut ShellState, next: &mut ShellState) {
    next.channel_tabs = std::mem::take(&mut previous.channel_tabs);
}
fn key(shell: &ShellState) -> Option<Key> {
    let team = shell.core.active_team.clone()?;
    let workspace = shell.core.workspaces.get(&team)?;
    Some(Key {
        account: shell.core.active_account.clone()?,
        team,
        user: workspace.self_user_id.clone(),
        channel: shell.core.active_channel.clone()?,
    })
}
pub(crate) fn entry(shell: &ShellState) -> Option<&Entry> {
    shell.channel_tabs.entries.get(&key(shell)?)
}
pub(crate) fn inventory(shell: &ShellState) -> Option<Inventory> {
    entry(shell).and_then(|e| e.inventory.clone()).or_else(|| {
        let k = key(shell)?;
        let c = shell
            .core
            .workspaces
            .get(&k.team)?
            .channels
            .get(&k.channel)?;
        Inventory::from_channel(&serde_json::to_value(c).ok()?).ok()
    })
}
/// Captured by the originating UI, never reconstructed by a retained callback.
#[derive(Clone)]
pub(crate) struct Visit(Rc<Owner>);
impl PartialEq for Visit {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
impl Visit {
    pub(crate) fn capture(shell: &ShellState) -> Option<Self> {
        Owner::capture(shell).map(|owner| Self(Rc::new(owner)))
    }
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        self.0.current(shell)
    }
}
pub(crate) fn refresh(state: Signal<ShellState>, visit: &Visit) {
    if entry(&state.peek()).is_some_and(|e| e.knowledge.is_some()) {
        knowledge::start(state, visit, None, false);
    } else {
        start(state, visit, None);
    }
}
pub(crate) fn start(
    mut state: Signal<ShellState>,
    visit: &Visit,
    action: Option<(Inventory, Action)>,
) {
    if !visit.current(&state.peek()) {
        return;
    }
    if action
        .as_ref()
        .is_some_and(|(_, action)| !selection::can_mutate(&state.peek(), action))
    {
        state
            .write()
            .show_toast("Finish or cancel the Canvas edit before removing its tab.");
        return;
    }
    let owner = (*visit.0).clone();
    let intent = match action {
        Some((base, action)) => match Intent::capture(base, action) {
            Ok(i) => Some(i),
            Err(_) => return,
        },
        None => None,
    };
    let serial = {
        let mut shell = state.write();
        let root = &mut shell.channel_tabs;
        if root.entries.get(&owner.key).is_some_and(|e| {
            e.busy
                || e.cooling
                || (intent.is_some() && (e.pending.is_some() || e.knowledge.is_some()))
        }) {
            return;
        }
        if !root.entries.contains_key(&owner.key) && root.entries.len() >= 32 {
            // Reclaim only settled idle presentation. Never discard an uncertain operation.
            let candidate = root
                .entries
                .iter()
                .find(|(_, e)| {
                    !e.busy && !e.cooling && e.pending.is_none() && e.knowledge.is_none()
                })
                .map(|(k, _)| k.clone());
            let Some(candidate) = candidate else {
                return;
            };
            root.entries.remove(&candidate);
        }
        let Some(serial) = root.next.checked_add(1) else {
            return;
        };
        root.next = serial;
        let e = root.entries.entry(owner.key.clone()).or_default();
        e.serial = serial;
        e.busy = true;
        e.notice.clear();
        serial
    };
    dioxus::core::spawn_forever(run::execute(state, owner, serial, intent));
}
fn current_entry<'a>(
    shell: &'a mut ShellState,
    owner: &Owner,
    serial: u64,
) -> Option<&'a mut Entry> {
    shell
        .channel_tabs
        .entries
        .get_mut(&owner.key)
        .filter(|e| e.serial == serial)
}
pub(crate) fn find_canvases(state: Signal<ShellState>, visit: &Visit, text: String) {
    let visit = visit.clone();
    dioxus::core::spawn_forever(async move {
        if !visit.current(&state.peek()) || visit.0.fixture.is_some() {
            return;
        }
        crate::files::load(
            state,
            super_platinum_core::files::FilesQuery {
                kind: super_platinum_core::files::FileKind::Canvases,
                channel: Some(visit.0.key.channel.clone()),
                text,
                ..Default::default()
            },
            1,
        )
        .await;
    });
}
pub(crate) fn open_canvas(state: Signal<ShellState>, visit: &Visit, base: Inventory, id: String) {
    selection::open(state, visit, base, id);
}
