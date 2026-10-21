//! Fresh ordinary-own deletion policy. Editing mode and edit windows are not
//! deletion authority. Each receipt remains bound to its original List owner.
use super::*;
use std::{collections::BTreeSet, future::Future, time::Duration};
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{api, client::PreparedRequest},
    slack_lists::comments::{OwnDeletePermissionQuery, OwnDeletePolicy},
    state::{
        directory_members::Subset,
        posting_metadata::authority::{Authority, RawChannel},
    },
};
mod facts;
#[cfg(test)]
mod tests;
const MEMBER_GENERATION: &str = "list-own-delete-self";
fn missing(what: &str) -> Error {
    Error::Api(format!(
        "Comment deletion needs current {what}. Refresh comments and try again."
    ))
}

pub(super) struct Observation {
    owner: Owner,
    target: Target,
    socket: u64,
    revision: u64,
    authority: Authority,
    channel: RawChannel,
    root: Value,
    member: Option<Subset>,
    session: WorkspaceSession,
    account: String,
    epoch: u64,
    policy: OwnDeletePolicy,
    proof: Value,
}
impl Observation {
    pub(super) fn policy(&self) -> &OwnDeletePolicy {
        &self.policy
    }
    pub(super) fn proof(&self) -> Value {
        self.proof.clone()
    }
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        active(shell, &self.owner, &self.target, self.socket, self.revision)
            && self.authority.current(&shell.core)
            && self.channel.current(&shell.core)
            && root(shell, &self.target).is_ok_and(|r| r == self.root)
            && self.member.as_ref().is_none_or(|source| {
                shell
                    .core
                    .workspaces
                    .get(&self.session.team_id)
                    .is_some_and(|w| {
                        source.current_rows(
                            &w.directory_members,
                            &self.account,
                            &self.session,
                            self.epoch,
                            MEMBER_GENERATION,
                            &w.users,
                        )
                    })
            })
            && observed(shell, &self.authority, self.channel.value(), &self.target)
                .is_ok_and(|v| v == self.proof["facts"])
    }
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
fn root(shell: &ShellState, target: &Target) -> Result<Value, Error> {
    let raw = member_root::snapshot(shell).map_err(|_| missing("original thread root"))?;
    facts::root(&raw, target)?;
    Ok(raw)
}
fn observed(
    shell: &ShellState,
    authority: &Authority,
    channel: &Value,
    target: &Target,
) -> Result<Value, Error> {
    let metadata = authority
        .metadata(&shell.core)
        .map_err(|_| missing("account metadata"))?;
    let config = authority.configuration();
    let context = metadata
        .context(config, channel)
        .map_err(|_| missing("account role metadata"))?;
    let can_thread = !facts::flag(channel, "is_non_threadable")?
        && !metadata
            .thread_read_only(config, channel)
            .map_err(|_| missing("thread posting policy"))?;
    let file_read_only = facts::read_only_file_thread(channel, target)?;
    Ok(
        serde_json::json!({"caller":{"app":"client","adminPageContext":null,"source":"native-ordinary-client"},"asAdmin":false,"canThread":can_thread,"readOnlyFileThread":file_read_only,
        "member":context.member,"topLevel":context.top_level_team_id}),
    )
}
fn native_client_permission_query(
    session: &WorkspaceSession,
    target: &Target,
) -> Result<OwnDeletePermissionQuery, Error> {
    // XSM4.TZ is bootData.app==='client'; 0+JV.s$/RE inspect admin-site
    // pageContext org/team, not actor privilege. This application owns an
    // ordinary Client route and implements no Slack admin-site page context.
    // This is a native caller identity, never a fabricated userBoot HTTP field.
    OwnDeletePermissionQuery::capture(
        &session.team_id,
        &session.user_id,
        target.channel_id(),
        false,
    )
    .and_then(|q| q.with_enterprise_scope(session.enterprise_id.as_deref()))
    .map_err(|e| Error::Api(e.to_string()))
}
fn self_source(
    shell: &ShellState,
    session: &WorkspaceSession,
    account: &str,
    channel: &Value,
) -> Result<(Option<Subset>, bool), Error> {
    // The source external-author predicate short-circuits on an unshared channel.
    if !facts::flag(channel, "is_ext_shared")? {
        return Ok((None, false));
    }
    let workspace = shell
        .core
        .workspaces
        .get(&session.team_id)
        .ok_or_else(|| missing("self member"))?;
    let source = workspace
        .directory_members
        .subset(
            account,
            session,
            shell.core.account_epoch,
            MEMBER_GENERATION,
            &BTreeSet::from([session.user_id.clone()]),
            &workspace.users,
        )
        .map_err(|_| missing("canonical self member"))?;
    let [row] = source.rows() else {
        return Err(missing("canonical self member"));
    };
    if row.source["id"] != session.user_id
        || facts::flag(&row.source, "isUnknown")?
        || facts::flag(&row.source, "isNonExistent")?
    {
        return Err(missing("canonical self member"));
    }
    let external = facts::flag(&row.source, "isExternal")?;
    Ok((Some(source), external))
}
async fn guarded_read<T>(
    current: impl Fn() -> bool,
    work: impl Future<Output = Result<T, Error>>,
) -> Result<T, Error> {
    if !current() {
        return Err(missing("original owner"));
    }
    let result = tokio::select! {
        biased;
        _=async {loop {tokio::time::sleep(Duration::from_millis(100)).await;if !current(){break;}}}=>return Err(missing("original owner")),
        _=tokio::time::sleep(Duration::from_secs(45))=>return Err(Error::Transport("Comment delete policy read timed out".into())),
        result=work=>result?,
    };
    if !current() {
        return Err(missing("original owner"));
    }
    Ok(result)
}
pub(super) async fn acquire(
    state: Signal<ShellState>,
    owner: &Owner,
    serial: u64,
) -> Result<Observation, Error> {
    if crate::fixture::is_fixture() {
        return Err(missing("closed fixture reader"));
    }
    let context = owner
        .scope
        .request(&state.peek())
        .ok_or_else(|| missing("original session"))?;
    acquire_from(
        state,
        owner,
        serial,
        move |request, query: Option<OwnDeletePermissionQuery>| {
            let transport = context.transport.clone();
            async move {
                match query {
                    Some(query) => transport
                        .execute_list_comment_delete_permission(request, query, || {
                            owner.reading(&state.peek(), serial)
                        })
                        .await
                        .map(|p| p.raw().clone()),
                    None => {
                        transport
                            .execute_list_comments_acquire(request, || {
                                owner.reading(&state.peek(), serial)
                            })
                            .await
                    }
                }
            }
        },
    )
    .await
}
pub(super) async fn acquire_with<F, Fut>(
    state: Signal<ShellState>,
    owner: &Owner,
    serial: u64,
    reader: F,
) -> Result<Observation, Error>
where
    F: Fn(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<Value, Error>>,
{
    acquire_from(state, owner, serial, move |request, _| reader(request)).await
}
async fn acquire_from<F, Fut>(
    mut state: Signal<ShellState>,
    owner: &Owner,
    serial: u64,
    reader: F,
) -> Result<Observation, Error>
where
    F: Fn(PreparedRequest, Option<OwnDeletePermissionQuery>) -> Fut,
    Fut: Future<Output = Result<Value, Error>>,
{
    let (target, socket, revision, authority, read, context, original_root, account) = {
        let mut shell = state.write();
        if !owner.reading(&shell, serial) {
            return Err(missing("original owner"));
        }
        let target = shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .ok_or_else(|| missing("original item"))?
            .target
            .clone();
        let context = owner
            .scope
            .request(&shell)
            .ok_or_else(|| missing("original session"))?;
        let account = shell
            .core
            .active_account
            .clone()
            .ok_or_else(|| missing("original account"))?;
        let socket = shell
            .core
            .workspaces
            .get(&owner.scope.team)
            .ok_or_else(|| missing("workspace"))?
            .rt_generation;
        let revision = crate::app_home::autocomplete::preferences_revision(&shell);
        let original_root = root(&shell, &target)?;
        let authority = Authority::capture(&shell.core, &owner.scope.team)
            .map_err(|_| missing("fresh account metadata"))?;
        let read = authority
            .begin_channel_read(&mut shell.core, target.channel_id())
            .map_err(|_| missing("channel metadata"))?;
        (
            target,
            socket,
            revision,
            authority,
            read,
            context,
            original_root,
            account,
        )
    };
    let read_state = state;
    let current = || {
        let shell = read_state.peek();
        owner.reading(&shell, serial)
            && active(&shell, owner, &target, socket, revision)
            && authority.current(&shell.core)
            && root(&shell, &target).is_ok_and(|r| r == original_root)
    };
    let request = api::conversations_generic_info(
        &context.client,
        &context.session,
        &BTreeMap::from([(target.channel_id().to_owned(), 0)]),
    );
    let response = guarded_read(&current, reader(request, None)).await?;
    let raw = facts::channel(&response, target.channel_id())?;
    let (channel, values, member, external) = {
        let mut shell = state.write();
        if !owner.reading(&shell, serial)
            || !active(&shell, owner, &target, socket, revision)
            || !authority.current(&shell.core)
            || root(&shell, &target)? != original_root
        {
            return Err(missing("original source"));
        }
        let channel = read
            .apply(&mut shell.core, raw)
            .map_err(|_| missing("channel metadata"))?;
        let values = observed(&shell, &authority, channel.value(), &target)?;
        let (member, external) = self_source(&shell, &context.session, &account, channel.value())?;
        (channel, values, member, external)
    };
    let current = || {
        current() && {
            let shell = read_state.peek();
            channel.current(&shell.core)
                && observed(&shell, &authority, channel.value(), &target).is_ok_and(|v| v == values)
                && member.as_ref().is_none_or(|m| {
                    shell
                        .core
                        .workspaces
                        .get(&context.session.team_id)
                        .is_some_and(|w| {
                            m.current_rows(
                                &w.directory_members,
                                &account,
                                &context.session,
                                context.epoch,
                                MEMBER_GENERATION,
                                &w.users,
                            )
                        })
                })
        }
    };
    let query = native_client_permission_query(&context.session, &target)?;
    let request = api::list_comment_delete_permissions(&context.client, &context.session, &query)?;
    let permission_raw = guarded_read(&current, reader(request, Some(query.clone()))).await?;
    let permission = query
        .accept(permission_raw.clone())
        .map_err(|e| Error::Api(e.to_string()))?;
    let policy = OwnDeletePolicy::observed(
        &context.session.team_id,
        &context.session.user_id,
        channel.value().clone(),
        values["canThread"].as_bool().unwrap(),
        values["readOnlyFileThread"].as_bool().unwrap(),
        external,
        &permission,
    )
    .map_err(|e| Error::Api(e.to_string()))?;
    if !current() {
        return Err(missing("original source"));
    }
    let proof = serde_json::json!({"channelResponse":response,"channel":channel.value(),"permissionResponse":permission_raw,
        "permissionQuery":permission.query().body(),"edgeScope":permission.query().edge_scope(),"facts":values,
        "authorExternal":external,"root":original_root,"document":target.document_raw(),"socket":socket,"metadataRevision":revision});
    Ok(Observation {
        owner: owner.clone(),
        target,
        socket,
        revision,
        authority,
        channel,
        root: original_root,
        member,
        session: context.session,
        account,
        epoch: context.epoch,
        policy,
        proof,
    })
}
