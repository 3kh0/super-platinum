//! Explicit message entry, independent of About and without opening a bot DM.
use super::*;
use crate::message_buttons::source::Source;
mod back;
pub(crate) mod fixture;
pub(super) use back::leave;
#[cfg(test)]
mod tests;

#[derive(Clone)]
pub(super) struct Target {
    workflow: String,
    execution: Option<String>,
    auto_pending: bool,
    loaded: bool,
}
pub(crate) fn showing(s: &ShellState) -> bool {
    s.app_home
        .about
        .management
        .surface
        .as_ref()
        .is_some_and(|h| h.owner.direct() && visible(s, h.id))
}
pub(crate) fn open(
    mut state: Signal<ShellState>,
    source: Source,
    workflow: String,
    execution: Option<String>,
) {
    let visit = {
        let mut s = state.write();
        if s.overlay.is_some() || super::showing(&s) {
            return;
        }
        let Some(owner) = Owner::message(&s, source) else {
            return;
        };
        if cooling(&s, &owner.ctx) || (crate::fixture::is_fixture() && !fixture::enabled(&s)) {
            return;
        }
        let visit = next();
        let account = s.core.active_account.clone();
        s.app_home.about.management.surface = Some(Surface {
            id: visit,
            about: None,
            entry: Some(Target {
                workflow: workflow.clone(),
                execution,
                auto_pending: false,
                loaded: false,
            }),
            owner,
            account,
            query: ListQuery::default(),
            list: List::default(),
            document: None,
            read: None,
            policy: None,
            notice: None,
            fixture: crate::fixture::is_fixture(),
        });
        visit
    };
    read::editor(state, visit, workflow);
}
pub(crate) fn close(s: &mut ShellState, visit: u64) {
    if showing(s)
        && s.app_home
            .about
            .management
            .surface
            .as_ref()
            .is_some_and(|h| h.id == visit)
    {
        s.app_home.about.management.surface = None;
    }
}
pub(super) fn retry(state: Signal<ShellState>, visit: u64) {
    let target = {
        let s = state.peek();
        if !showing(&s) {
            return;
        }
        s.app_home
            .about
            .management
            .surface
            .as_ref()
            .filter(|h| h.id == visit && h.read.is_none())
            .and_then(|h| h.entry.as_ref())
            .map(|t| t.workflow.clone())
    };
    if let Some(target) = target {
        read::editor(state, visit, target);
    }
}
/// Called in the successful read commit, before the editor's first native mount.
pub(super) fn loaded(s: &mut ShellState, visit: u64) {
    let Some(target) = s
        .app_home
        .about
        .management
        .surface
        .as_ref()
        .filter(|h| h.id == visit)
        .and_then(|h| h.entry.clone())
    else {
        return;
    };
    if target.loaded || document(s, visit).is_none_or(|d| d.editor.workflow_id() != target.workflow)
    {
        return;
    }
    if activity::execution::arm(s, visit, target.execution) {
        let target = s
            .app_home
            .about
            .management
            .surface
            .as_mut()
            .unwrap()
            .entry
            .as_mut()
            .unwrap();
        target.auto_pending = true;
        target.loaded = true;
    }
}
pub(super) fn ready(mut state: Signal<ShellState>, visit: u64) {
    // Internal mount token only. Do not render over a native buffer before the
    // existing metadata-to-Activity guard has acquired its original controls.
    #[allow(deprecated)]
    let armed = {
        let mut s = state.write_silent();
        s.app_home
            .about
            .management
            .surface
            .as_mut()
            .filter(|h| h.id == visit)
            .and_then(|h| h.entry.as_mut())
            .is_some_and(|t| std::mem::take(&mut t.auto_pending))
    };
    if armed {
        activity::show(state, visit);
    }
}
pub(crate) fn snapshot(s: &ShellState) -> Value {
    let h = s
        .app_home
        .about
        .management
        .surface
        .as_ref()
        .filter(|h| h.owner.direct());
    json!({"active":showing(s),"busy":h.is_some_and(|h|h.read.is_some()),"visit":h.map(|h|h.id),
        "workflow":h.and_then(|h|h.entry.as_ref()).map(|t|&t.workflow),
        "execution":h.and_then(|h|h.entry.as_ref()).and_then(|t|t.execution.as_ref()),
        "autoPending":h.and_then(|h|h.entry.as_ref()).is_some_and(|t|t.auto_pending),
        "notice":h.and_then(|h|h.notice.as_ref())})
}
