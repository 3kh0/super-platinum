//! Creation-specific navigation over the shared schema operation receipt.
use super::*;
use super_platinum_core::slack_lists::ViewLayout;
pub(crate) fn can_open(shell: &ShellState) -> bool {
    can_target(shell, &Target::NewView(ViewLayout::Table, None))
}
pub(crate) fn open(mut state: Signal<ShellState>, visit: u64, selection: u64) {
    open_shell(&mut state.write(), visit, selection);
}
pub(super) fn open_shell(shell: &mut ShellState, visit: u64, selection: u64) -> bool {
    if super::selection(shell) != Some(selection) {
        return false;
    }
    open_target(shell, Target::NewView(ViewLayout::Table, None), visit)
}
pub(crate) fn layout(mut state: Signal<ShellState>, serial: u64, value: ViewLayout) {
    let shell = state.peek();
    let Some(key) = Key::current(&shell) else {
        return;
    };
    with_registry(|r| {
        if let Some(e) = r.entries.get_mut(&key).filter(|e| {
            e.serial == serial && e.stage == Stage::Editing && e.target.creation().is_some()
        }) {
            e.target = Target::NewView(value, e.target.grouping().map(str::to_owned));
        }
    });
    drop(shell);
    state.write();
}
/// The event carries its mounted visit and receipt serial. A reopened editor
/// must not adopt a delayed selection, even for the same List and field key.
pub(crate) fn grouping(mut state: Signal<ShellState>, serial: u64, visit: u64, value: String) {
    grouping_shell(&mut state.write(), serial, visit, &value);
}
pub(super) fn grouping_shell(shell: &mut ShellState, serial: u64, visit: u64, value: &str) -> bool {
    if generation(shell) != Some(visit) || !owner::available(shell) {
        return false;
    }
    let Some(key) = Key::current(shell) else {
        return false;
    };
    if registry::blocked(&key.owner) {
        return false;
    }
    let Some(old) = entry(shell) else {
        return false;
    };
    let Some(layout) = old.target.creation() else {
        return false;
    };
    if old.serial != serial
        || old.stage != Stage::Editing
        || !selection_current(shell, &old)
        || shell
            .slack_lists
            .view
            .as_ref()
            .and_then(|v| v.loaded.as_ref())
            .is_none_or(|l| l.document != old.original)
        || (!value.is_empty()
            && !super_platinum_core::slack_lists::ViewCreate::grouping_columns(&old.original)
                .any(|c| c.key == value))
    {
        return false;
    }
    with_registry(|r| {
        r.entries.get_mut(&key).unwrap().target =
            Target::NewView(layout, (!value.is_empty()).then(|| value.to_owned()));
    });
    true
}
/// Probe the actual existing projector with the actual retained records/context.
/// This clone is action-only; no whole-document work is added to render paths.
fn projectable(shell: &ShellState, loaded: &network::Metadata, id: &str) -> bool {
    let Some(mut probe) = shell.slack_lists.view.clone() else {
        return false;
    };
    let Some(population) = probe.loaded.as_mut() else {
        return false;
    };
    population.document = loaded.document.clone();
    population.allow_lists = loaded.allow.clone();
    population.personal_task_lists = Some(loaded.personal.clone());
    probe.layout.selected = Some(id.into());
    probe.layout.manual = true;
    probe.layout.selection = None;
    super::super::layout::rebuild(
        &mut probe,
        shell
            .core
            .active_team
            .as_ref()
            .and_then(|t| shell.core.workspaces.get(t)),
    );
    probe.layout.selected.as_deref() == Some(id) && probe.layout.notice.is_none()
}
pub(super) fn matching(
    shell: &ShellState,
    intent: &Intent,
    loaded: &network::Metadata,
) -> Option<String> {
    let Intent::Create(intent) = intent else {
        return None;
    };
    if loaded.personal.contains(&loaded.document.id) {
        return None;
    }
    let id = intent.matching_addition(&loaded.document)?;
    projectable(shell, loaded, id).then(|| id.into())
}
/// Returned only after the exact-current synchronous navigation succeeded.
/// Renderer uses this post-transition clock, never a guessed next generation.
#[derive(serde::Serialize)]
pub(crate) struct Opened {
    pub serial: String,
    pub visit: String,
    pub selection: String,
    pub id: String,
}
pub(crate) fn open_available(mut state: Signal<ShellState>, serial: u64) -> Option<Opened> {
    open_available_shell(&mut state.write(), serial)
}
pub(super) fn open_available_shell(shell: &mut ShellState, serial: u64) -> Option<Opened> {
    let key = Key::current(shell)?;
    let e = entry(shell)?;
    if e.serial != serial
        || e.stage != Stage::Available
        || e.target.creation().is_none()
        || !e.review.as_ref()?.navigation_current(shell)
    {
        return None;
    }
    let id = e.available.as_ref()?;
    let view = shell.slack_lists.view.as_ref()?;
    let loaded = view.loaded.as_ref()?;
    let metadata = network::Metadata {
        experiments: None,
        document: loaded.document.clone(),
        allow: loaded.allow_lists.clone(),
        personal: loaded.personal_task_lists.clone()?,
    };
    if matching(shell, e.intent.as_ref()?, &metadata).as_ref() != Some(id) {
        return None;
    }
    let visit = view.generation;
    let generation = view.layout.generation;
    // Existing selection reducer is synchronous and issues no request.
    if !super::super::layout::select_shell(shell, Some(id.clone()), visit, generation) {
        return None;
    }
    let view = shell.slack_lists.view.as_ref()?;
    if view.layout.selected.as_ref() != Some(id) || view.layout.notice.is_some() {
        return None;
    }
    let opened = Opened {
        serial: serial.to_string(),
        visit: visit.to_string(),
        selection: view.layout.selection_generation.to_string(),
        id: id.clone(),
    };
    with_registry(|r| {
        r.entries.remove(&key);
    });
    Some(opened)
}
