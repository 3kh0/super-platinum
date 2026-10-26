//! A text callback is not a Button and does not capture a stale whole-form snapshot.
use super::*;
use crate::block_actions::form::home::{HomeForm, TextEvent};

#[derive(Clone, PartialEq)]
pub struct TextAction {
    user: String,
    view: View,
    event: TextEvent,
}
impl View {
    pub fn text_action(&self, event: &TextEvent, user: &str) -> Result<TextAction, Error> {
        if !id(user) || !event.same_view(&self.raw) {
            return Err(invalid());
        }
        // Revalidate source shape, not the event value against a later draft.
        let form = HomeForm::parse(&self.raw).map_err(|_| invalid())?;
        match event.action()["type"].as_str() {
            Some("plain_text_input") => form.text_field(event.key()),
            Some("number_input") => form.number_field(event.key()),
            Some("email_text_input") => form.email_field(event.key()),
            _ => return Err(invalid()),
        }
        .map_err(|_| invalid())?;
        Ok(TextAction {
            user: user.into(),
            view: self.clone(),
            event: event.clone(),
        })
    }
}
impl TextAction {
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
    /// Exact current account/view remains mandatory. Later ordinary edits are
    /// intentional: urb3 reads the current state when the debounce delivers.
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
mod tests;

#[cfg(test)]
mod number_tests;

#[cfg(test)]
mod email_tests;
