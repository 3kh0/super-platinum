//! Explicit channel-tab routes. Ordinary channel opens still select Messages.
//! Navigation history is presentation only: it never grants Canvas/file access.
use super::*;
use crate::state::{CanvasTarget, ChannelTab};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CanvasRoute {
    pub tab: String,
    pub file: String,
}
impl CanvasRoute {
    pub fn capture(base: &Inventory, id: &str) -> Option<Self> {
        let row = base.canvas(id).ok()?;
        Some(Self {
            tab: id.into(),
            file: super_platinum_core::channel_tabs::file_id(row)?.into(),
        })
    }
    pub fn current(&self, base: &Inventory) -> bool {
        Self::capture(base, &self.tab).as_ref() == Some(self)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Route {
    Resource(ChannelTab),
    Canvas(CanvasRoute),
}
impl Route {
    pub fn tab(&self) -> ChannelTab {
        match self {
            Self::Resource(tab) => *tab,
            Self::Canvas(_) => ChannelTab::Canvas,
        }
    }
    fn valid(&self, shell: &ShellState) -> bool {
        match self {
            Self::Resource(ChannelTab::AppHome) => crate::app_home::eligible(shell),
            Self::Resource(tab) => *tab != ChannelTab::Canvas,
            Self::Canvas(route) => inventory(shell).is_some_and(|base| route.current(&base)),
        }
    }
}
#[derive(Default)]
pub(super) struct Navigation {
    entries: BTreeMap<Key, History>,
    clock: u64,
}
#[derive(Default)]
struct History {
    routes: Vec<Route>,
    index: usize,
    touched: u64,
}
const CHANNELS: usize = 128;
const ROUTES: usize = 64;

fn selected_identity(shell: &ShellState) -> Option<&CanvasRoute> {
    if shell.channel_tab != ChannelTab::Canvas {
        return None;
    }
    let h = shell.channel_tabs.navigation.entries.get(&key(shell)?)?;
    match h.routes.get(h.index)? {
        Route::Canvas(route) => Some(route),
        _ => None,
    }
}
/// This guard protects draft custody, not Slack permissions. Rechecked at handoff.
pub(crate) fn can_mutate(shell: &ShellState, action: &Action) -> bool {
    !matches!(action, Action::Remove { id } if selected_identity(shell).is_some_and(|r| &r.tab == id)
        && !crate::canvas::surface::can_leave(shell))
}
pub(crate) fn removal_pending(shell: &ShellState) -> bool {
    selected_identity(shell).is_some_and(|route| {
        entry(shell)
            .and_then(|e| e.pending.as_ref())
            .is_some_and(|i| matches!(i.action(),Action::Remove{id} if id==&route.tab))
    })
}

pub(crate) fn current(shell: &ShellState) -> Option<Route> {
    if shell.channel_tab != ChannelTab::Canvas {
        return Some(Route::Resource(shell.channel_tab));
    }
    let h = shell.channel_tabs.navigation.entries.get(&key(shell)?)?;
    h.routes.get(h.index).filter(|r| r.valid(shell)).cloned()
}
pub(crate) fn target(shell: &ShellState) -> Option<CanvasTarget> {
    let Route::Canvas(route) = current(shell)? else {
        return None;
    };
    let workspace = shell
        .core
        .workspaces
        .get(shell.core.active_team.as_ref()?)?;
    if workspace.deleted_files.contains(&route.file) {
        return None;
    }
    let base = inventory(shell)?;
    let label = super_platinum_core::channel_tabs::label(base.canvas(&route.tab).ok()?);
    Some(CanvasTarget {
        id: route.file,
        title: if label.is_empty() {
            "Canvas".into()
        } else {
            label.into()
        },
        permalink: None,
    })
}
#[cfg(test)]
pub(crate) fn selected(shell: &ShellState, id: &str) -> bool {
    matches!(current(shell), Some(Route::Canvas(route)) if route.tab == id)
}
/// A history callback holds both the exact destination and the original cursor.
/// A replacement selection cannot be moved by an old Back/Forward handler.
#[derive(Clone, PartialEq)]
pub(crate) struct Step {
    key: Key,
    from: usize,
    to: usize,
    touched: u64,
    pub route: Route,
}
pub(crate) fn step(shell: &ShellState, forward: bool) -> Option<Step> {
    let key = key(shell)?;
    let h = shell.channel_tabs.navigation.entries.get(&key)?;
    if h.routes.get(h.index) != current(shell).as_ref() {
        return None;
    }
    let to = if forward {
        h.index.checked_add(1)?
    } else {
        h.index.checked_sub(1)?
    };
    let route = h.routes.get(to)?.clone();
    route.valid(shell).then_some(Step {
        key,
        from: h.index,
        to,
        touched: h.touched,
        route,
    })
}
pub(crate) fn accepts(shell: &ShellState, step: &Step) -> bool {
    key(shell).as_ref() == Some(&step.key)
        && step.route.valid(shell)
        && shell
            .channel_tabs
            .navigation
            .entries
            .get(&step.key)
            .is_some_and(|h| {
                h.index == step.from
                    && h.touched == step.touched
                    && h.routes.get(h.index) == current(shell).as_ref()
                    && h.routes.get(step.to) == Some(&step.route)
            })
}
pub(crate) fn record(shell: &mut ShellState, route: Route, step: Option<&Step>) -> bool {
    if !route.valid(shell) || step.is_some_and(|s| s.route != route || !accepts(shell, s)) {
        return false;
    }
    let Some(key) = key(shell) else { return false };
    let nav = &mut shell.channel_tabs.navigation;
    if !nav.entries.contains_key(&key) && nav.entries.len() >= CHANNELS {
        // Only bounded navigation metadata is reclaimed; operation custody is separate.
        let oldest = nav
            .entries
            .iter()
            .min_by_key(|(_, h)| h.touched)
            .map(|(k, _)| k.clone());
        if let Some(oldest) = oldest {
            nav.entries.remove(&oldest);
        }
    }
    nav.clock = nav.clock.saturating_add(1);
    let h = nav.entries.entry(key).or_default();
    h.touched = nav.clock;
    if let Some(step) = step {
        h.index = step.to;
    } else if h.routes.get(h.index) != Some(&route) {
        if h.routes.is_empty() && route != Route::Resource(ChannelTab::Messages) {
            h.routes.push(Route::Resource(ChannelTab::Messages));
        }
        h.routes.truncate(h.index + 1);
        h.routes.push(route);
        if h.routes.len() > ROUTES {
            h.routes.remove(0);
        }
        h.index = h.routes.len() - 1;
    }
    true
}
pub(crate) fn ordinary_open(shell: &mut ShellState) {
    shell.channel_tab = ChannelTab::Messages;
    record(shell, Route::Resource(ChannelTab::Messages), None);
}
pub(crate) fn history(state: Signal<ShellState>, visit: &Visit, step: Step) {
    if !visit.current(&state.peek()) || !accepts(&state.peek(), &step) {
        return;
    }
    crate::channel_resources::select_route(
        state,
        step.route.clone(),
        Some(visit.clone()),
        Some(step),
    );
}
pub(crate) fn open(state: Signal<ShellState>, visit: &Visit, base: Inventory, id: String) {
    if !visit.current(&state.peek()) || inventory(&state.peek()).as_ref() != Some(&base) {
        return;
    }
    let Some(route) = CanvasRoute::capture(&base, &id) else {
        return;
    };
    crate::channel_resources::select_route(state, Route::Canvas(route), Some(visit.clone()), None);
}

/// Auto-selection is a consequence of this visit's just-confirmed Add only.
/// Refreshing a retained uncertain operation never acquires navigation authority.
pub(super) fn confirmed_added(
    intent: &Intent,
    current: &Inventory,
    just_dispatched: bool,
    originating_visit_current: bool,
) -> Option<String> {
    use super_platinum_core::channel_tabs::{Observation, file_id, id};
    if !just_dispatched
        || !originating_visit_current
        || intent.observe(current) != Observation::Satisfied
    {
        return None;
    }
    let Action::AddCanvas { file, .. } = intent.action() else {
        return None;
    };
    current
        .rows()
        .iter()
        .find(|row| {
            file_id(row) == Some(file.as_str())
                && id(row).is_some_and(|id| intent.base().row(id).is_none())
        })
        .and_then(id)
        .map(str::to_owned)
}

#[cfg(test)]
mod tests;
