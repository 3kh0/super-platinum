use super::*;
use std::hash::{Hash, Hasher};
use super_platinum_core::slack::{Error, Transport};

pub(in crate::messaging) struct Owner {
    pub(in crate::messaging) account: String,
    epoch: u64,
    transport: Option<Arc<Transport>>,
    credentials: u64,
    team: Option<(String, String)>,
}
pub(in crate::messaging) fn credentials(shell: &ShellState) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    if let Some(session) = &shell.core.session {
        session.d_cookie.hash(&mut h);
        for (id, workspace) in &session.workspaces {
            (id, &workspace.user_id, &workspace.token).hash(&mut h);
        }
    }
    shell
        .core
        .transport
        .as_ref()
        .map(|t| t.d_cookie())
        .hash(&mut h);
    h.finish()
}
impl Owner {
    pub(in crate::messaging) fn capture(shell: &ShellState, team: Option<&str>) -> Option<Self> {
        Some(Self {
            account: shell.core.active_account.clone()?,
            epoch: shell.core.account_epoch,
            transport: shell.core.transport.clone(),
            credentials: credentials(shell),
            team: match team {
                Some(team) => Some((
                    team.into(),
                    shell.core.workspaces.get(team)?.self_user_id.clone(),
                )),
                None => None,
            },
        })
    }
    pub(in crate::messaging) fn current(&self, shell: &ShellState) -> bool {
        shell.core.active_account.as_ref() == Some(&self.account)
            && shell.core.account_epoch == self.epoch
            && credentials(shell) == self.credentials
            && match (&self.transport, &shell.core.transport) {
                (None, None) => true,
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                _ => false,
            }
            && self.team.as_ref().is_none_or(|(team, user)| {
                shell
                    .core
                    .workspaces
                    .get(team)
                    .is_some_and(|w| w.self_user_id == *user)
            })
    }
}
pub(in crate::messaging) struct Pending {
    backend: Backend,
    stored: Arc<Mutex<Stored>>,
}
impl Pending {
    pub(in crate::messaging) async fn finalize(
        &self,
        mut state: Signal<ShellState>,
        owner: &Owner,
        ids: Vec<String>,
    ) -> Result<(), Error> {
        // Settlement belongs to the reservation owner, even if its UI retired.
        // The exact owner check below still prevents any later share/UI update.
        let backend = self.backend.clone();
        let ticket = self.stored.lock().unwrap().ticket.clone();
        let row = tokio::task::spawn_blocking(move || backend.finalize(ticket, ids))
            .await
            .map_err(|_| Error::Transport("Upload recovery storage task stopped".into()))?
            .map_err(|_| Error::Transport("Upload recovery identities could not be saved".into()))?
            .ok_or_else(|| Error::Transport("Upload recovery reservation was retired".into()))?;
        *self.stored.lock().unwrap() = row.clone();
        if !owner.current(&state.peek()) {
            return Err(Error::UploadCanceled);
        }
        remember(&mut state.write(), row);
        Ok(())
    }
    pub(in crate::messaging) async fn retire(&self, mut state: Signal<ShellState>, owner: &Owner) {
        let row = self.stored.lock().unwrap().clone();
        if owner.current(&state.peek()) {
            let mut shell = state.write();
            shell.upload_journal.rows.retain(|r| r.ticket != row.ticket);
            shell.upload_journal.revision = shell.upload_journal.revision.wrapping_add(1);
        }
        let backend = self.backend.clone();
        let _ = tokio::task::spawn_blocking(move || backend.retire(row)).await;
    }
}
pub(in crate::messaging) struct ReserveFailure {
    pub(in crate::messaging) message: &'static str,
    pub(in crate::messaging) may_restore: bool,
}
impl From<&'static str> for ReserveFailure {
    fn from(message: &'static str) -> Self {
        Self {
            message,
            may_restore: true,
        }
    }
}
pub(in crate::messaging) async fn reserve(
    mut state: Signal<ShellState>,
    owner: &Owner,
    client: &str,
) -> Result<Pending, ReserveFailure> {
    let (backend, draft) = {
        let shell = state.peek();
        if !owner.current(&shell) {
            return Err("Upload owner changed before reservation".into());
        }
        let p = shell
            .core
            .pending_file_messages
            .iter()
            .find(|p| p.client_msg_id == client)
            .ok_or("Upload is no longer available")?;
        let user = shell
            .core
            .workspaces
            .get(&p.team)
            .ok_or("Upload workspace is unavailable")?
            .self_user_id
            .clone();
        (
            shell.upload_journal.backend.clone(),
            Draft {
                account_id: owner.account.clone(),
                team_id: p.team.clone(),
                user_id: user,
                channel_id: p.channel.clone(),
                thread_ts: p.thread_ts.clone(),
                reply_broadcast: p.reply_broadcast,
                client_msg_id: p.client_msg_id.clone(),
                optimistic_ts: p.message_ts.clone(),
                caption: p.text.clone(),
                attachments: p
                    .attachments
                    .iter()
                    .map(|a| uploads::Attachment {
                        name: a.name.clone(),
                        bytes: a.bytes,
                    })
                    .collect(),
            },
        )
    };
    let disk = backend.clone();
    let reserved = tokio::task::spawn_blocking(move || disk.reserve(draft))
        .await
        .map_err(|_| "Upload recovery storage task stopped")?
        .map_err(|_| "Upload recovery could not be saved; nothing was sent")?;
    if !reserved.created {
        remember(&mut state.write(), reserved.stored);
        return Err(ReserveFailure {
            message: "This upload already has a recovery receipt; it was not sent again",
            may_restore: false,
        });
    }
    let pending = Pending {
        backend,
        stored: Arc::new(Mutex::new(reserved.stored.clone())),
    };
    if !owner.current(&state.peek()) {
        pending.retire(state, owner).await;
        return Err("Upload owner changed before sending".into());
    }
    remember(&mut state.write(), reserved.stored);
    Ok(pending)
}
