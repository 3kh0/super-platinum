use super::*;
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{SlackClient, Transport, models::File},
};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Key {
    pub account: String,
    pub team: String,
    pub user: String,
}
// A fixture capability is an allocation created by the explicitly validated
// offline startup path. It never creates a Transport or installs credentials.
#[derive(Clone)]
enum Backend {
    Live {
        cookie: String,
        transport: Arc<Transport>,
    },
    Fixture(fixture::Capability),
}
#[derive(Clone)]
pub(super) struct Context {
    pub key: Key,
    epoch: u64,
    backend: Backend,
    pub client: SlackClient,
    pub session: WorkspaceSession,
}
impl Context {
    pub fn capture(shell: &ShellState) -> Option<Self> {
        if let Some(capability) = &shell.file_share_fixture {
            return capability.current(shell).then(|| Self {
                key: capability.key(),
                epoch: capability.epoch(),
                backend: Backend::Fixture(capability.clone()),
                client: SlackClient::default(),
                session: capability.builder_session(),
            });
        }
        let account = shell.core.active_account.clone()?;
        if account.is_empty() || account.len() > 512 || account.chars().any(char::is_control) {
            return None;
        }
        let team = shell.core.active_team.as_ref()?;
        let credentials = shell.core.session.as_ref()?;
        let session = credentials.workspaces.get(team)?.clone();
        let workspace = shell.core.workspaces.get(team)?;
        if session.team_id != *team
            || workspace.team_id != *team
            || session.user_id.is_empty()
            || workspace.self_user_id != session.user_id
            || shell.self_account.user_id != session.user_id
        {
            return None;
        }
        Some(Self {
            key: Key {
                account,
                team: team.clone(),
                user: session.user_id.clone(),
            },
            epoch: shell.core.account_epoch,
            backend: Backend::Live {
                cookie: credentials.d_cookie.clone(),
                transport: shell.core.transport.clone()?,
            },
            client: shell.core.client.clone(),
            session,
        })
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        Self::capture(shell).is_some_and(|c| {
            let backend = match (&self.backend, &c.backend) {
                (
                    Backend::Live { cookie, transport },
                    Backend::Live {
                        cookie: next_cookie,
                        transport: next_transport,
                    },
                ) => cookie == next_cookie && Arc::ptr_eq(transport, next_transport),
                (Backend::Fixture(a), Backend::Fixture(b)) => a.same_identity(b),
                _ => false,
            };
            backend
                && c.key == self.key
                && c.epoch == self.epoch
                && c.session.token == self.session.token
                && c.session.url == self.session.url
                && c.session.enterprise_id == self.session.enterprise_id
        })
    }
    pub fn fixture(&self) -> Option<&fixture::Capability> {
        match &self.backend {
            Backend::Fixture(capability) => Some(capability),
            _ => None,
        }
    }
    pub async fn file_info(&self, id: &str) -> Result<Value, Error> {
        match &self.backend {
            Backend::Live { transport, .. } => {
                transport
                    .execute(api::file_info(&self.client, &self.session, id))
                    .await
            }
            Backend::Fixture(capability) => capability.file_info(id).await,
        }
    }
    pub async fn channel_info(&self, id: &str) -> Result<Value, Error> {
        match &self.backend {
            Backend::Live { transport, .. } => {
                transport
                    .execute(api::conversations_info(
                        &self.client,
                        &self.session,
                        id.to_owned(),
                    ))
                    .await
            }
            Backend::Fixture(capability) => capability.channel_info(id).await,
        }
    }
    pub async fn peer_info(&self, id: &str) -> Result<Value, Error> {
        match &self.backend {
            Backend::Live { transport, .. } => {
                let request = super_platinum_core::slack::edge::users_info(
                    &self.client,
                    &self.session,
                    &[id.to_owned()],
                )
                .map_err(|_| {
                    Error::Transport("File share peer request could not be prepared".into())
                })?;
                transport.execute(request).await
            }
            Backend::Fixture(capability) => capability.peer_info(id).await,
        }
    }
    pub async fn after_begin(&self, receipt: &Receipt) -> Result<(), Error> {
        match &self.backend {
            Backend::Live { .. } => Ok(()),
            Backend::Fixture(capability) => capability.after_begin(receipt).await,
        }
    }
    pub async fn share(
        &self,
        receipt: &Receipt,
        request: super_platinum_core::slack::PreparedRequest,
    ) -> Result<Value, Error> {
        match &self.backend {
            Backend::Live { transport, .. } => transport.execute_draft_mutation(request).await,
            Backend::Fixture(capability) => capability.share(receipt, request).await,
        }
    }
}

