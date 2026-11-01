//! Reveal only timestamps owned by this accepted getView publication. No IO,
//! canonical writes, pane-history borrowing, or general history-completeness grant.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use super_platinum_core::channel_threads::Discussion;

struct Window {
    minimum: Option<String>,
    revision: u64,
}
pub(super) struct Context {
    receipt: Arc<Accepted>,
    origin: Arc<()>,
    windows: BTreeMap<String, Window>,
}
impl Context {
    /// Called only at successful original guarded Core publication. Membership is
    /// the post-pruning accepted window, never the canonical bag's larger union.
    pub(super) fn seed(s: &ShellState, route: &Route, receipt: Arc<Accepted>) -> Self {
        let windows = receipt
            .discussions()
            .iter()
            .filter_map(|d| {
                records(s, route, d)?;
                let root = canonical_root(s, route, d.root())?;
                let bag = s.core.threads.get(&(
                    route.gate.team.clone(),
                    route.gate.channel.clone(),
                    d.root().into(),
                ))?;
                let read = bag
                    .last_read
                    .as_deref()
                    .or_else(|| root.extra.get("last_read").and_then(Value::as_str));
                Some((
                    d.root().into(),
                    Window {
                        minimum: channel_threads::minimum_visible(
                            root,
                            d.timestamps(),
                            read,
                            d.reached_start(),
                        ),
                        revision: 0,
                    },
                ))
            })
            .collect();
        Self {
            receipt,
            origin: Arc::new(()),
            windows,
        }
    }
}
fn context(s: &ShellState) -> Option<&Context> {
    let view = visible(s)?;
    let local = view.local.as_ref()?;
    Arc::ptr_eq(view.accepted.as_ref()?, &local.receipt).then_some(local)
}
/// None outside an owned context; Some(None) means the entire accepted context.
pub(super) fn minimum<'a>(s: &'a ShellState, root: &str) -> Option<&'a Option<String>> {
    Some(&context(s)?.windows.get(root)?.minimum)
}
pub(super) fn expanded(s: &ShellState, root: &str) -> bool {
    context(s)
        .and_then(|c| c.windows.get(root))
        .is_some_and(|w| w.revision > 0)
}
fn records(s: &ShellState, route: &Route, d: &Discussion) -> Option<()> {
    canonical_root(s, route, d.root())?;
    let key = (
        route.gate.team.clone(),
        route.gate.channel.clone(),
        d.root().into(),
    );
    let bag = s.core.threads.get(&key)?;
    let transcript = s.core.workspaces.get(&key.0)?.messages.get(&key.1);
    if bag.messages.len() > 20_000 {
        return None;
    }
    let expected = d
        .timestamps()
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    for m in &bag.messages {
        if let Some(ts) = m.ts.as_deref().filter(|ts| expected.contains(ts)) {
            if m.channel.as_deref() != Some(route.gate.channel.as_str())
                || m.thread_ts.as_deref() != Some(d.root())
                || !seen.insert(ts)
            {
                return None;
            }
        }
    }
    for ts in expected {
        if bag.was_deleted(ts)
            || transcript.is_some_and(|b| b.was_deleted(ts))
            || !seen.contains(ts)
        {
            return None;
        }
    }
    Some(())
}
fn next(d: &Discussion, w: &Window) -> Option<Option<String>> {
    let minimum = w.minimum.as_ref()?;
    // Source budgets owned timestamps BEFORE hidden-record suppression.
    let older = d
        .timestamps()
        .iter()
        .filter(|ts| *ts < minimum)
        .collect::<Vec<_>>();
    let result = if older.len() >= 100 {
        Some(older[older.len() - 100].to_string())
    } else if !older.is_empty() && d.reached_start() {
        None
    } else {
        return None; // Network/history cohorts remain outside this local action.
    };
    if result.as_ref().is_some_and(|ts| ts >= minimum)
        || d.timestamps()
            .iter()
            .filter(|ts| result.as_ref().is_none_or(|min| *ts >= min))
            .count()
            > 200
    {
        return None;
    }
    w.revision.checked_add(1)?;
    Some(result)
}
#[derive(Clone)]
pub(crate) struct Action {
    source: Source,
    receipt: Arc<Accepted>,
    origin: Arc<()>,
    root: String,
    minimum: Option<String>,
    revision: u64,
}
pub(crate) fn capture(s: &ShellState, root: &str) -> Option<Action> {
    let source = Source::capture(s)?;
    if !source.current(s) {
        return None;
    }
    let local = context(s)?;
    let d = local
        .receipt
        .discussions()
        .iter()
        .find(|d| d.root() == root)?;
    records(s, &source.route, d)?;
    let window = local.windows.get(root)?;
    next(d, window)?;
    Some(Action {
        source,
        receipt: local.receipt.clone(),
        origin: local.origin.clone(),
        root: root.into(),
        minimum: window.minimum.clone(),
        revision: window.revision,
    })
}
impl Action {
    pub fn reveal(&self, mut state: Signal<ShellState>) {
        let mut s = state.write();
        if !self.source.current(&s) {
            return;
        }
        let Some(local) = context(&s) else {
            return;
        };
        if !Arc::ptr_eq(&local.origin, &self.origin) || !Arc::ptr_eq(&local.receipt, &self.receipt)
        {
            return;
        }
        let Some(window) = local.windows.get(&self.root) else {
            return;
        };
        if window.minimum != self.minimum || window.revision != self.revision {
            return;
        }
        let Some(d) = local
            .receipt
            .discussions()
            .iter()
            .find(|d| d.root() == self.root)
        else {
            return;
        };
        if records(&s, &self.source.route, d).is_none() {
            return;
        }
        let Some(minimum) = next(d, window) else {
            return;
        };
        let revision = window.revision + 1; // checked by next, under the same write
        let window = s
            .channel_tabs
            .threads
            .local
            .as_mut()
            .unwrap()
            .windows
            .get_mut(&self.root)
            .unwrap();
        window.minimum = minimum;
        window.revision = revision;
    }
}
