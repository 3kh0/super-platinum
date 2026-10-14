//! One checked desktop action facade for button and static-select dispatch.
use serde_json::Value;
use super_platinum_core::{
    block_actions::validation::{
        CapturedMessageButton, CapturedMessageChannel, CapturedMessageChannels,
        CapturedMessageConversation, CapturedMessageConversations, CapturedMessageDate,
        CapturedMessageMulti, CapturedMessageOverflow, CapturedMessageRadio, CapturedMessageSelect,
        CapturedMessageTime, CapturedMessageUser, CapturedMessageUsers, CurrentMessageButton,
        CurrentMessageChannel, CurrentMessageChannels, CurrentMessageConversation,
        CurrentMessageConversations, CurrentMessageDate, CurrentMessageMulti,
        CurrentMessageOverflow, CurrentMessageRadio, CurrentMessageSelect, CurrentMessageTime,
        CurrentMessageUser, CurrentMessageUsers, MessageActionError, MessageActionOwner,
    },
    config::WorkspaceSession,
    slack::{Error, SlackClient, api, client::PreparedRequest, models::Message},
};

#[derive(Clone)]
pub(super) enum Action {
    Conversations(
        CapturedMessageConversations,
        Vec<crate::message_buttons::inputs::user::directory::Choice>,
        crate::message_buttons::inputs::user::conversations::Filter,
    ),
    Channels(
        CapturedMessageChannels,
        Vec<crate::message_buttons::inputs::user::directory::Choice>,
        crate::message_buttons::inputs::user::conversations::Filter,
    ),
    Channel(
        CapturedMessageChannel,
        crate::message_buttons::inputs::user::directory::Choice,
    ),
    Conversation(CapturedMessageConversation),
    User(CapturedMessageUser),
    Users(CapturedMessageUsers),
    Overflow(CapturedMessageOverflow),
    Time(CapturedMessageTime),
    Date(CapturedMessageDate),
    Button(CapturedMessageButton),
    Select(CapturedMessageSelect),
    Radio(CapturedMessageRadio),
    Multi(CapturedMessageMulti),
}

