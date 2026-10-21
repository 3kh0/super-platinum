//! A channel-row origin, never an App Home workflow execution grant.
use super::{
    Browse, Owner, Row, ShellState, cooldown_scope, cooldown_snapshot, cooling, next,
    use_cooldown_wake,
};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex, Weak};
use super_platinum_core::channel_workflows::{PreviewKind, details::Projection};

pub(crate) mod fixture;
mod native;
mod read;
mod view;

pub(crate) struct Selection {
    id: u64,
    owner: Arc<Owner>,
    lease: Weak<()>,
    browse_serial: u64,
    group: String,
    index: usize,
    trigger: String,
    source: Value,
    preview: Value,
    button: String,
    thread: Option<String>,
    profile: Option<String>,
    group_panel: Option<String>,
    model: Mutex<Model>,
}
#[derive(Default)]
struct Model {
    serial: u64,
    status: &'static str,
    phase: Option<&'static str>,
    error: Option<(&'static str, &'static str)>,
    trigger: Option<Value>,
    workflow: Option<Value>,
    projection: Option<Projection>,
    reads: Vec<Value>,
}
impl Selection {
    fn current(&self, s: &ShellState) -> bool {
        self.owner.current(s)
            && s.thread_root == self.thread
            && s.profile_user == self.profile
            && s.group_panel.as_ref().map(|p| &p.id) == self.group_panel.as_ref()
            && self.lease.strong_count() > 0
    }
    fn expected(&self) -> Value {
        json!({"id":self.id.to_string(),"owner":self.owner.id.to_string(),
            "browseSerial":self.browse_serial.to_string(),"button":self.button,
            "trigger":self.trigger})
    }
    fn selected(&self, s: &ShellState) -> bool {
        self.current(s)
            && s.channel_workflows
                .details
                .as_ref()
                .is_some_and(|v| v.id == self.id)
    }
    fn snapshot(&self, s: &ShellState) -> Value {
        let m = self.model.lock().expect("workflow details model");
        json!({"selection":{"id":self.id,"browseSerial":self.browse_serial,"owner":self.owner.snapshot(),
            "group":self.group,"index":self.index,"triggerId":self.trigger,
            "source":self.source,"previewRaw":self.preview,
            "context":{"location":"bookmark","channelId":self.owner.channel(),"bookmarkId":self.trigger},
            "threadRoot":self.thread,"profileUser":self.profile,"groupId":self.group_panel},
            "serial":m.serial,"status":if self.selected(s){m.status}else{"retired"},
            "current":self.selected(s),"phase":m.phase,"reads":m.reads,
            "triggerRaw":m.trigger,"workflowRaw":m.workflow,"cooldown":cooldown_snapshot(s),
            "projection":m.projection.as_ref().map(view::projection_snapshot)})
    }
}
fn original_row<'a>(b: &'a Browse, group: &str, index: usize) -> Option<&'a Row> {
    let rows = b.rows.as_ref()?;
    let rows = match group {
        "featured" => rows.featured(),
        "bookmarked" => rows.bookmarked(),
        _ => return None,
    };
    // Indexes are the actual displayed list, including the inert private rows.
    rows.iter().filter(|r| r.preview().displayable()).nth(index)
}
pub(crate) fn visible(s: &ShellState) -> bool {
    s.channel_workflows
        .details
        .as_ref()
        .is_some_and(|d| d.current(s))
}
pub(crate) fn snapshot(s: &ShellState) -> Value {
    s.channel_workflows
        .details
        .as_ref()
        .map(|d| d.snapshot(s))
        .unwrap_or(Value::Null)
}
pub(super) fn open(
    mut state: Signal<ShellState>,
    browse: Signal<Browse>,
    group: String,
    index: usize,
) {
    let selection = {
        let s = state.peek();
        let b = browse.peek();
        if !b.current(&s) || !matches!(b.status, "ready" | "partial") {
            return;
        }
        if b.lease.is_none() {
            return;
        }
        let Some(owner) = b.owner.clone() else {
            return;
        };
        let Some(row) = original_row(&b, &group, index) else {
            return;
        };
        if !matches!(row.preview().kind(), PreviewKind::Available) {
            return;
        }
        let Some(preview) = row.preview().raw() else {
            return;
        };
        Arc::new(Selection {
            id: next(),
            owner,
            lease: Arc::downgrade(b.lease.as_ref().expect("ready browse lease")),
            browse_serial: b.serial,
            group: group.clone(),
            index,
            trigger: row.preview().id().into(),
            source: row.source().clone(),
            preview: preview.clone(),
            button: format!("channel-workflow-open-{group}-{index}"),
            thread: s.thread_root.clone(),
            profile: s.profile_user.clone(),
            group_panel: s.group_panel.as_ref().map(|p| p.id.clone()),
            model: Mutex::new(Model {
                status: "idle",
                ..Default::default()
            }),
        })
    };
    spawn(async move {
        if !selection.current(&state.peek())
            || !native::capture(&selection).await
            || !selection.current(&state.peek())
        {
            return;
        }
        if state
            .peek()
            .channel_workflows
            .details
            .as_ref()
            .is_some_and(|d| d.id >= selection.id)
        {
            return;
        }
        state.write().channel_workflows.details = Some(selection);
    });
}
fn close(mut state: Signal<ShellState>, selection: Arc<Selection>) {
    spawn(async move {
        if !selection.selected(&state.peek())
            || !native::bind(&selection).await
            || !selection.selected(&state.peek())
            || !native::current(&selection, "close").await
            || !selection.selected(&state.peek())
        {
            return;
        }
        state.write().channel_workflows.details = None;
        if selection.current(&state.peek()) {
            native::restore(&selection).await;
        }
    });
}
#[component]
pub(crate) fn Panel(state: Signal<ShellState>) -> Element {
    use_cooldown_wake(state);
    let selection = state.read().channel_workflows.details.clone();
    // Keys participate in fragment reconciliation. A key on the component's
    // root alone does not remount it, leaving a successor's initial read idle.
    rsx! {
        {selection.filter(|d| d.current(&state.read()))
            .into_iter().map(|selection| view::panel(state, selection))}
    }
}
