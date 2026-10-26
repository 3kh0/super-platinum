//! A sealed static selection occurrence, independent of later form edits.
use super::*;
use crate::block_actions::form::home::{HomeForm, StaticEvent};

#[derive(Clone, PartialEq)]
pub struct StaticAction {
    user: String,
    view: View,
    event: StaticEvent,
}
impl View {
    pub fn static_action(&self, event: &StaticEvent, user: &str) -> Result<StaticAction, Error> {
        if !id(user) || !event.same_view(&self.raw) {
            return Err(invalid());
        }
        let form = HomeForm::parse(&self.raw).map_err(|_| invalid())?;
        match event.action()["type"].as_str() {
            Some("multi_external_select") => form.external_field(event.key()).and_then(|f| {
                if matches!(
                    f.kind,
                    crate::block_actions::form::InputKind::MultiExternalSelect { .. }
                ) {
                    Ok(f)
                } else {
                    Err(crate::block_actions::form::FormError::ValueTypeMismatch)
                }
            }),
            Some("external_select") => form.external_field(event.key()).and_then(|f| {
                if matches!(
                    f.kind,
                    crate::block_actions::form::InputKind::ExternalSelect { .. }
                ) {
                    Ok(f)
                } else {
                    Err(crate::block_actions::form::FormError::ValueTypeMismatch)
                }
            }),
            Some("conversations_select") => form.conversation_field(event.key()).and_then(|f| {
                if matches!(f.kind, crate::block_actions::form::InputKind::Conversation) {
                    Ok(f)
                } else {
                    Err(crate::block_actions::form::FormError::ValueTypeMismatch)
                }
            }),
            Some("multi_conversations_select") => {
                form.conversation_field(event.key()).and_then(|f| {
                    if matches!(
                        f.kind,
                        crate::block_actions::form::InputKind::MultiConversation { .. }
                    ) {
                        Ok(f)
                    } else {
                        Err(crate::block_actions::form::FormError::ValueTypeMismatch)
                    }
                })
            }
            Some("multi_channels_select") => form.channel_field(event.key()).and_then(|f| {
                if matches!(
                    f.kind,
                    crate::block_actions::form::InputKind::MultiChannel { .. }
                ) {
                    Ok(f)
                } else {
                    Err(crate::block_actions::form::FormError::ValueTypeMismatch)
                }
            }),
            Some("channels_select") => form.channel_field(event.key()).and_then(|f| {
                if matches!(f.kind, crate::block_actions::form::InputKind::Channel) {
                    Ok(f)
                } else {
                    Err(crate::block_actions::form::FormError::ValueTypeMismatch)
                }
            }),
            Some("users_select") => form.user_field(event.key()).and_then(|f| {
                if matches!(f.kind, crate::block_actions::form::InputKind::User) {
                    Ok(f)
                } else {
                    Err(crate::block_actions::form::FormError::ValueTypeMismatch)
                }
            }),
            Some("multi_users_select") => form.user_field(event.key()).and_then(|f| {
                if matches!(
                    f.kind,
                    crate::block_actions::form::InputKind::MultiUser { .. }
                ) {
                    Ok(f)
                } else {
                    Err(crate::block_actions::form::FormError::ValueTypeMismatch)
                }
            }),
            Some("static_select") => form.static_field(event.key()),
            Some("multi_static_select") => form.multi_static_field(event.key()),
            Some("checkboxes") => form.checkbox_field(event.key()),
            Some("radio_buttons") => form.radio_field(event.key()),
            Some("datepicker") => form.date_field(event.key()),
            Some("timepicker") => form.time_field(event.key()),
            Some("datetimepicker") => form.datetime_field(event.key()),
            _ => return Err(invalid()),
        }
        .map_err(|_| invalid())?;
        Ok(StaticAction {
            user: user.into(),
            view: self.clone(),
            event: event.clone(),
        })
    }
}
impl StaticAction {
    pub fn action(&self) -> &Value {
        self.event.action()
    }
    pub fn raw(&self) -> &Value {
        &self.view.raw
    }
    pub fn app_id(&self) -> &str {
        &self.view.home.app
    }
    pub fn key(&self) -> (&str, &str) {
        (&self.event.key().block_id, &self.event.key().action_id)
    }
    pub fn same_view(&self, current: &View) -> bool {
        &self.view == current
    }
    /// urb3 reads the latest whole-form state when the occurrence is delivered;
    /// the action retains the exact option selected for that occurrence.
    pub fn prepare_with_context(
        &self,
        client: &SlackClient,
        workspace: &WorkspaceSession,
        current: &View,
        form: &HomeForm,
        token: &str,
        view_context_team_id: Option<&str>,
    ) -> Result<PreparedRequest, Error> {
        if workspace.user_id != self.user
            || !self.same_view(current)
            || form.raw_view() != current.raw()
            || token.is_empty()
            || token.len() > 255
            || token.chars().any(|c| c.is_whitespace() || c.is_control())
        {
            return Err(invalid());
        }
        let state = form.state().map_err(|_| invalid())?;
        let service_team = view_context_team_id
            .filter(|v| !v.is_empty())
            .unwrap_or(&self.view.home.installed_team);
        context::validate_identity(service_team)?;
        let mut fields = vec![
            ("app_id", self.view.home.app.clone()),
            ("service_team_id", service_team.into()),
            (
                "container",
                json!({"type":"view","view_id":self.view.home.view_id}).to_string(),
            ),
            ("actions", json!([self.event.action()]).to_string()),
            ("state", state.to_string()),
            ("client_token", token.into()),
            ("_x_reason", "dispatch_action_to_developer".into()),
        ];
        if let Some(bot) = optional(&self.view.raw, "bot_id")? {
            fields.push(("service_id", bot));
        }
        Ok(client.rest_form(workspace, "blocks.actions", fields))
    }
}

#[cfg(test)]
mod checkbox_tests;
#[cfg(test)]
mod date_tests;
#[cfg(test)]
mod radio_tests;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod time_tests;

#[cfg(test)]
mod datetime_tests;

#[cfg(test)]
mod user_tests;

#[cfg(test)]
mod multi_user_tests;

#[cfg(test)]
mod channel_tests;

#[cfg(test)]
mod multi_channel_tests;

#[cfg(test)]
mod conversation_tests;

#[cfg(test)]
mod multi_conversation_tests;

#[cfg(test)]
mod external_tests;
