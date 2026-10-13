//! Time display and availability borrow the cached parsed model, never a new snapshot.
use super::*;

pub(crate) struct Presentation {
    pub enabled: bool,
    pub busy: bool,
    pub reason: Option<String>,
    pub placeholder: String,
    pub revision: u64,
    pub timezone: Option<String>,
}
pub(crate) fn presentation(
    shell: &ShellState,
    source: &Source,
    metadata: &BlockElementMetadata,
) -> Presentation {
    let mut view = Presentation {
        enabled: false,
        busy: false,
        reason: Some(UNAVAILABLE.into()),
        placeholder: "Choose a time".into(),
        revision: 0,
        timezone: None,
    };
    let Some(entry) = shell
        .message_buttons
        .inputs
        .get(&ContainerKey::new(source))
        .filter(|entry| entry.source.same_action_revision(source) && source.current(shell))
    else {
        return view;
    };
    let Some(key) = field_key(metadata) else {
        return view;
    };
    let Some(field) = entry
        .model
        .fields
        .get(&key)
        .filter(|field| matches!(field.kind, InputKind::Time { .. }))
    else {
        return view;
    };
    view.placeholder = field.placeholder.clone().unwrap_or(view.placeholder);
    view.revision = entry.revision;
    if let InputKind::Time { timezone } = &field.kind {
        view.timezone = timezone.clone();
    }
    if entry.controls.get(&key) != Some(metadata) {
        return view;
    }
    let status = super::super::operation_status(shell, source, metadata);
    view.enabled = status.enabled;
    view.busy = status.busy;
    view.reason = status.reason;
    view
}
/// Ordinary captured timepickers are required. Incomplete/empty native edits
/// never become a clear action, and cannot update shared state.
pub(crate) fn change(
    mut state: Signal<ShellState>,
    source: Source,
    metadata: BlockElementMetadata,
    value: String,
    mounted: display::Mounted,
) {
    if value.is_empty() {
        invalidate(&mut state.write(), &source);
        return;
    }
    super::change(state, source, metadata, Some(value), mounted);
}
