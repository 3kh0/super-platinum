use super::*;
use crate::slack::{self, PreparedRequest, api, models::BootData};

/// One actual client.userBoot attempt. A fresh request may recover retired
/// posting authority; neither cached boots nor later picker config can bind it.
pub struct BootRead {
    scope: Arc<Scope>,
    epoch: Arc<()>,
    nonce: Arc<()>,
    previous_self: String,
    channel_revision: u64,
}
impl BootRead {
    pub fn begin(
        core: &mut CoreAppState,
        transport: &Arc<Transport>,
        session: &WorkspaceSession,
    ) -> Result<Self> {
        let scope = Arc::new(Scope::capture(core, &session.team_id)?);
        if !Arc::ptr_eq(transport, &scope.transport) || !same_credentials(session, &scope.session) {
            return Err(Unknown::Stale);
        }
        let workspace = core.workspaces.get_mut(&session.team_id).unwrap();
        let metadata = &mut workspace.posting_metadata;
        let nonce = Arc::new(());
        metadata.runtime.pending_boot = Some(nonce.clone());
        Ok(Self {
            scope,
            epoch: metadata.epoch.clone(),
            nonce,
            previous_self: workspace.self_user_id.clone(),
            channel_revision: workspace.channel_lifecycle.revision(),
        })
    }
    pub fn current(&self, core: &CoreAppState) -> bool {
        self.scope.current(core)
            && core
                .workspaces
                .get(&self.scope.session.team_id)
                .is_some_and(|w| {
                    w.self_user_id == self.previous_self
                        && w.channel_lifecycle.revision() == self.channel_revision
                        && Arc::ptr_eq(&w.posting_metadata.epoch, &self.epoch)
                        && w.posting_metadata
                            .runtime
                            .pending_boot
                            .as_ref()
                            .is_some_and(|n| Arc::ptr_eq(n, &self.nonce))
                })
    }
    pub fn request(&self, core: &CoreAppState) -> Result<PreparedRequest> {
        if !self.current(core) {
            return Err(Unknown::Stale);
        }
        Ok(api::user_boot(&core.client, &self.scope.session))
    }
    /// Release only this owned attempt. A failed read never fabricates fresh
    /// authority; caller retains the original Slack error and Retry-After.
    pub fn failed(self, core: &mut CoreAppState) -> bool {
        if !self.current(core) {
            return false;
        }
        core.workspaces
            .get_mut(&self.scope.session.team_id)
            .unwrap()
            .posting_metadata
            .runtime
            .pending_boot = None;
        true
    }
    /// Adopt an actual raw success. `apply_workspace` is true in normal boot;
    /// picker recovery uses false to refresh only posting metadata, preserving
    /// unrelated timeline/read state. Both paths validate the same owner.
    pub fn apply(
        self,
        core: &mut CoreAppState,
        raw: Value,
        apply_workspace: bool,
    ) -> std::result::Result<bool, slack::Error> {
        if !self.current(core) {
            return Ok(false);
        }
        let team = self.scope.session.team_id.clone();
        core.workspaces
            .get_mut(&team)
            .unwrap()
            .posting_metadata
            .runtime
            .pending_boot = None;
        if raw.get("ok").and_then(Value::as_bool) != Some(true) {
            return Err(slack::Error::Api(
                "client.userBoot unsuccessful response".into(),
            ));
        }
        let boot: BootData = serde_json::from_value(raw.clone())
            .map_err(|_| slack::Error::Api("client.userBoot invalid response".into()))?;
        // Empty legacy credential user IDs still allow ordinary app bootstrap.
        // They do not mint posting authority until a known identity is acquired
        // by the existing session owner and a fresh bound boot is requested.
        if boot.team.as_ref().map(|t| t.id.as_str()) != Some(&team)
            || !id(&boot.self_user.id)
            || (!self.scope.session.user_id.is_empty()
                && boot.self_user.id != self.scope.session.user_id)
        {
            return Err(slack::Error::Api(
                "client.userBoot identity mismatch".into(),
            ));
        }
        let mut metadata = Metadata::from_boot(&team, &boot.self_user.id, &boot);
        metadata.external = core.workspaces[&team].posting_metadata.external.clone();
        metadata.external.retire();
        if let Some(config) = &self.scope.configuration {
            metadata.runtime.bound = Some(self.scope.clone());
            let _ = metadata.external.bind(
                &team,
                &boot.self_user.id,
                super::super::external::top(config, &raw),
            );
            metadata
                .external
                .set_free(super::super::external::free(config, &raw));
            // Match BootData::all_channels ordering and first-ID precedence.
            for key in ["channels", "groups", "ims", "mpims"] {
                if let Some(channels) = raw.get(key).and_then(Value::as_array) {
                    for channel in channels {
                        if let Some(id) = channel
                            .get("id")
                            .and_then(Value::as_str)
                            .filter(|v| super::id(v))
                        {
                            if metadata.runtime.channels.contains_key(id) {
                                continue;
                            }
                            let _ = metadata.external.channel(channel);
                            metadata
                                .runtime
                                .channels
                                .insert(id.into(), (Arc::new(()), Some(Arc::new(channel.clone()))));
                            let _ = metadata.observe_channel(config, channel);
                        }
                    }
                }
            }
        }
        let workspace = core.workspaces.get_mut(&team).unwrap();
        if apply_workspace {
            workspace.apply_boot(boot);
        }
        workspace.posting_metadata = metadata;
        let ids: Vec<_> = workspace
            .posting_metadata
            .runtime
            .channels
            .keys()
            .cloned()
            .collect();
        for channel in ids {
            let stamp = Stamp::capture(core, &team, &channel);
            core.workspaces
                .get_mut(&team)
                .unwrap()
                .posting_metadata
                .runtime
                .stamps
                .insert(channel, stamp);
        }
        Ok(true)
    }
}
