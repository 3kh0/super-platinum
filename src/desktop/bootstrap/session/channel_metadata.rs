//! Ownership of the ordinary typed channel reads used by bootstrap.
//! This does not certify the source directory/conditional-fetch lifecycle.
use std::{collections::BTreeSet, sync::Arc};
use super_platinum_core::{
    CoreAppState,
    config::WorkspaceSession,
    slack::{
        Transport,
        models::{Channel, GenericInfoPage},
    },
};

pub(super) struct Owner {
    account: Option<String>,
    epoch: u64,
    transport: Arc<Transport>,
    session: WorkspaceSession,
    cookie: String,
    self_user: String,
}
impl Owner {
    pub(super) fn capture(
        core: &CoreAppState,
        transport: &Transport,
        session: &WorkspaceSession,
    ) -> Option<Self> {
        let workspace = core.workspaces.get(&session.team_id)?;
        if !session.user_id.is_empty() && workspace.self_user_id != session.user_id {
            return None;
        }
        let owner = Self {
            account: core.active_account.clone(),
            epoch: core.account_epoch,
            transport: core.transport.clone()?,
            session: session.clone(),
            cookie: core.session.as_ref()?.d_cookie.clone(),
            self_user: workspace.self_user_id.clone(),
        };
        (std::ptr::eq(owner.transport.as_ref(), transport) && owner.current(core)).then_some(owner)
    }
    pub(super) fn current(&self, core: &CoreAppState) -> bool {
        core.active_account == self.account
            && core.account_epoch == self.epoch
            && core
                .transport
                .as_ref()
                .is_some_and(|t| Arc::ptr_eq(t, &self.transport))
            && self.transport.d_cookie() == self.cookie
            && core.session.as_ref().is_some_and(|s| {
                s.d_cookie == self.cookie
                    && s.workspaces.get(&self.session.team_id).is_some_and(|w| {
                        w.team_id == self.session.team_id
                            && w.user_id == self.session.user_id
                            && w.enterprise_id == self.session.enterprise_id
                            && w.token == self.session.token
                            && w.url == self.session.url
                    })
            })
            && core.workspaces.get(&self.session.team_id).is_some_and(|w| {
                w.team_id == self.session.team_id && w.self_user_id == self.self_user
            })
    }
    pub(super) fn begin(
        &self,
        core: &CoreAppState,
        ids: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> Option<Read<'_>> {
        self.current(core).then(|| Read {
            owner: self,
            ids: ids.into_iter().map(|id| id.as_ref().to_owned()).collect(),
            revision: core.workspaces[&self.session.team_id]
                .channel_lifecycle
                .revision(),
        })
    }
}
pub(super) struct Read<'a> {
    owner: &'a Owner,
    ids: BTreeSet<String>,
    revision: u64,
}
impl Read<'_> {
    pub(super) fn accepts(&self, core: &CoreAppState, channels: &[Channel]) -> bool {
        let mut seen = BTreeSet::new();
        self.owner.current(core)
            && channels
                .iter()
                .all(|c| self.ids.contains(&c.id) && seen.insert(&c.id))
    }
    pub(super) fn refresh(&self, core: &mut CoreAppState, page: GenericInfoPage) -> bool {
        if !self.accepts(core, &page.channels) {
            return false;
        }
        core.workspaces
            .get_mut(&self.owner.session.team_id)
            .unwrap()
            .apply_channel_refresh_since(page, self.revision);
        true
    }
    pub(super) fn hydrate(&self, core: &mut CoreAppState, mut channels: Vec<Channel>) -> bool {
        if !self.accepts(core, &channels) {
            return false;
        }
        self.retry_hydration(core);
        core.channel_hydrated.extend(
            channels
                .iter()
                .filter(|c| c.name.as_deref().is_some_and(|name| !name.is_empty()))
                .map(|c| (self.owner.session.team_id.clone(), c.id.clone())),
        );
        let workspace = core
            .workspaces
            .get_mut(&self.owner.session.team_id)
            .unwrap();
        for channel in &mut channels {
            workspace.channel_lifecycle.overlay(channel, self.revision);
        }
        workspace.apply_channels_info(channels);
        true
    }
    /// Release this batch only while its account still owns the marker set.
    pub(super) fn retry_hydration(&self, core: &mut CoreAppState) {
        if self.owner.current(core) {
            for id in &self.ids {
                core.channel_hydrated
                    .remove(&(self.owner.session.team_id.clone(), id.clone()));
            }
        }
    }
}

#[cfg(test)]
mod tests;
