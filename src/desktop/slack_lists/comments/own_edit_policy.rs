//! Fresh ordinary-own-edit policy. Raw receipts are not global cache grants.
use super::*;
use std::{future::Future, time::Duration};
use super_platinum_core::{
    block_actions::name_policy,
    slack::{api, client::PreparedRequest},
    slack_lists::comments::OwnEditPolicy,
    state::posting_metadata::authority::{Authority, RawChannel},
};
mod decode;
#[cfg(test)]
mod tests;

pub(super) struct Observation {
    owner: Owner,
    target: Target,
    socket: u64,
    revision: u64,
    authority: Authority,
    channel: RawChannel,
    policy: OwnEditPolicy,
    proof: Value,
}
impl Observation {
    pub(super) fn authority(&self) -> Authority {
        self.authority.clone()
    }
    pub(super) fn policy(&self) -> &OwnEditPolicy {
        &self.policy
    }
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        active(shell, &self.owner, &self.target, self.socket, self.revision)
            && self.authority.current(&shell.core)
            && self.channel.current(&shell.core)
            && self.authority.metadata(&shell.core).is_ok_and(|metadata| {
                let config = self.authority.configuration();
                metadata
                    .context(config, self.channel.value())
                    .is_ok_and(|c| self.proof["topLevel"] == c.top_level_team_id)
                    && metadata
                        .relevant_workspaces(config, self.channel.value())
                        .is_ok_and(|ids| self.proof["relevantWorkspaces"] == serde_json::json!(ids))
                    && metadata
                        .thread_read_only(config, self.channel.value())
                        .is_ok_and(|read_only| self.proof["canThread"] == !read_only)
            })
    }
    pub(super) fn proof(&self) -> Value {
        self.proof.clone()
    }
}
/// Even before a caret query, accepted preference/team policy events must retire
/// an edit's observations. Root wires this into the existing revision producer.
pub(crate) fn observes_preferences(shell: &ShellState, team: &str, generation: u64) -> bool {
    shell.slack_lists.comments.entry.as_ref().is_some_and(|e| {
        e.owner.scope.team == team
            && e.owner.active(shell)
            && shell
                .core
                .workspaces
                .get(team)
                .is_some_and(|w| w.rt_generation == generation)
    })
}
fn active(shell: &ShellState, owner: &Owner, target: &Target, socket: u64, revision: u64) -> bool {
    owner.active(shell)
        && shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .is_some_and(|e| e.target == *target)
        && shell
            .core
            .workspaces
            .get(&owner.scope.team)
            .is_some_and(|w| w.rt_generation == socket)
        && crate::app_home::autocomplete::preferences_revision(shell) == revision
}
/// Existing actual factories: raw channel, complete user prefs, original
/// assignment response and top-level/workspace team preference carrier.
fn requests(
    client: &super_platinum_core::slack::SlackClient,
    session: &super_platinum_core::config::WorkspaceSession,
    target: &Target,
) -> [PreparedRequest; 4] {
    [
        api::conversations_generic_info(
            client,
            session,
            &BTreeMap::from([(target.channel_id().to_owned(), 0)]),
        ),
        name_policy::request::user(client, session),
        api::list_experiments(client, session),
        api::own_profile_team_preferences_request(client, session),
    ]
}
async fn guarded_read<F, Fut>(
    state: Signal<ShellState>,
    request: PreparedRequest,
    reader: &F,
    current: &impl Fn(&ShellState) -> bool,
) -> Result<Value, Error>
where
    F: Fn(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<Value, Error>>,
{
    if !current(&state.peek()) {
        return Err(unavailable());
    }
    let result = tokio::select! {
        biased;
        _=async { loop {tokio::time::sleep(Duration::from_millis(100)).await; if !current(&state.peek()) {break;} } }=>return Err(unavailable()),
        _=tokio::time::sleep(Duration::from_secs(45))=>return Err(Error::Transport("Comment edit policy read timed out".into())),
        value=reader(request)=>value?,
    };
    if !current(&state.peek()) {
        return Err(unavailable());
    }
    Ok(result)
}
/// Production entry point never falls through to a live carrier in fixture mode.
pub(super) async fn acquire(
    state: Signal<ShellState>,
    owner: &Owner,
    serial: u64,
) -> Result<Observation, Error> {
    if crate::fixture::is_fixture() {
        return Err(unavailable());
    }
    let ctx = owner.scope.request(&state.peek()).ok_or_else(unavailable)?;
    let transport = ctx.transport;
    acquire_with(state, owner, serial, move |request| {
        let transport = transport.clone();
        async move { transport.execute(request).await }
    })
    .await
}
/// The closed reader must validate these exact original request factories. It
/// supplies bytes only: Authority, decoding and currentness stay on this path.
pub(super) async fn acquire_with<F, Fut>(
    mut state: Signal<ShellState>,
    owner: &Owner,
    serial: u64,
    reader: F,
) -> Result<Observation, Error>
where
    F: Fn(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<Value, Error>>,
{
    let (target, socket, revision, authority, channel_read, requests) = {
        let mut shell = state.write();
        if !owner.reading(&shell, serial) {
            return Err(unavailable());
        }
        let target = shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .ok_or_else(unavailable)?
            .target
            .clone();
        let ctx = owner.scope.request(&shell).ok_or_else(unavailable)?;
        let socket = shell
            .core
            .workspaces
            .get(&owner.scope.team)
            .ok_or_else(unavailable)?
            .rt_generation;
        let revision = crate::app_home::autocomplete::preferences_revision(&shell);
        let authority =
            Authority::capture(&shell.core, &owner.scope.team).map_err(|_| unavailable())?;
        let channel_read = authority
            .begin_channel_read(&mut shell.core, target.channel_id())
            .map_err(|_| unavailable())?;
        let requests = requests(&ctx.client, &ctx.session, &target);
        (target, socket, revision, authority, channel_read, requests)
    };
    let current = |shell: &ShellState| {
        owner.reading(shell, serial)
            && active(shell, owner, &target, socket, revision)
            && authority.current(&shell.core)
    };
    let [
        channel_request,
        user_request,
        assignment_request,
        team_request,
    ] = requests;
    let channel_raw = guarded_read(state, channel_request, &reader, &current).await?;
    let raw_channel = decode::channel(&channel_raw, target.channel_id())?;
    let (channel, top_level, relevant, can_thread) = {
        let mut shell = state.write();
        if !current(&shell) {
            return Err(unavailable());
        }
        let channel = channel_read
            .apply(&mut shell.core, raw_channel)
            .map_err(|_| unavailable())?;
        let metadata = authority.metadata(&shell.core).map_err(|_| unavailable())?;
        let context = metadata
            .context(authority.configuration(), channel.value())
            .map_err(|_| unavailable())?;
        let top_level = context.top_level_team_id.to_owned();
        let relevant = metadata
            .relevant_workspaces(authority.configuration(), channel.value())
            .map_err(|_| unavailable())?;
        let can_thread = !metadata
            .thread_read_only(authority.configuration(), channel.value())
            .map_err(|_| unavailable())?;
        (channel, top_level, relevant, can_thread)
    };
    let current = |shell: &ShellState| current(shell) && channel.current(&shell.core);
    let user = guarded_read(state, user_request, &reader, &current).await?;
    let assignments = guarded_read(state, assignment_request, &reader, &current).await?;
    let extras = guarded_read(state, team_request, &reader, &current).await?;
    let (plain, portable) = decode::mode(&user, &assignments)?;
    let windows = decode::windows(&extras, &top_level, &relevant)?;
    let (_, workspace, self_user) = owner.scope.stable_owner();
    let policy = OwnEditPolicy::observed(
        &workspace,
        &self_user,
        channel.value().clone(),
        can_thread,
        target.record_raw()["archived"] == true,
        windows.clone(),
        plain,
        portable,
    )
    .map_err(|e| Error::Api(e.to_string()))?;
    if !current(&state.peek()) {
        return Err(unavailable());
    }
    let proof = serde_json::json!({"workspace":workspace,"user":self_user,"topLevel":top_level,"relevantWorkspaces":relevant,
        "channelResponse":channel_raw,"channel":channel.value(),"userPreferences":user,"assignments":assignments,"teamPreferences":extras,
        "editWindows":windows,"plainTextMode":plain,"portableContentComposer":portable,"canThread":can_thread,
        "archiveView":target.record_raw()["archived"]==true,"socket":socket,"preferencesRevision":revision});
    Ok(Observation {
        owner: owner.clone(),
        target,
        socket,
        revision,
        authority,
        channel,
        policy,
        proof,
    })
}
