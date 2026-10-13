//! One checked desktop action facade for button and static-select dispatch.
use serde_json::Value;
use super_platinum_core::{
    block_actions::validation::{
        CapturedMessageButton, CapturedMessageRadio, CapturedMessageSelect, CurrentMessageButton,
        CurrentMessageRadio, CurrentMessageSelect, MessageActionError, MessageActionOwner,
    },
    config::WorkspaceSession,
    slack::{Error, SlackClient, api, client::PreparedRequest, models::Message},
};

#[derive(Clone)]
pub(super) enum Action {
    Button(CapturedMessageButton),
    Select(CapturedMessageSelect),
    Radio(CapturedMessageRadio),
}

pub(crate) enum Current<'a> {
    Button(CurrentMessageButton<'a>),
    Select(CurrentMessageSelect<'a>),
    Radio(CurrentMessageRadio<'a>),
}

impl Action {
    pub(super) fn selected_value(&self) -> Option<Option<&str>> {
        match self {
            Self::Button(_) => None,
            Self::Radio(radio) => Some(
                radio
                    .selected_option()
                    .and_then(|option| option.get("value"))
                    .and_then(Value::as_str),
            ),
            Self::Select(select) => Some(
                select
                    .selected_option()
                    .and_then(|option| option.get("value"))
                    .and_then(Value::as_str),
            ),
        }
    }
    pub(super) fn revalidate<'a>(
        &'a self,
        owner: &MessageActionOwner,
        message: &Message,
    ) -> Result<Current<'a>, MessageActionError> {
        match self {
            Self::Button(button) => button.revalidate(owner, message).map(Current::Button),
            Self::Select(select) => select.revalidate(owner, message).map(Current::Select),
            Self::Radio(radio) => radio.revalidate(owner, message).map(Current::Radio),
        }
    }
}

impl Current<'_> {
    pub(crate) fn action(&self) -> &Value {
        match self {
            Self::Button(value) => value.action(),
            Self::Select(value) => value.action(),
            Self::Radio(value) => value.action(),
        }
    }
    pub(crate) fn container(&self) -> Value {
        match self {
            Self::Button(value) => value.container(),
            Self::Select(value) => value.container(),
            Self::Radio(value) => value.container(),
        }
    }
    #[cfg(test)]
    pub(crate) fn service_id(&self) -> &str {
        match self {
            Self::Button(value) => value.service_id(),
            Self::Select(value) => value.service_id(),
            Self::Radio(value) => value.service_id(),
        }
    }
    #[cfg(test)]
    pub(crate) fn owner(&self) -> &MessageActionOwner {
        match self {
            Self::Button(value) => value.owner(),
            Self::Select(value) => value.owner(),
            Self::Radio(value) => value.owner(),
        }
    }
    #[cfg(test)]
    pub(crate) fn function_execution_id(&self) -> Option<&str> {
        match self {
            Self::Button(value) => value.function_execution_id(),
            Self::Select(value) => value.function_execution_id(),
            Self::Radio(value) => value.function_execution_id(),
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare(
        &self,
        client: &SlackClient,
        workspace: &WorkspaceSession,
        app_id: &str,
        service_team_id: &str,
        client_token: &str,
        state: Option<&Value>,
    ) -> Result<PreparedRequest, Error> {
        match self {
            Self::Button(button) => api::block_action_button(
                client,
                workspace,
                button,
                app_id,
                service_team_id,
                client_token,
                state,
            ),
            Self::Radio(radio) => api::block_action_radio(
                client,
                workspace,
                radio,
                app_id,
                service_team_id,
                client_token,
                state,
            ),
            Self::Select(select) => api::block_action_select(
                client,
                workspace,
                select,
                app_id,
                service_team_id,
                client_token,
                state,
            ),
        }
    }
}
