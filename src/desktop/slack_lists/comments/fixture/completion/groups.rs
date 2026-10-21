//! Closed group search and original-owner source publication for List acceptance.
use super::*;
use super_platinum_core::{
    block_actions::directory_endpoint::DirectoryEndpoint,
    slack::{SlackClient, api::custom::texty::groups as wire, client::RequestBody},
};

pub(crate) const FIRST: &str = "SLISTGRP1";
pub(crate) const SECOND: &str = "SLISTGRP2";
pub(crate) const HIDDEN: &str = "SLISTGRP3";
pub(crate) const DELETED: &str = "SLISTGRP4";
pub(crate) const LABEL: &str = "@alex-team";

pub(crate) fn named() -> bool {
    matches!(
        std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref(),
        Ok("group-completion"
            | "group-completion-custody"
            | "group-completion-restricted"
            | "file-completion"
            | "file-completion-custody"
            | "file-completion-off"
            | "file-navigation"
            | "file-navigation-custody")
    )
}
pub(crate) fn restricted() -> bool {
    named()
        && std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref()
            == Ok("group-completion-restricted")
}
fn row(owner: &WorkspaceSession, id: &str, handle: &str, deleted: bool, hidden: bool) -> Value {
    json!({"id":id,"team_id":owner.team_id,"name":format!("Alex group {id}"),
        "handle":handle,"date_update":101,"date_delete":if deleted {123}else{0},
        "is_hidden":hidden,"users":[],"user_count":"0","is_external":false,
        "fixture_opaque":{"preserve":id,"ordered":["raw","effective"]}})
}
pub(crate) fn rows(owner: &WorkspaceSession) -> Vec<Value> {
    vec![
        row(owner, FIRST, "alex-team", false, false),
        row(owner, SECOND, "alex-team", false, false),
        row(owner, HIDDEN, "alex-hidden", false, true),
        row(owner, DELETED, "alex-deleted", true, false),
    ]
}
pub(super) fn reply(owner: &WorkspaceSession, mode: Mode) -> Result<Reply, Error> {
    Ok((
        match mode {
            Mode::Offline => {
                return Err(Error::Offline(
                    "Synthetic List group search unavailable".into(),
                ));
            }
            Mode::Malformed => json!({"ok":true,"results":false}),
            Mode::Empty => json!({"ok":true,"results":[]}),
            Mode::Success | Mode::Held => {
                json!({"ok":true,"results":if named(){rows(owner)}else{vec![]}})
            }
        },
        None,
    ))
}
pub(super) fn plan(
    client: &SlackClient,
    owner: &WorkspaceSession,
    request: &PreparedRequest,
) -> Result<(PreparedRequest, Value), Error> {
    let RequestBody::Json(body) = &request.body else {
        return Err(refused());
    };
    let query = wire::Query::capture(owner, body["query"].as_str().ok_or_else(refused)?, true)?;
    let now = Instant::now();
    let endpoint = DirectoryEndpoint::decode(
        owner,
        &json!({"ok":true,
        "url":format!("https://edgeapi.slack.com/cache/{}",owner.team_id),"ttl_seconds":300}),
        now,
    )?;
    Ok((
        query.request(client, owner, &endpoint, now, None)?,
        reply(owner, Mode::Success)?.0,
    ))
}
async fn publish(mut state: Signal<ShellState>, rows: Vec<Value>) -> Result<Value, Error> {
    use crate::usergroups::source::{self, Admission, NativeMembers, Operation, Origin};
    let model = handle(&state.peek())?;
    if !named() {
        return Err(refused());
    }
    let owner = Arc::new(crate::directory_member_fetches::Context::capture(
        &state.peek(),
        &model.session.team_id,
    )?);
    let work = match source::enqueue(
        &mut state.write(),
        owner.clone(),
        Operation::Metadata {
            origin: Origin::Fetch,
            rows: rows.clone(),
            native_members: NativeMembers::Preserve,
        },
    ) {
        Admission::Tracked(work) => work,
        Admission::Unavailable => return Err(refused()),
    };
    work.run(state, |shell| model.current(shell) && owner.current(shell))
        .await?;
    let shell = state.peek();
    if !model.current(&shell) || !owner.current(&shell) {
        return Err(refused());
    }
    let workspace = shell
        .core
        .workspaces
        .get(&model.session.team_id)
        .ok_or_else(refused)?;
    let observed = workspace
        .directory_usergroups
        .snapshot(
            &owner.account,
            &owner.session,
            owner.epoch,
            &workspace.usergroups,
        )
        .map_err(|_| refused())?;
    Ok(json!({"incoming":rows,"rows":observed.rows(),"membership":observed.membership()}))
}
pub(super) async fn seed(state: Signal<ShellState>) -> Result<Value, Error> {
    let owner = handle(&state.peek())?.session;
    publish(state, vec![rows(&owner).remove(0)]).await
}
pub(crate) async fn replace(
    state: Signal<ShellState>,
    id: &str,
    handle: &str,
    deleted: bool,
) -> Result<Value, Error> {
    let owner = super::handle(&state.peek())?.session;
    if ![FIRST, SECOND, HIDDEN].contains(&id) {
        return Err(refused());
    }
    let mut incoming = row(&owner, id, handle, deleted, id == HIDDEN);
    incoming["date_update"] = json!(if deleted { 103 } else { 102 });
    publish(state, vec![incoming]).await
}
pub(crate) fn snapshot(shell: &ShellState) -> Result<Value, Error> {
    let model = handle(shell)?;
    let owner = crate::directory_member_fetches::Context::capture(shell, &model.session.team_id)?;
    let workspace = shell
        .core
        .workspaces
        .get(&model.session.team_id)
        .ok_or_else(refused)?;
    let observed = workspace
        .directory_usergroups
        .snapshot(
            &owner.account,
            &owner.session,
            owner.epoch,
            &workspace.usergroups,
        )
        .map_err(|_| refused())?;
    Ok(
        json!({"rows":observed.rows(),"membership":observed.membership(),
        "selfRestricted":workspace.users.get(&owner.session.user_id).map(|u|u.is_restricted)}),
    )
}
