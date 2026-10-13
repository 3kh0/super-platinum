//! Container state survives rendered bags and navigation, but never account ownership.
use super::{Captured, Pending, Source, next_id};
pub(crate) mod display;
#[cfg(test)]
mod radio_tests;
#[cfg(test)]
mod tests;
use crate::state::{Overlay, ShellState};
use dioxus::prelude::*;
use std::collections::BTreeMap;
use super_platinum_core::block_actions::{
    BlockElementMetadata,
    form::{InputKey, InputKind, InputValue, OptionSpec},
    message_state::MessageInputs,
};

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ContainerKey {
    epoch: u64,
    user: String,
    team: String,
    channel: String,
    ts: String,
}
impl ContainerKey {
    fn new(source: &Source) -> Self {
        let owner = source.owner();
        Self {
            epoch: owner.account_epoch,
            user: owner.self_user_id.clone(),
            team: owner.team_id.clone(),
            channel: owner.channel_id.clone(),
            ts: owner.message_ts.clone(),
        }
    }
}
#[derive(Clone)]
pub(super) struct Entry {
    source: Source,
    model: MessageInputs,
    revision: u64,
    controls: BTreeMap<InputKey, BlockElementMetadata>,
}
pub(super) type Cache = BTreeMap<ContainerKey, Entry>;
const MAX_CONTAINERS: usize = 256;
const UNAVAILABLE: &str = "This app control is unavailable or has changed";

fn candidate(shell: &ShellState, source: &Source) -> Result<Entry, &'static str> {
    let message = source.message(shell).map_err(|_| UNAVAILABLE)?;
    let mut entry =
        if let Some(entry) = shell.message_buttons.inputs.get(&ContainerKey::new(source)) {
            let mut entry = entry.clone();
            if entry.source == *source {
                return Ok(entry);
            }
            let before = entry.model.clone();
            entry.model.reconcile(message).map_err(|_| UNAVAILABLE)?;
            if entry.model != before {
                entry.revision = next_id();
            }
            entry.source = source.clone();
            entry
        } else {
            let model = MessageInputs::parse(message).map_err(|_| UNAVAILABLE)?;
            if !model.fields.is_empty() && shell.message_buttons.inputs.len() >= MAX_CONTAINERS {
                return Err("Too many app messages are open in this session");
            }
            Entry {
                source: source.clone(),
                model,
                revision: next_id(),
                controls: BTreeMap::new(),
            }
        };
    // Cache canonical eligibility without constructing action snapshots. Current
    // Source checks still guard every presentation.
    entry.controls = entry
        .model
        .fields
        .iter()
        .filter_map(|(key, field)| {
            let metadata = match &field.kind {
                InputKind::Radio { .. } => {
                    source.radio_metadata(shell, &key.block_id, &key.action_id)
                }
                _ => source.select_metadata(shell, &key.block_id, &key.action_id),
            };
            metadata.ok().map(|metadata| (key.clone(), metadata))
        })
        .collect();
    Ok(entry)
}
pub(crate) fn ensure(mut state: Signal<ShellState>, source: Source) {
    let entry = {
        let shell = state.peek();
        if shell
            .message_buttons
            .inputs
            .get(&ContainerKey::new(&source))
            .is_some_and(|entry| entry.source == source)
        {
            return;
        }
        candidate(&shell, &source)
    };
    if let Ok(entry) = entry {
        state
            .write()
            .message_buttons
            .inputs
            .insert(ContainerKey::new(&source), entry);
    }
}
pub(super) fn revision(shell: &ShellState, source: &Source) -> Option<u64> {
    shell
        .message_buttons
        .inputs
        .get(&ContainerKey::new(source))
        .map(|entry| entry.revision)
}
pub(super) fn invalidate(shell: &mut ShellState, source: &Source) {
    if let Some(entry) = shell
        .message_buttons
        .inputs
        .get_mut(&ContainerKey::new(source))
    {
        entry.revision = next_id();
    }
}
pub(super) fn prune(shell: &mut ShellState) {
    shell.message_buttons.inputs.retain(|key, _| {
        key.epoch == shell.core.account_epoch
            && shell
                .core
                .workspaces
                .get(&key.team)
                .is_some_and(|workspace| workspace.self_user_id == key.user)
    });
}
pub(super) fn pending_current(shell: &ShellState, pending: &Pending) -> bool {
    pending.captured.display_current()
        && pending.captured.revalidate(shell).is_ok()
        && pending.input_revision.is_none_or(|revision| {
            Some(revision) == self::revision(shell, pending.captured.source())
        })
}
pub(super) fn field_key(metadata: &BlockElementMetadata) -> Option<InputKey> {
    Some(InputKey {
        block_id: metadata.block_id.clone()?,
        action_id: metadata.action_id.clone()?,
    })
}

