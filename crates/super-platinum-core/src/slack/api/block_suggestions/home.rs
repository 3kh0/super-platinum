//! AppHomeBlocks owns a view route; it is never a modal or submit capability.
use super::*;
use crate::block_actions::{form::home::HomeForm, home::View};

impl SuggestionsContext {
    /// `view_context_team_id` is the caller's explicit Home context. The original
    /// gEn+ route uses a truthy override, otherwise the view's installed team.
    /// This does not establish desktop account/visit/query ownership.
    pub fn home(
        team_id: &str,
        self_user_id: &str,
        view_context_team_id: Option<&str>,
        view: &View,
        key: &InputKey,
    ) -> Result<Self, Error> {
        // View.home is public; revalidate the sealed raw payload against it.
        View::decode(&view.home, view.raw().clone())?;
        identity(&key.block_id)?;
        identity(&key.action_id)?;
        HomeForm::parse(view.raw())
            .and_then(|form| form.external_field(key).map(|_| ()))
            .map_err(|_| invalid())?;
        let service_team = view_context_team_id
            .filter(|value| !value.is_empty())
            .unwrap_or(&view.home.installed_team);
        Ok(Self {
            team: identity(team_id)?,
            user: identity(self_user_id)?,
            service_id: identity(view.raw()["bot_id"].as_str().ok_or_else(invalid)?)?,
            app_id: identity(&view.home.app)?,
            service_team_id: identity(service_team)?,
            function_execution_id: None,
            container: json!({"type":"view","view_id":identity(view.raw()["id"].as_str().ok_or_else(invalid)?)?}),
            key: key.clone(),
        })
    }
}

#[cfg(test)]
mod tests;
