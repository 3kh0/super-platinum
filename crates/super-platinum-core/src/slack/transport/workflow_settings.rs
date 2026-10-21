//! Only a closed settings Read can reach the ordered read executor here.
use super::*;
use crate::block_actions::{
    directory_endpoint::DirectoryEndpoint, workflow_management::settings_acquisition::Read,
};
use crate::{config::WorkspaceSession, slack::SlackClient};
use std::time::Instant;

impl Transport {
    pub async fn execute_workflow_settings_read(
        &self,
        read: &Read,
        client: &SlackClient,
        owner: &WorkspaceSession,
        endpoint: Option<&DirectoryEndpoint>,
        now: Instant,
        current: impl Fn() -> bool,
    ) -> Result<WorkflowSettingsResponse, Error> {
        if !current() {
            return Err(Error::Offline("Workflow settings owner retired".into()));
        }
        let req = read.request(client, owner, endpoint, now)?;
        let result = self
            .execute_profile_member_read_admitted(req, &current)
            .await;
        // Successful bytes alone do not survive retirement during the await.
        if !current() {
            return Err(Error::Offline("Workflow settings owner retired".into()));
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_actions::workflow_management::settings_acquisition::SearchKind;

    #[tokio::test]
    async fn retired_original_owner_cannot_dispatch_even_without_http_capability() {
        let transport = Transport {
            http: None,
            canvas_http: Arc::new(std::sync::OnceLock::new()),
            d_cookie: "synthetic".into(),
            health: Arc::new(watch::Sender::new(Health::Unknown)),
        };
        let owner = WorkspaceSession {
            team_id: "T00000001".into(),
            enterprise_id: None,
            user_id: "U00000001".into(),
            name: "Fixture".into(),
            url: "https://fixture.test".into(),
            token: "synthetic".into(),
        };
        let read = Read::new(&owner, SearchKind::InternalTeams, "", None).unwrap();
        let result = transport
            .execute_workflow_settings_read(
                &read,
                &SlackClient::default(),
                &owner,
                None,
                Instant::now(),
                || false,
            )
            .await;
        assert!(
            matches!(result, Err(Error::Offline(message)) if message == "Workflow settings owner retired")
        );
    }
}