pub(crate) struct SelectPresentation {
    pub enabled: bool,
    pub busy: bool,
    pub reason: Option<String>,
    pub options: Vec<OptionSpec>,
    pub selected: Option<String>,
    pub placeholder: String,
    pub revision: u64,
}
pub(crate) fn presentation(
    shell: &ShellState,
    source: &Source,
    metadata: &BlockElementMetadata,
) -> SelectPresentation {
    let mut view = SelectPresentation {
        enabled: false,
        busy: false,
        reason: Some(UNAVAILABLE.into()),
        options: vec![],
        selected: None,
        placeholder: "Choose an option".into(),
        revision: 0,
    };
    let Some(entry) = shell
        .message_buttons
        .inputs
        .get(&ContainerKey::new(source))
        .filter(|entry| entry.source.same_action_revision(source) && source.current(shell))
    else {
        return view;
    };
    let Some(field) = field_key(metadata).and_then(|key| entry.model.fields.get(&key)) else {
        return view;
    };
    let options = match &field.kind {
        InputKind::StaticSelect { options, .. } | InputKind::Radio { options } => options,
        _ => return view,
    };
    view.options = options.clone();
    view.selected = match &field.value {
        InputValue::Selected(values) => values.first().cloned(),
        _ => None,
    };
    view.placeholder = field
        .placeholder
        .clone()
        .unwrap_or_else(|| "Choose an option".into());
    view.revision = entry.revision;
    let Some(key) = field_key(metadata) else {
        return view;
    };
    if entry.controls.get(&key) != Some(metadata) {
        return view;
    }
    let status = super::operation_status(shell, source, metadata);
    view.enabled = status.enabled;
    view.busy = status.busy;
    view.reason = status.reason;
    view
}
pub(crate) fn change(
    mut state: Signal<ShellState>,
    source: Source,
    metadata: BlockElementMetadata,
    selected: Option<String>,
    mounted: display::Mounted,
) {
    ensure(state, source.clone());
    let captured = {
        let mut shell = state.write();
        if shell.overlay.is_some() || !presentation(&shell, &source, &metadata).enabled {
            invalidate(&mut shell, &source);
            return;
        }
        let Some(key) = field_key(&metadata) else {
            return;
        };
        let captured = match &metadata.source {
            super_platinum_core::block_actions::BlockElementSource::UnsupportedControl { kind }
                if kind == "radio_buttons" =>
            {
                source.capture_radio(&shell, &key.block_id, &key.action_id, selected.as_deref())
            }
            _ => source.capture_select(&shell, &key.block_id, &key.action_id, selected.as_deref()),
        };
        let Ok(mut captured) = captured else {
            invalidate(&mut shell, &source);
            return;
        };
        if captured.metadata() != &metadata {
            return;
        }
        let Some(display) = mounted.capture(&ContainerKey::new(&source), &key) else {
            invalidate(&mut shell, &source);
            return;
        };
        captured.set_display(display);
        if captured.confirmation().is_some() {
            let input_revision = revision(&shell, &source);
            shell.message_buttons.confirmation = Some(Pending {
                captured,
                revision: next_id(),
                input_revision,
            });
            shell.overlay = Some(Overlay::ButtonConfirmation);
            return;
        }
        captured
    };
    super::dispatch(state, captured);
}

/// Prepare against a cloned model; publish state only after a valid request exists.
pub(super) fn prepare(
    shell: &mut ShellState,
    captured: &Captured,
    client: &super_platinum_core::slack::SlackClient,
    workspace: &super_platinum_core::config::WorkspaceSession,
    token: &str,
) -> Result<super_platinum_core::slack::PreparedRequest, &'static str> {
    if !captured.display_current() {
        return Err(UNAVAILABLE);
    }
    let current = captured.revalidate(shell).map_err(|_| UNAVAILABLE)?;
    let mut entry = candidate(shell, captured.source())?;
    if let Some(selected) = captured.selected_value() {
        let key = field_key(captured.metadata()).ok_or(UNAVAILABLE)?;
        entry
            .model
            .commit(&key, selected)
            .map_err(|_| UNAVAILABLE)?;
        entry.revision = next_id();
    } else {
        entry.model.initialize();
    }
    let values = entry.model.values().map_err(|_| UNAVAILABLE)?;
    let request = current
        .prepare(
            client,
            workspace,
            captured.app_id(),
            captured.app_team_id().ok_or(UNAVAILABLE)?,
            token,
            values.as_ref(),
        )
        .map_err(|_| UNAVAILABLE)?;
    if let Some(selected) = captured.selected_value() {
        if !captured.commit_display(selected) {
            return Err(UNAVAILABLE);
        }
    }
    // Stateless buttons need no retained cache entry, but still send initialized {}.
    if !entry.model.fields.is_empty() {
        shell
            .message_buttons
            .inputs
            .insert(ContainerKey::new(captured.source()), entry);
    }
    Ok(request)
}

pub(crate) use change as change_radio;
pub(crate) use presentation as radio_presentation;

/// Shared state only seeds a new mount. Subsequent display changes follow the
/// component's own props and successfully prepared actions from that mount.
pub(crate) fn display_selected(
    shell: &ShellState,
    source: &Source,
    metadata: &BlockElementMetadata,
    mounted: &display::Mounted,
) -> Option<String> {
    let entry = shell
        .message_buttons
        .inputs
        .get(&ContainerKey::new(source))
        .filter(|entry| entry.source.same_action_revision(source) && source.current(shell))?;
    let key = field_key(metadata)?;
    let field = entry.model.fields.get(&key)?;
    mounted.sync(
        ContainerKey::new(source),
        key.clone(),
        field,
        entry.model.saved_option(&key),
    )
}
