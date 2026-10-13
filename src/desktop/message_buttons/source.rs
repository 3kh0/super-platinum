//! Exact rendered-message ownership. Never search other bags or infer a channel
//! from whichever conversation happens to be active at click/confirmation time.
mod action;
mod revision;
use action::Action;
pub(crate) use action::Current;

use crate::state::{ChannelTab, MainView, MessageVm, ShellState};
use serde_json::{Value, json};
use std::sync::Arc;
use super_platinum_core::{
    block_actions::{
        BlockConfirmation, BlockConfirmationState, BlockElementMetadata, BlockElementSource,
        validation::{
            CapturedMessageButton, CapturedMessageRadio, CapturedMessageSelect, MessageActionError,
            MessageActionOwner,
        },
    },
    slack::models::Message,
    state::ChannelMessages,
};

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Bag {
    Timeline,
    Thread { root: String },
    Pins { item_key: String },
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Source {
    owner: MessageActionOwner,
    bag: Bag,
    channel_generation: u64,
    bag_generation: u64,
    realtime_generation: u64,
    main_view: MainView,
    channel_tab: ChannelTab,
    revision: Arc<Value>,
    wire_revision: Arc<Value>,
    service: Option<super::service::Service>,
}

#[derive(Clone)]
pub(crate) struct Captured {
    source: Source,
    action: Action,
    metadata: BlockElementMetadata,
    url: Option<String>,
    service: super::service::Service,
    display: Option<super::inputs::display::Action>,
}

impl Source {
    pub(crate) fn capture(
        shell: &ShellState,
        row: &MessageVm,
        channel_id: &str,
        bag: Bag,
    ) -> Result<Self, MessageActionError> {
        if row.pending || row.id != row.ts || channel_id.trim().is_empty() {
            return Err(MessageActionError::InvalidOwner);
        }
        let team = shell
            .core
            .active_team
            .as_ref()
            .ok_or(MessageActionError::InvalidOwner)?;
        let workspace = shell
            .core
            .workspaces
            .get(team)
            .ok_or(MessageActionError::InvalidOwner)?;
        let bag_generation = match &bag {
            Bag::Timeline => shell.channel_generation,
            Bag::Thread { .. } => shell.thread_generation,
            Bag::Pins { .. } => shell.core.pins.generation,
        };
        let mut source = Self {
            owner: MessageActionOwner {
                account_epoch: shell.core.account_epoch,
                self_user_id: workspace.self_user_id.clone(),
                team_id: team.clone(),
                channel_id: channel_id.into(),
                message_ts: row.ts.clone(),
            },
            bag,
            channel_generation: shell.channel_generation,
            bag_generation,
            realtime_generation: workspace.rt_generation,
            main_view: shell.main_view,
            channel_tab: shell.channel_tab,
            revision: Arc::new(Value::Null),
            wire_revision: Arc::new(Value::Null),
            service: None,
        };
        let message = source.resolve(shell, true)?;
        source.revision = Arc::new(revision::capture(message));
        source.service = super::service::resolve(workspace, message).ok();
        source.wire_revision = Arc::new(wire_revision(message, source.service.as_ref()));
        Ok(source)
    }

    pub(crate) fn owner(&self) -> &MessageActionOwner {
        &self.owner
    }

    /// Compare an earlier operation to a freshly validated rendered candidate.
    /// The original bag may have been evicted; that alone must never authorize
    /// repeating a write whose acknowledgement is still unknown.
    pub(crate) fn same_action_revision(&self, other: &Self) -> bool {
        self.owner == other.owner && self.wire_revision == other.wire_revision
    }

    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        self.resolve(shell, true).is_ok_and(|message| {
            revision::matches(message, self.revision.as_ref())
                && self.service_current(shell, message)
        })
    }

    pub(crate) fn message<'a>(
        &self,
        shell: &'a ShellState,
    ) -> Result<&'a Message, MessageActionError> {
        let message = self.resolve(shell, true)?;
        if !revision::matches(message, self.revision.as_ref())
            || !self.service_current(shell, message)
        {
            return Err(MessageActionError::StaleMessage);
        }
        Ok(message)
    }

    /// An uncertain write belongs to the canonical action, not its visit. A
    /// navigation/reconnect must not make that same action retryable.
    pub(crate) fn revision_current(&self, shell: &ShellState) -> bool {
        self.resolve(shell, false).is_ok_and(|message| {
            revision::matches(message, self.revision.as_ref())
                && self.service_current(shell, message)
        })
    }

    pub(crate) fn capture_button(
        &self,
        shell: &ShellState,
        block_id: &str,
        action_id: &str,
    ) -> Result<Captured, MessageActionError> {
        let message = self.resolve(shell, true)?;
        if !revision::matches(message, self.revision.as_ref()) {
            return Err(MessageActionError::StaleMessage);
        }
        let service =
            super::service::resolve(&shell.core.workspaces[&self.owner.team_id], message)?;
        if self.service.as_ref() != Some(&service) {
            return Err(MessageActionError::StaleMessage);
        }
        super_platinum_core::block_actions::message_state::MessageInputs::parse(message)
            .map_err(|_| MessageActionError::UnsupportedButton)?;
        let button =
            CapturedMessageButton::capture(self.owner.clone(), message, block_id, action_id)?;
        // Core capture above proves this immediate block/action match is unique
        // and that confirmation cannot be missing or malformed accidentally.
        let raw =
            raw_button(message, block_id, action_id).ok_or(MessageActionError::MissingButton)?;
        let metadata =
            crate::blocks::action::metadata(raw, Some(block_id), BlockElementSource::Button);
        Ok(Captured {
            source: self.clone(),
            action: Action::Button(button),
            display: None,
            metadata,
            url: raw.get("url").and_then(Value::as_str).map(str::to_owned),
            service,
        })
    }

    /// Eligibility for an already parsed input model, without capturing an
    /// action or cloning the message/options. Resolve raw data canonically here.
    pub(crate) fn select_metadata(
        &self,
        shell: &ShellState,
        block_id: &str,
        action_id: &str,
    ) -> Result<BlockElementMetadata, MessageActionError> {
        let message = self.message(shell)?;
        super::service::resolve(&shell.core.workspaces[&self.owner.team_id], message)?;
        let raw =
            CapturedMessageSelect::validate_element(&self.owner, message, block_id, action_id)?;
        Ok(crate::blocks::action::metadata(
            raw,
            Some(block_id),
            BlockElementSource::UnsupportedControl {
                kind: "static_select".into(),
            },
        ))
    }

    pub(crate) fn capture_select(
        &self,
        shell: &ShellState,
        block_id: &str,
        action_id: &str,
        selected: Option<&str>,
    ) -> Result<Captured, MessageActionError> {
        let message = self.message(shell)?;
        let service =
            super::service::resolve(&shell.core.workspaces[&self.owner.team_id], message)?;
        let inputs =
            super_platinum_core::block_actions::message_state::MessageInputs::parse(message)
                .map_err(|_| MessageActionError::UnsupportedButton)?;
        let key = super_platinum_core::block_actions::form::InputKey {
            block_id: block_id.into(),
            action_id: action_id.into(),
        };
        let field = inputs
            .fields
            .get(&key)
            .ok_or(MessageActionError::MissingButton)?;
        let super_platinum_core::block_actions::form::InputKind::StaticSelect {
            options,
            multiple: false,
            ..
        } = &field.kind
        else {
            return Err(MessageActionError::UnsupportedButton);
        };
        let option = selected
            .map(|value| {
                options
                    .iter()
                    .find(|option| option.value == value)
                    .map(|option| &option.raw)
                    .ok_or(MessageActionError::MalformedButton)
            })
            .transpose()?;
        let select = CapturedMessageSelect::capture(
            self.owner.clone(),
            message,
            block_id,
            action_id,
            option,
        )?;
        let raw =
            raw_button(message, block_id, action_id).ok_or(MessageActionError::MissingButton)?;
        let metadata = crate::blocks::action::metadata(
            raw,
            Some(block_id),
            BlockElementSource::UnsupportedControl {
                kind: "static_select".into(),
            },
        );
        Ok(Captured {
            source: self.clone(),
            action: Action::Select(select),
            display: None,
            metadata,
            url: None,
            service,
        })
    }

    /// Eligibility for an already parsed input model, without capturing an
    /// action or cloning the message/options. Resolve raw data canonically here.
    pub(crate) fn radio_metadata(
        &self,
        shell: &ShellState,
        block_id: &str,
        action_id: &str,
    ) -> Result<BlockElementMetadata, MessageActionError> {
        let message = self.message(shell)?;
        super::service::resolve(&shell.core.workspaces[&self.owner.team_id], message)?;
        let raw =
            CapturedMessageRadio::validate_element(&self.owner, message, block_id, action_id)?;
        Ok(crate::blocks::action::metadata(
            raw,
            Some(block_id),
            BlockElementSource::UnsupportedControl {
                kind: "radio_buttons".into(),
            },
        ))
    }

    pub(crate) fn capture_radio(
        &self,
        shell: &ShellState,
        block_id: &str,
        action_id: &str,
        selected: Option<&str>,
    ) -> Result<Captured, MessageActionError> {
        let message = self.message(shell)?;
        let service =
            super::service::resolve(&shell.core.workspaces[&self.owner.team_id], message)?;
        let inputs =
            super_platinum_core::block_actions::message_state::MessageInputs::parse(message)
                .map_err(|_| MessageActionError::UnsupportedButton)?;
        let key = super_platinum_core::block_actions::form::InputKey {
            block_id: block_id.into(),
            action_id: action_id.into(),
        };
        let field = inputs
            .fields
            .get(&key)
            .ok_or(MessageActionError::MissingButton)?;
        let super_platinum_core::block_actions::form::InputKind::Radio { options } = &field.kind
        else {
            return Err(MessageActionError::UnsupportedButton);
        };
        let option = selected
            .map(|value| {
                options
                    .iter()
                    .find(|option| option.value == value)
                    .map(|option| &option.raw)
                    .ok_or(MessageActionError::MalformedButton)
            })
            .transpose()?;
        let select = CapturedMessageRadio::capture(
            self.owner.clone(),
            message,
            block_id,
            action_id,
            option,
        )?;
        let raw =
            raw_button(message, block_id, action_id).ok_or(MessageActionError::MissingButton)?;
        let metadata = crate::blocks::action::metadata(
            raw,
            Some(block_id),
            BlockElementSource::UnsupportedControl {
                kind: "radio_buttons".into(),
            },
        );
        Ok(Captured {
            source: self.clone(),
            action: Action::Radio(select),
            display: None,
            metadata,
            url: None,
            service,
        })
    }

    fn service_current(&self, shell: &ShellState, message: &Message) -> bool {
        super::service::resolve(&shell.core.workspaces[&self.owner.team_id], message).ok()
            == self.service
    }

    /// Lookup metadata only for this still-current canonical message. No request
    /// or cache mutation occurs during rendering; the button effect owns loading.
    pub(crate) fn lookup_target(&self, shell: &ShellState) -> Option<(String, Option<String>)> {
        let message = self.resolve(shell, true).ok()?;
        if !revision::matches(message, self.revision.as_ref()) {
            return None;
        }
        super_platinum_core::block_actions::message_state::MessageInputs::parse(message).ok()?;
        super::service::lookup_target(&shell.core.workspaces[&self.owner.team_id], message)
    }

    fn resolve<'a>(
        &self,
        shell: &'a ShellState,
        visible: bool,
    ) -> Result<&'a Message, MessageActionError> {
        let invalid = MessageActionError::WrongMessage;
        let owner = &self.owner;
        let workspace = shell.core.workspaces.get(&owner.team_id).ok_or(invalid)?;
        if shell.core.account_epoch != owner.account_epoch
            || shell.core.active_team.as_deref() != Some(owner.team_id.as_str())
            || workspace.team_id != owner.team_id
            || workspace.self_user_id != owner.self_user_id
            || owner.self_user_id.trim().is_empty()
            || (visible
                && (shell.core.active_channel.as_deref() != Some(owner.channel_id.as_str())
                    || workspace.rt_generation != self.realtime_generation
                    || shell.channel_generation != self.channel_generation
                    || shell.main_view != self.main_view
                    || shell.channel_tab != self.channel_tab))
        {
            return Err(invalid);
        }
        let message = match &self.bag {
            Bag::Timeline => {
                if visible
                    && (self.channel_tab != ChannelTab::Messages
                        || self.bag_generation != shell.channel_generation)
                {
                    return Err(invalid);
                }
                exact_message(
                    workspace.messages.get(&owner.channel_id).ok_or(invalid)?,
                    &owner.message_ts,
                )?
            }
            Bag::Thread { root } => {
                if visible
                    && (shell.thread_root.as_deref() != Some(root.as_str())
                        || shell.thread_generation != self.bag_generation)
                {
                    return Err(invalid);
                }
                let key = (
                    owner.team_id.clone(),
                    owner.channel_id.clone(),
                    root.clone(),
                );
                let message = exact_message(
                    shell.core.threads.get(&key).ok_or(invalid)?,
                    &owner.message_ts,
                )?;
                if message
                    .thread_ts
                    .as_deref()
                    .is_some_and(|value| value != root)
                {
                    return Err(invalid);
                }
                message
            }
            Bag::Pins { item_key } => {
                let pins = &shell.core.pins;
                if (visible
                    && (self.channel_tab != ChannelTab::Pins
                        || pins.generation != self.bag_generation))
                    || !pins.matches_scope(owner.account_epoch, &owner.team_id)
                    || pins.channel.as_deref() != Some(owner.channel_id.as_str())
                {
                    return Err(invalid);
                }
                let item = pins.items.get(item_key).ok_or(invalid)?;
                if item.kind != "message"
                    || item.key().as_ref() != Some(item_key)
                    || item
                        .channel
                        .as_deref()
                        .is_some_and(|channel| channel != owner.channel_id)
                {
                    return Err(invalid);
                }
                item.message.as_ref().ok_or(invalid)?
            }
        };
        if message.ts.as_deref() != Some(owner.message_ts.as_str())
            || message
                .channel
                .as_deref()
                .is_some_and(|channel| channel != owner.channel_id)
            || message.extra.get("deleted").and_then(Value::as_bool) == Some(true)
            || message.subtype.as_deref() == Some("message_deleted")
        {
            return Err(invalid);
        }
        Ok(message)
    }
}

