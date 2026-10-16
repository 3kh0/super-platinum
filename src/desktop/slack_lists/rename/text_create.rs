//! Explicit ordinary Text/Number/Checkbox creation, retaining the exact item population.
use super::*;
use std::sync::Arc;
use super_platinum_core::slack_lists::{
    ColumnCreate, ColumnCreateAttempt, ColumnCreateKind, ListError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Choice {
    pub revision: u64,
    pub kind: ColumnCreateKind,
}
impl Choice {
    pub(super) fn initial() -> Self {
        Self {
            revision: next_id(),
            kind: ColumnCreateKind::Text,
        }
    }
}
pub(crate) fn choice(shell: &ShellState) -> Option<Choice> {
    entry(shell)
        .filter(|e| e.target.is_text_creation())
        .map(|e| e.field_choice)
}
pub(crate) fn label(kind: ColumnCreateKind) -> &'static str {
    match kind {
        ColumnCreateKind::Text => "Text",
        ColumnCreateKind::NumberDefault0 => "Number",
        ColumnCreateKind::Checkbox => "Checkbox",
    }
}
fn parse_kind(value: &str) -> Option<ColumnCreateKind> {
    match value {
        "text" => Some(ColumnCreateKind::Text),
        "number" => Some(ColumnCreateKind::NumberDefault0),
        "checkbox" => Some(ColumnCreateKind::Checkbox),
        _ => None,
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Observed {
    pub id: String,
    pub key: String,
    pub name: String,
}
fn user(shell: &ShellState) -> Option<&str> {
    let scope = shell
        .slack_lists
        .scope
        .as_ref()
        .filter(|s| s.current(shell))?;
    Some(&shell.core.workspaces.get(&scope.team)?.self_user_id)
}
pub(crate) fn can_open(shell: &ShellState) -> bool {
    if shell.slack_lists.view.as_ref().is_none_or(|v| {
        v.layout.notice.is_some() || v.layout.kind != crate::slack_lists::ProjectionLayout::Table
    }) {
        return false;
    }
    let Some(id) = selected_view(shell) else {
        return false;
    };
    let Some(user) = user(shell) else {
        return false;
    };
    can_target(shell, &Target::NewTextField(id.into()))
        && shell
            .slack_lists
            .view
            .as_ref()
            .and_then(|v| v.loaded.as_ref())
            .is_some_and(|l| ColumnCreate::supports(&l.document, id, user))
}
pub(crate) fn open(mut state: Signal<ShellState>, visit: u64, selection: u64) {
    open_shell(&mut state.write(), visit, selection);
}
pub(crate) fn open_shell(shell: &mut ShellState, visit: u64, selection: u64) -> bool {
    if super::selection(shell) != Some(selection) || !can_open(shell) {
        return false;
    }
    let id = selected_view(shell).unwrap().to_owned();
    open_target(shell, Target::NewTextField(id), visit)
}
/// Serialize only on explicit open, before retaining a second bounded snapshot.
pub(super) fn capture_population(shell: &ShellState, target: &Target) -> Option<Arc<RecordsPage>> {
    let Target::NewTextField(view) = target else {
        return None;
    };
    let loaded = shell.slack_lists.view.as_ref()?.loaded.as_ref()?;
    if !ColumnCreate::supports(&loaded.document, view, user(shell)?) {
        return None;
    }
    bounded_population(shell)
}
/// Current read/review custody is independent of the original create snapshot.
/// This is action-only, and refuses oversize/inconsistent representations.
pub(super) fn bounded_population(shell: &ShellState) -> Option<Arc<RecordsPage>> {
    let loaded = shell.slack_lists.view.as_ref()?.loaded.as_ref()?;
    if serde_json::to_vec(&loaded.records.raw).ok()?.len() > registry::MAX_DOCUMENT
        || RecordsPage::parse(loaded.records.raw.clone()).ok()? != loaded.records
    {
        return None;
    }
    Some(Arc::new(loaded.records.clone()))
}
pub(super) fn population_current(shell: &ShellState, entry: &registry::Entry) -> bool {
    entry.original_records.as_ref().is_none_or(|expected| {
        shell
            .slack_lists
            .view
            .as_ref()
            .and_then(|v| v.loaded.as_ref())
            .is_some_and(|l| l.records == **expected)
    })
}
pub(crate) fn input(
    mut state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    selection: u64,
    rendered: Choice,
    value: String,
) {
    if input_shell(&state.peek(), serial, visit, selection, rendered, value) {
        state.write();
    }
}
pub(crate) fn input_shell(
    shell: &ShellState,
    serial: u64,
    visit: u64,
    selection: u64,
    rendered: Choice,
    value: String,
) -> bool {
    if value.encode_utf16().count() > 256 {
        return false;
    }
    change(
        shell,
        serial,
        visit,
        selection,
        rendered,
        Some(value),
        rendered.kind,
    )
}
pub(crate) fn choose_kind(
    mut state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    selection: u64,
    rendered: Choice,
    value: String,
) {
    if kind_shell(&state.peek(), serial, visit, selection, rendered, &value) {
        state.write();
    }
}
pub(crate) fn kind_shell(
    shell: &ShellState,
    serial: u64,
    visit: u64,
    selection: u64,
    rendered: Choice,
    value: &str,
) -> bool {
    let Some(kind) = parse_kind(value) else {
        return false;
    };
    change(shell, serial, visit, selection, rendered, None, kind)
}
fn change(
    shell: &ShellState,
    serial: u64,
    visit: u64,
    selection: u64,
    rendered: Choice,
    name: Option<String>,
    kind: ColumnCreateKind,
) -> bool {
    if generation(shell) != Some(visit)
        || super::selection(shell) != Some(selection)
        || !owner::available(shell)
    {
        return false;
    }
    let Some(key) = Key::current(shell) else {
        return false;
    };
    if registry::blocked(&key.owner) {
        return false;
    }
    with_registry(|r| {
        let Some(e) = r.entries.get_mut(&key).filter(|e| {
            e.serial == serial
                && e.stage == Stage::Editing
                && e.target.is_text_creation()
                && selection_current(shell, e)
                && population_current(shell, e)
                && e.field_choice == rendered
        }) else {
            return false;
        };
        if shell
            .slack_lists
            .view
            .as_ref()
            .and_then(|v| v.loaded.as_ref())
            .is_none_or(|l| l.document != e.original)
        {
            return false;
        }
        let Input::NewTextField(old_name) = &e.input else {
            return false;
        };
        let next_name = name.unwrap_or_else(|| old_name.clone());
        if next_name == *old_name && kind == e.field_choice.kind {
            return false;
        }
        e.input = Input::NewTextField(next_name);
        // A type epoch guards callbacks across type ABA. Name events from the
        // same rendered handler remain ordered even before the next render.
        if kind != e.field_choice.kind {
            e.field_choice = Choice {
                revision: next_id(),
                kind,
            };
        }
        true
    })
}
pub(super) fn capture(
    shell: &ShellState,
    entry: &registry::Entry,
    loaded: &Loaded,
) -> Result<Intent, ListError> {
    if let Target::NewTextField(view) = &entry.target {
        let Input::NewTextField(name) = &entry.input else {
            return Err(ListError("The field input changed."));
        };
        let expected = entry
            .original_records
            .as_deref()
            .ok_or(ListError("The loaded items were not retained."))?;
        if loaded.records != *expected {
            return Err(ListError("The loaded items changed. Review a new field."));
        }
        ColumnCreate::capture_kind(
            &entry.original,
            expected,
            view,
            name,
            entry.field_choice.kind,
            user(shell).ok_or(ListError("The authenticated owner changed."))?,
            loaded.allow_lists.as_deref(),
            loaded.personal_task_lists.as_ref(),
        )
        .map(Intent::NewTextField)
    } else {
        entry.target.capture(
            &entry.original,
            &entry.input,
            loaded.allow_lists.as_deref(),
            loaded.personal_task_lists.as_ref(),
        )
    }
}
pub(super) fn admit(
    shell: &ShellState,
    intent: &Intent,
    fresh: &network::Metadata,
) -> Option<Permit> {
    if let Intent::NewTextField(intent) = intent {
        let records = &shell.slack_lists.view.as_ref()?.loaded.as_ref()?.records;
        ColumnCreateAttempt::new(intent.clone())
            .admit(
                &fresh.document,
                records,
                user(shell)?,
                fresh.allow.as_deref(),
                Some(&fresh.personal),
            )
            .ok()
            .map(Permit::NewTextField)
    } else {
        intent.admit(&fresh.document, fresh.allow.as_deref(), &fresh.personal)
    }
}
/// Probe the actual selected Table with retained records, Find and context.
/// This action-only projection never guesses a new field's placement/visibility.
pub(super) fn matching(
    shell: &ShellState,
    intent: &Intent,
    fresh: &network::Metadata,
) -> Option<Observed> {
    let Intent::NewTextField(intent) = intent else {
        return None;
    };
    if user(shell)? != intent.authenticated_user()
        || fresh.personal.contains(&fresh.document.id)
        || selected_view(shell) != Some(intent.view_id())
    {
        return None;
    }
    let mut probe = shell.slack_lists.view.clone()?;
    let loaded = probe.loaded.as_mut()?;
    let column = intent.matching_addition(&fresh.document, &loaded.records)?;
    loaded.document = fresh.document.clone();
    loaded.allow_lists = fresh.allow.clone();
    loaded.personal_task_lists = Some(fresh.personal.clone());
    super::super::layout::rebuild(
        &mut probe,
        shell
            .core
            .active_team
            .as_ref()
            .and_then(|t| shell.core.workspaces.get(t)),
    );
    if probe.layout.selected.as_deref() != Some(intent.view_id())
        || probe.layout.notice.is_some()
        || !matches!(
            probe.layout.kind,
            super_platinum_core::slack_lists::saved_views::ProjectionLayout::Table
        )
        || !probe.layout.columns.iter().any(|c| c.id == column.id)
    {
        return None;
    }
    Some(Observed {
        id: column.id.clone(),
        key: column.key.clone(),
        name: column.name.clone(),
    })
}
