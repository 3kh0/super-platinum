//! Member and conversation display and availability borrow the cached parsed model, never a new snapshot.
use super::*;

pub(crate) struct Presentation {
    pub enabled: bool,
    pub busy: bool,
    pub reason: Option<String>,
    pub placeholder: String,
    pub revision: u64,
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
        placeholder: if is_channel(metadata) {
            if is_multiple(metadata) {
                "Select channels"
            } else {
                "Select a channel"
            }
        } else if is_conversation(metadata) {
            if is_multiple(metadata) {
                "Select conversations"
            } else {
                "Select a conversation"
            }
        } else {
            if is_multiple(metadata) {
                "Select users"
            } else {
                "Select a user"
            }
        }
        .into(),
        revision: 0,
    };
    if let Some(reason) = super::channel::unavailable(shell, source) {
        view.reason = Some(reason.into());
        return view;
    }
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
    let Some(field) = entry.model.fields.get(&key).filter(|field| {
        matches!(
            field.kind,
            InputKind::User
                | InputKind::MultiUser { .. }
                | InputKind::MultiChannel { .. }
                | InputKind::MultiConversation { .. }
                | InputKind::Conversation
                | InputKind::Channel
        )
    }) else {
        return view;
    };
    view.placeholder = field.placeholder.clone().unwrap_or(view.placeholder);
    if matches!(
        field.kind,
        InputKind::Conversation | InputKind::MultiConversation { .. }
    ) && filter(shell, source, metadata)
        .is_some_and(|f| f.only_from_team_id.is_some() && f.enterprise_id.is_some())
    {
        view.reason=Some("Workspace-restricted conversation selection is unavailable for this Enterprise Grid account.".into());
        return view;
    }

    view.revision = entry.revision;
    if entry.controls.get(&key) != Some(metadata) {
        return view;
    }
    let status = super::super::operation_status(shell, source, metadata);
    view.enabled = status.enabled;
    view.busy = status.busy;
    view.reason = status.reason;
    view
}

pub(crate) mod conversations;
mod multiple;
pub(crate) use multiple::{display_selected_users, remove_user, submit_users};

pub(crate) fn is_multiple(metadata: &BlockElementMetadata) -> bool {
    matches!(&metadata.source,super_platinum_core::block_actions::BlockElementSource::UnsupportedControl {kind} if matches!(kind.as_str(), "multi_users_select" | "multi_channels_select" | "multi_conversations_select"))
}

pub(crate) mod directory;

pub(crate) fn is_channel(metadata: &BlockElementMetadata) -> bool {
    matches!(&metadata.source,super_platinum_core::block_actions::BlockElementSource::UnsupportedControl { kind } if matches!(kind.as_str(), "channels_select" | "multi_channels_select"))
}
pub(crate) fn is_conversation(metadata: &BlockElementMetadata) -> bool {
    matches!(&metadata.source,super_platinum_core::block_actions::BlockElementSource::UnsupportedControl { kind } if matches!(kind.as_str(), "conversations_select" | "multi_conversations_select"))
}
fn filter(
    shell: &ShellState,
    source: &Source,
    metadata: &BlockElementMetadata,
) -> Option<conversations::Filter> {
    if is_channel(metadata) {
        return Some(super::channel::filter(shell, &source.owner().team_id));
    }
    if !is_conversation(metadata) {
        return None;
    }
    let entry = shell
        .message_buttons
        .inputs
        .get(&ContainerKey::new(source))?;
    let field = entry.model.fields.get(&field_key(metadata)?)?;
    conversations::workspace_filter(shell, &source.owner().team_id, &field.raw)
}
pub(crate) use directory::search;