impl Captured {
    pub(crate) fn set_display(&mut self, display: super::inputs::display::Action) {
        self.display = Some(display);
    }
    pub(crate) fn display_current(&self) -> bool {
        self.display
            .as_ref()
            .is_none_or(|display| display.current())
    }
    pub(crate) fn commit_display(&self, selected: Option<&str>) -> bool {
        self.display
            .as_ref()
            .is_none_or(|display| display.commit(selected))
    }
    pub(crate) fn selected_value(&self) -> Option<Option<&str>> {
        self.action.selected_value()
    }
    pub(crate) fn source(&self) -> &Source {
        &self.source
    }
    pub(crate) fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }
    pub(crate) fn app_id(&self) -> &str {
        &self.service.app_id
    }
    pub(crate) fn app_team_id(&self) -> Option<&str> {
        Some(&self.service.team_id)
    }
    pub(crate) fn metadata(&self) -> &BlockElementMetadata {
        &self.metadata
    }
    pub(crate) fn confirmation(&self) -> Option<&BlockConfirmation> {
        match &self.metadata.confirm {
            BlockConfirmationState::Parsed(confirm) => Some(confirm),
            _ => None,
        }
    }
    pub(crate) fn revalidate<'a>(
        &'a self,
        shell: &ShellState,
    ) -> Result<Current<'a>, MessageActionError> {
        let message = self.source.resolve(shell, true)?;
        if !revision::matches(message, self.source.revision.as_ref()) {
            return Err(MessageActionError::StaleMessage);
        }
        let service =
            super::service::resolve(&shell.core.workspaces[&self.source.owner.team_id], message)?;
        if service != self.service {
            return Err(MessageActionError::StaleMessage);
        }
        super_platinum_core::block_actions::message_state::MessageInputs::parse(message)
            .map_err(|_| MessageActionError::UnsupportedButton)?;
        self.action.revalidate(&self.source.owner, message)
    }
}

