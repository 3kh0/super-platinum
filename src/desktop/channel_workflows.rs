//! Read-only, component-owned Channel Workflows collections. Preview rows do not
//! grant execution, management, or a workflow-modal origin.
use crate::state::{ChannelTab, ShellState};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
use super_platinum_core::channel_workflows::{Collections, Previews, Row, Rows};

mod cooldown;
pub(crate) mod details;
pub(crate) mod fixture;
/// The read throttle survives view mounts; the details selection grants only
/// presentation for one original available row.
#[derive(Default)]
pub(crate) struct Root {
    cooldown: cooldown::Root,
    details: Option<Arc<details::Selection>>,
}
impl Root {
    pub(crate) fn gate(&self, scope: &cooldown::Scope, now: std::time::Instant) -> cooldown::Gate {
        self.cooldown.gate(scope, now)
    }
    pub(crate) fn record(
        &mut self,
        scope: cooldown::Scope,
        method: &'static str,
        retry_after: Option<u64>,
        now: std::time::Instant,
    ) -> bool {
        self.cooldown.record(scope, method, retry_after, now)
    }
}
mod owner;
mod read;
#[cfg(test)]
mod tests;
mod view;
use owner::{Mount, Owner};

fn next() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

fn cooldown_scope(s: &ShellState) -> Option<cooldown::Scope> {
    Some(cooldown::Scope {
        account: s.core.active_account.clone(),
        epoch: s.core.account_epoch,
        team: s.core.active_team.clone()?,
    })
}
fn cooling(s: &ShellState) -> cooldown::Gate {
    cooldown_scope(s)
        .map(|scope| s.channel_workflows.gate(&scope, std::time::Instant::now()))
        .unwrap_or_default()
}
fn cooldown_snapshot(s: &ShellState) -> Value {
    let gate = cooling(s);
    json!({"blocked":gate.blocked,"retryAfterSeconds":gate.retry_after_seconds,"indefinite":gate.indefinite})
}

/// One panel-scoped wake, never an automatic read or a deadline-clearing task.
/// A delayed old wake cannot shorten a subsequently extended retained deadline.
fn use_cooldown_wake(state: Signal<ShellState>) {
    use std::{cell::RefCell, time::Instant};
    let timer = use_hook(|| Rc::new(RefCell::new(None::<(Instant, dioxus::core::Task)>)));
    let mut tick = use_signal(|| 0u64);
    use_effect(move || {
        let _ = *tick.read();
        let deadline = cooling(&state.read()).wake_at;
        let mut timer = timer.borrow_mut();
        if timer.as_ref().map(|(t, _)| *t) == deadline {
            return;
        }
        if let Some((_, task)) = timer.take() {
            task.cancel();
        }
        if let Some(deadline) = deadline {
            let task = spawn(async move {
                tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
                let next = (*tick.peek()).wrapping_add(1);
                tick.set(next);
            });
            *timer = Some((deadline, task));
        }
    });
    let _ = *tick.read();
}

/// Configured source tabs and an explicitly selected Add destination. Selecting
/// the latter never writes a fabricated tab record into channel metadata.
pub(crate) fn available(s: &ShellState) -> bool {
    if !s.conversation_visible() {
        return false;
    }
    if s.channel_tab == ChannelTab::Workflows {
        return true;
    }
    // App-channel Home/About tabs have a separate source selector. A raw
    // configured record alone does not replace that branch.
    !crate::app_home::eligible(s)
        && owner::channel_raw(s)
            .and_then(|v| {
                v.pointer("/properties/tabz")
                    .and_then(Value::as_array)
                    .cloned()
            })
            .is_some_and(|tabs| {
                tabs.iter()
                    .any(|v| v["type"] == "workflows" && v["isDisabled"] != true)
            })
}

struct Browse {
    // Only the component owns this lease; replacing rows or unmounting revokes
    // every details selection without retaining a renderer Signal in ShellState.
    lease: Option<Arc<()>>,
    mount: Option<Arc<Mount>>,
    owner: Option<Arc<Owner>>,
    serial: u64,
    status: &'static str,
    phase: Option<&'static str>,
    notice: Option<&'static str>,
    bookmarks: Option<Value>,
    previews: Option<Value>,
    collections: Option<Collections>,
    rows: Option<Rows>,
    reads: Vec<Value>,
}
impl Browse {
    fn new(s: &ShellState) -> Self {
        Self {
            lease: None,
            mount: Mount::capture(s).map(Arc::new),
            owner: None,
            serial: 0,
            status: "idle",
            phase: None,
            notice: None,
            bookmarks: None,
            previews: None,
            collections: None,
            rows: None,
            reads: vec![],
        }
    }
    fn current(&self, s: &ShellState) -> bool {
        self.mount.as_ref().is_some_and(|m| m.current(s))
            && self.owner.as_ref().is_none_or(|o| o.current(s))
    }
    fn snapshot(&self, s: &ShellState) -> Value {
        let current = self.current(s);
        json!({"owner":self.owner.as_ref().map(|o|o.snapshot()),"serial":self.serial,
            "status":if current {self.status}else{"retired"},"current":current,
            "phase":if current {self.phase}else{None},"notice":self.notice,"cooldown":cooldown_snapshot(s),
            "channelRaw":self.owner.as_ref().map(|o|&o.raw),"bookmarksRaw":self.bookmarks,
            "previewsRaw":self.previews,"reads":self.reads,
            "featured":self.rows.as_ref().map(|r|r.featured().iter().map(row_snapshot).collect::<Vec<_>>()).unwrap_or_default(),
            "bookmarked":self.rows.as_ref().map(|r|r.bookmarked().iter().map(row_snapshot).collect::<Vec<_>>()).unwrap_or_default()})
    }
}
fn row_snapshot(row: &Row) -> Value {
    json!({"id":row.preview().id(),"kind":view::kind(row),"title":view::title(row),
        "description":view::description(row),"source":row.source(),"previewRaw":row.preview().raw()})
}

#[component]
pub(crate) fn Panel(state: Signal<ShellState>) -> Element {
    let browse = use_signal(|| Browse::new(&state.peek()));
    use_cooldown_wake(state);
    // onmounted owns the only automatic load. Shell/media rerenders never
    // dispatch reads. Component-scoped tasks are canceled when this panel exits.
    view::panel(state, browse)
}
