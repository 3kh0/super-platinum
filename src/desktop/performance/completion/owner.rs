use crate::state::ShellState;
use super_platinum_core::state::Workspace;

type User = (
    String,
    String,
    bool,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);
type Channel = (String, String, Option<String>, bool, bool, bool, bool, bool);
type Group = (String, String, String, String, u64);

/// Only canonical fields read by actual suggestions; maps are never cloned per query.
/// Compare before input and after the second frame, outside measured intervals.
pub(super) struct Owner {
    account: Option<super_platinum_core::config::AccountId>,
    epoch: u64,
    generation: u64,
    users: Vec<User>,
    channels: Vec<Channel>,
    groups: Vec<Group>,
}

impl Owner {
    pub(super) fn capture(shell: &ShellState) -> Self {
        let ws = &shell.core.workspaces["T1"];
        Self {
            account: shell.core.active_account.clone(),
            epoch: shell.core.account_epoch,
            generation: shell.channel_generation,
            users: ws
                .users
                .iter()
                .map(|(key, u)| {
                    (
                        key.clone(),
                        u.id.clone(),
                        u.deleted,
                        u.name.clone(),
                        u.real_name.clone(),
                        u.profile.as_ref().and_then(|p| p.display_name.clone()),
                        u.profile.as_ref().and_then(|p| p.real_name.clone()),
                    )
                })
                .collect(),
            channels: ws
                .channels
                .iter()
                .map(|(key, c)| {
                    (
                        key.clone(),
                        c.id.clone(),
                        c.name.clone(),
                        c.is_archived,
                        c.is_im,
                        c.is_mpim,
                        c.is_private,
                        c.is_group,
                    )
                })
                .collect(),
            groups: ws
                .usergroups
                .iter()
                .map(|(key, g)| {
                    (
                        key.clone(),
                        g.id.clone(),
                        g.handle.clone(),
                        g.name.clone(),
                        g.date_delete,
                    )
                })
                .collect(),
        }
    }

    pub(super) fn current(&self, shell: &ShellState) -> bool {
        shell.core.session.is_none()
            && shell.core.transport.is_none()
            && shell.core.active_account == self.account
            && shell.core.account_epoch == self.epoch
            && shell.channel_generation == self.generation
            && shell.core.active_team.as_deref() == Some("T1")
            && shell.core.active_channel.as_deref() == Some("CPERF000001")
            && shell.thread_root.is_none()
            && shell
                .core
                .workspaces
                .get("T1")
                .is_some_and(|ws| self.directory_current(ws))
    }

    fn directory_current(&self, ws: &Workspace) -> bool {
        ws.users.len() == self.users.len()
            && ws.channels.len() == self.channels.len()
            && ws.usergroups.len() == self.groups.len()
            && self
                .users
                .iter()
                .all(|(key, id, deleted, name, real, display, profile_real)| {
                    ws.users.get(key).is_some_and(|u| {
                        &u.id == id
                            && &u.deleted == deleted
                            && &u.name == name
                            && &u.real_name == real
                            && u.profile.as_ref().and_then(|p| p.display_name.as_ref())
                                == display.as_ref()
                            && u.profile.as_ref().and_then(|p| p.real_name.as_ref())
                                == profile_real.as_ref()
                    })
                })
            && self
                .channels
                .iter()
                .all(|(key, id, name, archived, im, mpim, private, group)| {
                    ws.channels.get(key).is_some_and(|c| {
                        &c.id == id
                            && &c.name == name
                            && &c.is_archived == archived
                            && &c.is_im == im
                            && &c.is_mpim == mpim
                            && &c.is_private == private
                            && &c.is_group == group
                    })
                })
            && self.groups.iter().all(|(key, id, handle, name, deleted)| {
                ws.usergroups.get(key).is_some_and(|g| {
                    &g.id == id
                        && &g.handle == handle
                        && &g.name == name
                        && &g.date_delete == deleted
                })
            })
    }
}