fn exact_message<'a>(
    bag: &'a ChannelMessages,
    ts: &str,
) -> Result<&'a Message, MessageActionError> {
    if bag.is_pending(ts) || bag.remote_deletions.contains(ts) {
        return Err(MessageActionError::WrongMessage);
    }
    let mut matches = bag
        .messages
        .iter()
        .filter(|message| message.ts.as_deref() == Some(ts));
    let message = matches.next().ok_or(MessageActionError::WrongMessage)?;
    if matches.next().is_some() {
        return Err(MessageActionError::WrongMessage);
    }
    Ok(message)
}

fn raw_button<'a>(message: &'a Message, block_id: &str, action_id: &str) -> Option<&'a Value> {
    let block = message
        .blocks
        .iter()
        .find(|block| block.get("block_id").and_then(Value::as_str) == Some(block_id))?;
    match block.get("type").and_then(Value::as_str)? {
        "actions" => block
            .get("elements")?
            .as_array()?
            .iter()
            .find(|button| button.get("action_id").and_then(Value::as_str) == Some(action_id)),
        "section" => block
            .get("accessory")
            .filter(|button| button.get("action_id").and_then(Value::as_str) == Some(action_id)),
        _ => None,
    }
}

fn wire_revision(message: &Message, service: Option<&super::service::Service>) -> Value {
    let ephemeral = message.extra.get("is_ephemeral").and_then(Value::as_bool) == Some(true);
    // Captured Emf2 uses message.bot_id as service ID, and bot_profile's app/team
    // for routing. Profile avatars/name/id and optional message.channel do not
    // change the wire action. A thread annotation matters only for ephemeral
    // containers; ordinary thread buttons still address the message timestamp.
    json!({
        "blocks": message.blocks,
        "bot_id": service.map(|s| s.service_id.as_str()).or(message.bot_id.as_deref()),
        "app_id": service.map(|s| s.app_id.as_str()),
        "app_team_id": service.map(|s| s.team_id.as_str()),
        "function_execution_id": message.extra.get("function_execution_id"),
        "is_ephemeral": ephemeral,
        "ephemeral_thread_ts": if ephemeral { message.thread_ts.as_deref() } else { None },
    })
}

#[cfg(test)]
mod tests;
