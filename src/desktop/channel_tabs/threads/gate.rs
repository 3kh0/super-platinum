//! Original channel/assignment/post-boot observations; never tab-row defaults.
use super::*;
use crate::app_home::channel_fetches::{self, ChannelThreads, canonical::Canonical};
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{SlackClient, Transport},
};

pub(super) struct Gate {
    pub account: String,
    pub team: String,
    pub channel: String,
    pub base: Inventory,
    pub assignment: ChannelThreads,
    pub canonical: Arc<Canonical>,
    pub session: WorkspaceSession,
    pub transport: Arc<Transport>,
    pub client: SlackClient,
    main: crate::state::MainView,
    pub(super) owner: Arc<crate::bootstrap::ReadOwner>,
}
/// Source getChannelType precedence over original accepted raw fields. Positive
/// joined membership avoids the unrelated unknown-file-channel exception.
pub(super) fn ordinary_joined(row: &Value, id: &str) -> bool {
    row["id"] == id
        && row["is_member"] == true
        && !truth(row.get("isNonExistent"))
        && !truth(row.get("is_im"))
        && !truth(row.get("is_mpim"))
        && (truth(row.get("is_private"))
            || truth(row.get("is_group"))
            || truth(row.get("is_channel")))
}
fn truth(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null | Value::Bool(false)) => false,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|n| n != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        _ => true,
    }
}
impl Gate {
    pub fn capture(shell: &ShellState, base: &Inventory) -> Option<Arc<Self>> {
        if crate::fixture::is_fixture()
            || shell.core.screen != super_platinum_core::state::Screen::Main
            || shell.loading
            || !shell.signed_in
            || shell.overlay.is_some()
            || !shell.conversation_visible()
        {
            return None;
        }
        let team = shell.core.active_team.clone()?;
        let channel = shell.core.active_channel.clone()?;
        if base.channel() != channel {
            return None;
        }
        let canonical = channel_fetches::canonical::observed(shell, &team).ok()?;
        let raw = canonical.row(&channel)?;
        if !ordinary_joined(raw, &channel)
            || Inventory::from_channel(raw).ok().as_ref() != Some(base)
        {
            return None;
        }
        let assignment = channel_fetches::channel_threads(shell, &team).ok()?;
        let session = shell.core.session.as_ref()?.workspaces.get(&team)?.clone();
        let transport = shell.core.transport.clone()?;
        let owner = Arc::new(crate::bootstrap::ReadOwner::capture(
            &shell.core,
            &transport,
            &session,
        )?);
        let account = shell.core.active_account.clone()?;
        Some(Arc::new(Self {
            account,
            team,
            channel,
            base: base.clone(),
            assignment,
            canonical,
            session,
            transport,
            client: shell.core.client.clone(),
            main: shell.main_view,
            owner,
        }))
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        !crate::fixture::is_fixture()
            && shell.core.screen == super_platinum_core::state::Screen::Main
            && !shell.loading
            && shell.signed_in
            && shell.main_view == self.main
            && shell.core.active_team.as_ref() == Some(&self.team)
            && shell.core.active_channel.as_ref() == Some(&self.channel)
            && self.owner.current(&shell.core)
            && shell.core.client.same_configuration(&self.client)
            && self.assignment.current(shell)
            && self.canonical.current_for(shell, &self.team)
            && super::super::inventory(shell).as_ref() == Some(&self.base)
            && self.canonical.row(&self.channel).is_some_and(|r| {
                ordinary_joined(r, &self.channel)
                    && Inventory::from_channel(r).ok().as_ref() == Some(&self.base)
            })
    }
}