pub(crate) struct Picker {
    source: Source,
    multiple: Option<multiple::Selection>,
    context: Option<crate::scheduled::Context>,
    filter: Option<conversations::Filter>,
    metadata: BlockElementMetadata,
    mounted: display::Mounted,
    display: display::Action,
    input_revision: u64,
    revision: u64,
    query_revision: u64,
    query: String,
    candidates: Vec<directory::Choice>,
    loading: bool,
    agent_unavailable: bool,
    error: Option<String>,
}
pub(crate) struct PickerPresentation {
    pub conversation: bool,
    pub channel: bool,
    pub multiple: bool,
    pub selected: Vec<(String, String)>,
    pub selection_error: Option<String>,
    pub revision: u64,
    pub query_revision: u64,
    pub query: String,
    pub candidates: Vec<directory::Choice>,
    pub loading: bool,
    pub agent_unavailable: bool,
    pub error: Option<String>,
    pub placeholder: String,
}
pub(crate) fn open(
    mut state: Signal<ShellState>,
    source: Source,
    metadata: BlockElementMetadata,
    mounted: display::Mounted,
) {
    ensure(state, source.clone());
    let mut shell = state.write();
    if shell.overlay.is_some() || !presentation(&shell, &source, &metadata).enabled {
        return;
    }
    let Some(key) = field_key(&metadata) else {
        return;
    };
    let multiple = multiple::selection(&shell, &source, &metadata, &mounted);
    let Some(display) = mounted.capture(&ContainerKey::new(&source), &key) else {
        return;
    };
    let Some(input_revision) = revision(&shell, &source) else {
        return;
    };
    let filter = filter(&shell, &source, &metadata);
    let candidates = if let Some(filter) = &filter {
        conversations::cached(&shell, &source, filter, "")
    } else {
        directory::cached(&shell, &source, "")
    };
    let context = crate::scheduled::Context::capture(&shell);
    let agent_unavailable = filter.as_ref().is_none_or(|f| f.allows("im"))
        && shell
            .core
            .workspaces
            .get(&source.owner().team_id)
            .is_some_and(|w| {
                w.users.values().any(|u| {
                    u.extra.get("is_agentforce_bot") == Some(&serde_json::Value::Bool(true))
                })
            });
    shell.message_buttons.user_picker = Some(Picker {
        source,
        multiple,
        context,
        filter,
        metadata,
        mounted,
        display,
        input_revision,
        revision: next_id(),
        query_revision: next_id(),
        query: String::new(),
        candidates,
        loading: false,
        agent_unavailable,
        error: None,
    });
    shell.overlay = Some(Overlay::MessageUserSelect);
}
fn current(shell: &ShellState, picker: &Picker) -> bool {
    shell.overlay == Some(Overlay::MessageUserSelect)
        && picker
            .context
            .as_ref()
            .map_or(shell.core.transport.is_none(), |ctx| {
                directory::context_current(shell, ctx)
            })
        && picker.display.current()
        && picker.source.current(shell)
        && picker.filter == filter(shell, &picker.source, &picker.metadata)
        && revision(shell, &picker.source) == Some(picker.input_revision)
        && presentation(shell, &picker.source, &picker.metadata).enabled
}
pub(crate) fn picker(shell: &ShellState) -> Option<PickerPresentation> {
    let picker = shell.message_buttons.user_picker.as_ref()?;
    if !current(shell, picker) {
        return None;
    }
    Some(PickerPresentation {
        conversation: picker.filter.is_some(),
        channel: is_channel(&picker.metadata),
        multiple: picker.multiple.is_some(),
        selected: picker
            .multiple
            .as_ref()
            .map(|m| {
                m.selected
                    .iter()
                    .map(|id| (id.clone(), label(shell, &picker.source, id)))
                    .collect()
            })
            .unwrap_or_default(),
        selection_error: picker
            .multiple
            .as_ref()
            .and_then(multiple::error)
            .or_else(|| multiple::channel_error(shell, picker)),
        revision: picker.revision,
        query_revision: picker.query_revision,
        query: picker.query.clone(),
        candidates: picker.candidates.clone(),
        loading: picker.loading,
        agent_unavailable: picker.agent_unavailable,
        error: picker.error.clone(),
        placeholder: presentation(shell, &picker.source, &picker.metadata).placeholder,
    })
}
pub(crate) fn choose(
    mut state: Signal<ShellState>,
    expected: u64,
    query_revision: u64,
    id: String,
) {
    let choice = {
        let mut shell = state.write();
        let Some(picker) = shell.message_buttons.user_picker.as_ref() else {
            return;
        };
        if picker.revision != expected
            || picker.query_revision != query_revision
            || !current(&shell, picker)
            || !picker.candidates.iter().any(|c| c.id == id)
        {
            return;
        }
        let Some(workspace) = shell.core.workspaces.get(&picker.source.owner().team_id) else {
            return;
        };
        if picker.filter.as_ref().map_or_else(
            || {
                workspace
                    .users
                    .get(&id)
                    .is_some_and(|u| !directory::eligible(workspace, u))
            },
            |filter| {
                !conversations::choice_current(
                    workspace,
                    filter,
                    picker.candidates.iter().find(|c| c.id == id).unwrap(),
                )
            },
        ) {
            return;
        }
        let label = picker
            .candidates
            .iter()
            .find(|c| c.id == id)
            .unwrap()
            .label
            .clone();
        let channel_choice = (is_channel(&picker.metadata) || is_conversation(&picker.metadata))
            .then(|| {
                picker
                    .candidates
                    .iter()
                    .find(|c| c.id == id)
                    .unwrap()
                    .clone()
            });
        let owner = picker.source.owner();
        let label_key = (
            owner.account_epoch,
            owner.team_id.clone(),
            owner.self_user_id.clone(),
            id.clone(),
        );
        shell
            .message_buttons
            .user_labels
            .retain(|(epoch, _, _, _), _| *epoch == label_key.0);
        if shell.message_buttons.user_labels.len() >= 512
            && !shell.message_buttons.user_labels.contains_key(&label_key)
        {
            shell.message_buttons.user_labels.pop_first();
        }
        shell.message_buttons.user_labels.insert(label_key, label);
        if let Some(selection) = shell
            .message_buttons
            .user_picker
            .as_mut()
            .and_then(|p| p.multiple.as_mut())
        {
            if let Some(choice) = channel_choice {
                selection.proofs.insert(id.clone(), choice);
            }
            multiple::toggle(selection, id);
            return;
        }
        let picker = shell.message_buttons.user_picker.take().unwrap();
        shell.overlay = None;
        (
            picker.source,
            picker.metadata,
            picker.mounted,
            channel_choice,
        )
    };
    super::change_with_choice(state, choice.0, choice.1, Some(id), choice.2, choice.3);
}
pub(crate) fn cancel(mut state: Signal<ShellState>, expected: u64) {
    let mut shell = state.write();
    if shell
        .message_buttons
        .user_picker
        .as_ref()
        .is_some_and(|p| p.revision == expected)
    {
        cancel_local(&mut shell);
    }
}
pub(crate) fn cancel_local(shell: &mut ShellState) {
    shell.message_buttons.user_picker = None;
    if shell.overlay == Some(Overlay::MessageUserSelect) {
        shell.overlay = None;
    }
}
pub(in crate::message_buttons) fn poll(shell: &mut ShellState) -> bool {
    if shell
        .message_buttons
        .user_picker
        .as_ref()
        .is_some_and(|p| !current(shell, p))
    {
        cancel_local(shell);
        return true;
    }
    false
}

/// Resolve current cached profile first; remote-only labels remain scoped to the
/// original account/workspace/viewer and never become action wire content.
pub(crate) fn label(shell: &ShellState, source: &Source, id: &str) -> String {
    let owner = source.owner();
    if let Some(workspace) = shell.core.workspaces.get(&owner.team_id) {
        if let Some(channel) = workspace.channels.get(id) {
            return conversations::channel_label(workspace, channel);
        }
        if workspace.users.contains_key(id) {
            return workspace.display_name(id);
        }
    }
    shell
        .message_buttons
        .user_labels
        .get(&(
            owner.account_epoch,
            owner.team_id.clone(),
            owner.self_user_id.clone(),
            id.into(),
        ))
        .cloned()
        .unwrap_or_else(|| id.into())
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod conversation_tests;

#[cfg(test)]
mod channel_tests;
