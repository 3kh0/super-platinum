use super::*;
use crate::{scheduled::Context, state::MainView};

pub(super) fn channel_raw(s: &ShellState) -> Option<Value> {
    let w = s.core.workspaces.get(s.core.active_team.as_ref()?)?;
    serde_json::to_value(w.channels.get(s.core.active_channel.as_ref()?)?).ok()
}

/// Read state is independent of unread/last-read changes. Only the actual
/// source fields consumed by this panel retire its source observation.
fn content(raw: &Value) -> Value {
    json!({"id":raw.get("id"),"name":raw.get("name"),"properties":raw.get("properties"),
        "is_archived":raw.get("is_archived"),"is_private":raw.get("is_private"),
        "is_channel":raw.get("is_channel"),"is_group":raw.get("is_group"),
        "is_im":raw.get("is_im"),"is_mpim":raw.get("is_mpim")})
}
pub(super) struct Mount {
    account: Option<String>,
    epoch: u64,
    team: String,
    channel: String,
    generation: u64,
    main_view: MainView,
}
impl Mount {
    pub(super) fn capture(s: &ShellState) -> Option<Self> {
        if s.channel_tab != ChannelTab::Workflows || !s.conversation_visible() {
            return None;
        }
        channel_raw(s)?;
        Some(Self {
            account: s.core.active_account.clone(),
            epoch: s.core.account_epoch,
            team: s.core.active_team.clone()?,
            channel: s.core.active_channel.clone()?,
            generation: s.channel_generation,
            main_view: s.main_view,
        })
    }
    pub(super) fn current(&self, s: &ShellState) -> bool {
        s.core.active_account == self.account
            && s.core.account_epoch == self.epoch
            && s.core.active_team.as_ref() == Some(&self.team)
            && s.core.active_channel.as_ref() == Some(&self.channel)
            && s.channel_generation == self.generation
            && s.main_view == self.main_view
            && s.channel_tab == ChannelTab::Workflows
            && s.conversation_visible()
    }
}
pub(super) struct Owner {
    pub(super) id: u64,
    pub(super) ctx: Context,
    mount: Arc<Mount>,
    credentials: String,
    socket: u64,
    pub(super) raw: Value,
}
impl Owner {
    pub(super) fn capture(s: &ShellState, mount: Arc<Mount>) -> Option<Self> {
        if !mount.current(s) {
            return None;
        }
        let ctx = Context::capture(s)?;
        let w = s.core.workspaces.get(&ctx.session.team_id)?;
        if w.team_id != ctx.session.team_id || w.self_user_id != ctx.session.user_id {
            return None;
        }
        Some(Self {
            id: next(),
            ctx,
            mount,
            credentials: serde_json::to_string(s.core.session.as_ref()?).ok()?,
            socket: w.rt_generation,
            raw: channel_raw(s)?,
        })
    }
    pub(super) fn channel(&self) -> &str {
        &self.mount.channel
    }
    pub(super) fn current(&self, s: &ShellState) -> bool {
        self.mount.current(s)
            && self.ctx.current(s)
            && s.core
                .session
                .as_ref()
                .and_then(|v| serde_json::to_string(v).ok())
                .as_ref()
                == Some(&self.credentials)
            && s.core
                .workspaces
                .get(&self.ctx.session.team_id)
                .is_some_and(|w| {
                    w.rt_generation == self.socket && w.self_user_id == self.ctx.session.user_id
                })
            && channel_raw(s).is_some_and(|v| content(&v) == content(&self.raw))
    }
    pub(super) fn snapshot(&self) -> Value {
        json!({"id":self.id,"account":self.mount.account,"accountEpoch":self.mount.epoch,
            "team":self.mount.team,"channel":self.mount.channel,"channelGeneration":self.mount.generation,"socket":self.socket})
    }
}