#[derive(Clone)]
pub(super) struct Source {
    pub context: Context,
    pub guard: crate::file_actions::attachment::Guard,
    pub file: Value,
    pub id: String,
    pub title: String,
    pub link: Option<String>,
    generation: u64,
    connected: bool,
}
impl Source {
    pub fn capture(shell: &ShellState, target: &crate::file_actions::Target) -> Option<Self> {
        let context = Context::capture(shell)?;
        if target.epoch != shell.core.account_epoch || target.team != context.key.team {
            return None;
        }
        let guard = target.attachment.clone()?;
        let file = guard.canonical_file(shell)?;
        if policy::source(file) != Eligibility::Allowed
            || file.id.as_deref() != Some(target.id.as_str())
        {
            return None;
        }
        let raw = serde_json::to_value(file).ok()?;
        if !bounded(&raw, 256 * 1024, 32) {
            return None;
        }
        let workspace = shell.core.workspaces.get(&context.key.team)?;
        Some(Self {
            generation: workspace.rt_generation,
            connected: matches!(
                workspace.rt,
                super_platinum_core::state::RealtimeStatus::Connected(_)
            ),
            context,
            guard,
            file: raw,
            id: target.id.clone(),
            title: target.title.clone(),
            link: target.link.clone(),
        })
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        self.context.current(shell)
            && shell
                .core
                .workspaces
                .get(&self.context.key.team)
                .is_some_and(|w| {
                    w.rt_generation == self.generation
                        && matches!(
                            w.rt,
                            super_platinum_core::state::RealtimeStatus::Connected(_)
                        ) == self.connected
                })
            && self.guard.canonical_file(shell).is_some_and(|f| {
                policy::source(f) == Eligibility::Allowed
                    && serde_json::to_value(f).ok().as_ref() == Some(&self.file)
            })
    }
    pub fn fresh(&self, raw: Value) -> Result<File, Refusal> {
        if !bounded(&raw, 256 * 1024, 32) {
            return Err(Refusal::Source);
        }
        if raw.get("ok") != Some(&Value::Bool(true))
            || raw.get("error").is_some_and(|v| !v.is_null())
        {
            return Err(Refusal::Source);
        }
        let file: File = serde_json::from_value(raw.get("file").cloned().ok_or(Refusal::Source)?)
            .map_err(|_| Refusal::Source)?;
        if file.id.as_deref() != Some(self.id.as_str())
            || super_platinum_core::state::file_title(&file) != self.title
            || policy::source(&file) != Eligibility::Allowed
        {
            return Err(Refusal::Source);
        }
        Ok(file)
    }
}

pub(crate) fn destinations(shell: &ShellState, query: &str) -> Vec<Destination> {
    let Some(team) = shell
        .core
        .active_team
        .as_ref()
        .and_then(|t| shell.core.workspaces.get(t))
    else {
        return vec![];
    };
    let query = query.trim().trim_start_matches('#').to_lowercase();
    let candidates = team
        .channels
        .values()
        .filter(|c| recipients::eligible(shell, &c.id))
        .filter_map(|c| {
            let name = super_platinum_core::state::channel_display_name(team, c);
            name.to_lowercase().contains(&query).then(|| Destination {
                id: c.id.clone(),
                name,
                private: c.is_private || c.is_group,
            })
        });
    let mut result: Vec<(String, Destination)> = Vec::with_capacity(30);
    for destination in candidates {
        let rank = destination.name.to_lowercase();
        let index = result
            .binary_search_by(|(name, entry)| (name, &entry.id).cmp(&(&rank, &destination.id)))
            .unwrap_or_else(|index| index);
        if index < 30 {
            if result.len() == 30 {
                result.pop();
            }
            result.insert(index, (rank, destination));
        }
    }
    result
        .into_iter()
        .map(|(_, destination)| destination)
        .collect()
}
pub(super) fn selected(shell: &ShellState, id: &str) -> Option<Destination> {
    let context = Context::capture(shell)?;
    let workspace = shell.core.workspaces.get(&context.key.team)?;
    let channel = workspace.channels.get(id)?;
    recipients::capture(shell, id)?;
    Some(Destination {
        id: id.into(),
        name: super_platinum_core::state::channel_display_name(workspace, channel),
        private: channel.is_private || channel.is_group,
    })
}
pub(super) fn bounded(value: &Value, bytes: usize, depth: usize) -> bool {
    fn levels(v: &Value, left: usize) -> bool {
        left > 0
            && match v {
                Value::Array(a) => a.iter().all(|v| levels(v, left - 1)),
                Value::Object(o) => o.values().all(|v| levels(v, left - 1)),
                _ => true,
            }
    }
    struct Budget(usize);
    impl std::io::Write for Budget {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_sub(b.len())
                .ok_or(std::io::ErrorKind::FileTooLarge)?;
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    levels(value, depth) && serde_json::to_writer(Budget(bytes), value).is_ok()
}
