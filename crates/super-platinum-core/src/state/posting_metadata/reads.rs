use super::*;
use crate::config::WorkspaceSession;
use crate::slack::client::{PreparedRequest, SlackClient};

#[derive(Clone, Debug)]
enum Kind {
    Features,
    Workspace(String),
}

/// One exact boot/read/configuration owner. The caller must additionally fence
/// its account epoch and Transport before request dispatch and result adoption.
/// This is a retry-safe read descriptor, not a posting or mutation capability.
#[derive(Clone, Debug)]
pub struct Read {
    epoch: Arc<()>,
    serial: Arc<()>,
    config: Configuration,
    kind: Kind,
    previous_role: Option<bool>,
}
impl Read {
    fn key(&self) -> &str {
        match &self.kind {
            Kind::Features => "features",
            Kind::Workspace(team) => team,
        }
    }

    pub fn current(&self, metadata: &Metadata, config: &Configuration) -> bool {
        config == &self.config
            && Arc::ptr_eq(&self.epoch, &metadata.epoch)
            && metadata
                .reads
                .get(self.key())
                .is_some_and(|serial| Arc::ptr_eq(serial, &self.serial))
            && metadata.check(config).is_ok()
    }

    pub fn request(
        &self,
        metadata: &Metadata,
        config: &Configuration,
        client: &SlackClient,
        workspace: &WorkspaceSession,
    ) -> Result<PreparedRequest> {
        if !self.current(metadata, config) {
            return Err(Unknown::Stale);
        }
        if workspace.team_id != config.team
            || workspace.user_id != config.user
            || workspace.enterprise_id != config.enterprise
        {
            return Err(Unknown::Identity);
        }
        Ok(match &self.kind {
            // IjwW / sQEb: POST features.access.policies.list, token only.
            Kind::Features => client.rest_form(workspace, "features.access.policies.list", vec![]),
            // ly/g: users.info in the selected org context, self + exact team.
            Kind::Workspace(team) => client.rest_form(
                workspace,
                "users.info",
                vec![("self", "true".into()), ("team", team.clone())],
            ),
        })
    }

    /// Consume the matching read even for malformed responses. Later callbacks
    /// and clones cannot overwrite a newer observation or reuse this receipt.
    pub fn apply(
        self,
        metadata: &mut Metadata,
        config: &Configuration,
        response: &Value,
    ) -> Result<()> {
        if !self.current(metadata, config) {
            return Err(Unknown::Stale);
        }
        metadata.reads.remove(self.key());
        if let Kind::Workspace(team) = &self.kind {
            // Preserve the last known comparison baseline even if parsing or
            // ownership validation below fails. Admission stays unknown until
            // a valid owned response replaces this role.
            metadata.invalid_workspace_roles.insert(team.clone());
        }
        if response.get("ok") != Some(&Value::Bool(true)) {
            return Err(Unknown::Shape);
        }
        match self.kind {
            Kind::Features => {
                let policies = response
                    .get("feature_policies")
                    .filter(|v| v.is_object())
                    .ok_or(Unknown::Shape)?;
                // A missing policy remains unknown. Do not copy the selector's
                // default false into an acquisition observation.
                metadata.manage_posting = policies
                    .get("manage_channel_posting__access")
                    .and_then(|v| v.get("is_enabled"))
                    .and_then(Value::as_bool);
                metadata.manage_posting.ok_or(Unknown::Shape)?;
            }
            Kind::Workspace(team) => {
                let user = response
                    .get("user")
                    .filter(|v| v.is_object())
                    .ok_or(Unknown::Shape)?;
                if user.get("id").and_then(Value::as_str) != Some(metadata.user_id.as_str()) {
                    return Err(Unknown::Identity);
                }
                let team_user = user
                    .get("team_user")
                    .filter(|v| v.is_object())
                    .ok_or(Unknown::Role)?;
                if team_user
                    .get("team_id")
                    .is_some_and(|v| v.as_str() != Some(team.as_str()))
                {
                    return Err(Unknown::Identity);
                }
                if team_user
                    .get("id")
                    .is_some_and(|v| v.as_str() != Some(metadata.user_id.as_str()))
                {
                    return Err(Unknown::Identity);
                }
                let admin = flag(team_user, "is_admin")?;
                // Validate the complete three-role response even though posting
                // uses isWorkspaceAdmin; owners do not imply it in zEyr.
                flag(team_user, "is_owner")?;
                flag(team_user, "is_primary_owner")?;
                if self.previous_role.is_some_and(|previous| previous != admin) {
                    // Learning a previously unknown role is not a change event.
                    // A changed known role retires older server flags too.
                    metadata.retire_channel_facts();
                }
                metadata.invalid_workspace_roles.remove(&team);
                metadata.workspace_admin.insert(team, admin);
            }
        }
        Ok(())
    }
}

impl Metadata {
    pub fn begin_feature_read(&mut self, config: &Configuration) -> Result<Read> {
        self.check(config)?;
        self.manage_posting = None;
        Ok(self.begin(config, Kind::Features))
    }

    /// Only a currently relevant target may be fetched. This does not create
    /// membership: the target must also be in the complete boot workspaces.
    pub fn begin_workspace_role_read(
        &mut self,
        config: &Configuration,
        channel: &Value,
        target: &str,
    ) -> Result<Read> {
        self.check(config)?;
        if config.unified != Some(true)
            || config.enterprise.is_none()
            || !self
                .relevant_workspaces(config, channel)?
                .iter()
                .any(|v| v == target)
            || !self.workspaces.contains_key(target)
            || !target.starts_with('T')
        {
            return Err(Unknown::Membership);
        }
        // Preserve the last known role as the comparison baseline across
        // superseded reads. channel_admin treats the pending key as unknown.
        // ly/g/fS6z update role stores, not separate server flag inventories.
        self.retire_derived_flags();
        Ok(self.begin(config, Kind::Workspace(target.into())))
    }

    fn begin(&mut self, config: &Configuration, kind: Kind) -> Read {
        let previous_role = match &kind {
            Kind::Workspace(team) => self.workspace_admin.get(team).copied(),
            Kind::Features => None,
        };
        let read = Read {
            epoch: self.epoch.clone(),
            serial: Arc::new(()),
            config: config.clone(),
            kind,
            previous_role,
        };
        self.reads.insert(read.key().into(), read.serial.clone());
        read
    }
}