pub(crate) enum Current<'a> {
    Conversations(CurrentMessageConversations<'a>),
    Channels(CurrentMessageChannels<'a>),
    Channel(CurrentMessageChannel<'a>),
    Conversation(CurrentMessageConversation<'a>),
    User(CurrentMessageUser<'a>),
    Users(CurrentMessageUsers<'a>),
    Overflow(CurrentMessageOverflow<'a>),
    Time(CurrentMessageTime<'a>),
    Date(CurrentMessageDate<'a>),
    Button(CurrentMessageButton<'a>),
    Select(CurrentMessageSelect<'a>),
    Radio(CurrentMessageRadio<'a>),
    Multi(CurrentMessageMulti<'a>),
}

impl Action {
    pub(super) fn selected_value(&self) -> Option<Option<&str>> {
        match self {
            Self::Conversations(_, _, _)
            | Self::Channels(_, _, _)
            | Self::Button(_)
            | Self::Multi(_)
            | Self::Users(_)
            | Self::Overflow(_) => None,
            Self::Channel(value, _) => Some(value.selected_channel()),
            Self::Conversation(value) => Some(value.selected_conversation()),
            Self::User(user) => Some(user.selected_user()),
            Self::Time(time) => Some(time.selected_time()),
            Self::Date(date) => Some(date.selected_date()),
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
    pub(super) fn selected_users(&self) -> Option<Vec<String>> {
        match self {
            Self::Users(users) => Some(users.selected_users()),
            _ => None,
        }
    }
    pub(super) fn selected_options(&self) -> Option<(Option<&[Value]>, &[Value])> {
        match self {
            Self::Multi(multi) => {
                Some((multi.selected_options_optional(), multi.retained_options()))
            }
            _ => None,
        }
    }
    pub(super) fn revalidate<'a>(
        &'a self,
        owner: &MessageActionOwner,
        message: &Message,
    ) -> Result<Current<'a>, MessageActionError> {
        match self {
            Self::Conversations(value, _, _) => {
                value.revalidate(owner, message).map(Current::Conversations)
            }
            Self::Channels(value, _, _) => value.revalidate(owner, message).map(Current::Channels),
            Self::Channel(value, _) => value.revalidate(owner, message).map(Current::Channel),
            Self::Conversation(value) => {
                value.revalidate(owner, message).map(Current::Conversation)
            }
            Self::User(user) => user.revalidate(owner, message).map(Current::User),
            Self::Users(users) => users.revalidate(owner, message).map(Current::Users),
            Self::Overflow(value) => value.revalidate(owner, message).map(Current::Overflow),
            Self::Time(time) => time.revalidate(owner, message).map(Current::Time),
            Self::Date(date) => date.revalidate(owner, message).map(Current::Date),
            Self::Button(button) => button.revalidate(owner, message).map(Current::Button),
            Self::Select(select) => select.revalidate(owner, message).map(Current::Select),
            Self::Radio(radio) => radio.revalidate(owner, message).map(Current::Radio),
            Self::Multi(multi) => multi.revalidate(owner, message).map(Current::Multi),
        }
    }
}

impl Current<'_> {
    pub(crate) fn action(&self) -> &Value {
        match self {
            Self::Conversations(value) => value.action(),
            Self::Channels(value) => value.action(),
            Self::Channel(value) => value.action(),
            Self::Conversation(value) => value.action(),
            Self::User(value) => value.action(),
            Self::Users(value) => value.action(),
            Self::Overflow(value) => value.action(),
            Self::Time(value) => value.action(),
            Self::Date(value) => value.action(),
            Self::Button(value) => value.action(),
            Self::Select(value) => value.action(),
            Self::Radio(value) => value.action(),
            Self::Multi(value) => value.action(),
        }
    }
    pub(crate) fn container(&self) -> Value {
        match self {
            Self::Conversations(value) => value.container(),
            Self::Channels(value) => value.container(),
            Self::Channel(value) => value.container(),
            Self::Conversation(value) => value.container(),
            Self::User(value) => value.container(),
            Self::Users(value) => value.container(),
            Self::Overflow(value) => value.container(),
            Self::Time(value) => value.container(),
            Self::Date(value) => value.container(),
            Self::Button(value) => value.container(),
            Self::Select(value) => value.container(),
            Self::Radio(value) => value.container(),
            Self::Multi(value) => value.container(),
        }
    }
    #[cfg(test)]
    pub(crate) fn service_id(&self) -> &str {
        match self {
            Self::Conversations(value) => value.service_id(),
            Self::Channels(value) => value.service_id(),
            Self::Channel(value) => value.service_id(),
            Self::Conversation(value) => value.service_id(),
            Self::User(value) => value.service_id(),
            Self::Users(value) => value.service_id(),
            Self::Overflow(value) => value.service_id(),
            Self::Time(value) => value.service_id(),
            Self::Date(value) => value.service_id(),
            Self::Button(value) => value.service_id(),
            Self::Select(value) => value.service_id(),
            Self::Radio(value) => value.service_id(),
            Self::Multi(value) => value.service_id(),
        }
    }
    #[cfg(test)]
    pub(crate) fn owner(&self) -> &MessageActionOwner {
        match self {
            Self::Conversations(value) => value.owner(),
            Self::Channels(value) => value.owner(),
            Self::Channel(value) => value.owner(),
            Self::Conversation(value) => value.owner(),
            Self::User(value) => value.owner(),
            Self::Users(value) => value.owner(),
            Self::Overflow(value) => value.owner(),
            Self::Time(value) => value.owner(),
            Self::Date(value) => value.owner(),
            Self::Button(value) => value.owner(),
            Self::Select(value) => value.owner(),
            Self::Radio(value) => value.owner(),
            Self::Multi(value) => value.owner(),
        }
    }
    #[cfg(test)]
    pub(crate) fn function_execution_id(&self) -> Option<&str> {
        match self {
            Self::Conversations(value) => value.function_execution_id(),
            Self::Channels(value) => value.function_execution_id(),
            Self::Channel(value) => value.function_execution_id(),
            Self::Conversation(value) => value.function_execution_id(),
            Self::User(value) => value.function_execution_id(),
            Self::Users(value) => value.function_execution_id(),
            Self::Overflow(value) => value.function_execution_id(),
            Self::Time(value) => value.function_execution_id(),
            Self::Date(value) => value.function_execution_id(),
            Self::Button(value) => value.function_execution_id(),
            Self::Select(value) => value.function_execution_id(),
            Self::Radio(value) => value.function_execution_id(),
            Self::Multi(value) => value.function_execution_id(),
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
            Self::Conversations(value) => api::block_action_conversations(
                client,
                workspace,
                value,
                app_id,
                service_team_id,
                client_token,
                state,
            ),
            Self::Channels(value) => api::block_action_channels(
                client,
                workspace,
                value,
                app_id,
                service_team_id,
                client_token,
                state,
            ),
            Self::Channel(user) => api::block_action_channel(
                client,
                workspace,
                user,
                app_id,
                service_team_id,
                client_token,
                state,
            ),
            Self::Conversation(user) => api::block_action_conversation(
                client,
                workspace,
                user,
                app_id,
                service_team_id,
                client_token,
                state,
            ),
            Self::Overflow(value) => api::block_action_overflow(
                client,
                workspace,
                value,
                app_id,
                service_team_id,
                client_token,
                state,
            ),
            Self::Users(users) => api::block_action_users(
                client,
                workspace,
                users,
                app_id,
                service_team_id,
                client_token,
                state,
            ),
            Self::User(user) => api::block_action_user(
                client,
                workspace,
                user,
                app_id,
                service_team_id,
                client_token,
                state,
            ),
            Self::Time(time) => api::block_action_time(
                client,
                workspace,
                time,
                app_id,
                service_team_id,
                client_token,
                state,
            ),
            Self::Date(date) => api::block_action_date(
                client,
                workspace,
                date,
                app_id,
                service_team_id,
                client_token,
                state,
            ),
            Self::Multi(multi) => api::block_action_multi(
                client,
                workspace,
                multi,
                app_id,
                service_team_id,
                client_token,
                state,
            ),
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
