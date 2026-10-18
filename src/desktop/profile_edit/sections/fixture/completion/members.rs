//! Closed member fixture payloads; only production request factories admit them.
use super::*;
use super_platinum_core::slack::api::custom::texty::members as wire;
pub(crate) const FIRST: &str = "UCOMPLETE1";
pub(crate) const SECOND: &str = "UCOMPLETE2";
pub(crate) const DEACTIVATED: &str = "UCOMPLETE3";
pub(crate) const DELETED_BOT: &str = "UCOMPLETE4";
pub(crate) const LABEL: &str = "Alex Harbor";
pub(crate) const DEACTIVATED_LABEL: &str = "Alex Retired";
pub(crate) const SELF_LABEL: &str = "Alex Self";
fn row(owner: &WorkspaceSession, id: &str, label: &str, deleted: bool, bot: bool) -> Value {
    json!({"id":id,"team_id":owner.team_id,"name":format!("completion_{}",id.to_lowercase()),
        "real_name":format!("Top real {id}"),"deleted":deleted,"is_bot":bot,
        "is_restricted":false,"is_ultra_restricted":false,"updated":101,
        "profile":{"display_name":label,"real_name":format!("Profile real {id}"),"email":format!("{}@fixture.invalid",id.to_lowercase())},
        "fixture_opaque":{"preserve":id,"ordered":["first","second"]}})
}
pub(crate) fn self_row(owner: &WorkspaceSession) -> Value {
    let mut value = row(owner, &owner.user_id, SELF_LABEL, false, false);
    value["is_restricted"] = json!(super::groups::restricted());
    value
}
pub(crate) fn rows(owner: &WorkspaceSession) -> Vec<Value> {
    vec![
        row(owner, FIRST, LABEL, false, false),
        row(owner, SECOND, LABEL, false, false),
        row(owner, DEACTIVATED, DEACTIVATED_LABEL, true, false),
        row(owner, DELETED_BOT, "Alex Deleted Bot", true, true),
        self_row(owner),
    ]
}
pub(super) fn gantry(owner: &WorkspaceSession) -> Value {
    json!({"ok":true,"self":self_row(owner),"team":{"id":owner.team_id},
        "workspaces":[{"id":owner.team_id,"prefs":{"display_real_names":false}}],"default_workspace":owner.team_id})
}
pub(super) fn boot(owner: &WorkspaceSession) -> Value {
    json!({"ok":true,"self":self_row(owner),"team":{"id":owner.team_id,"name":owner.name,"plan":"plus"},
        "workspaces":[{"id":owner.team_id}],"read_only_channels":[],"thread_only_channels":[],
        "users":[self_row(owner)],"channels":[],"ims":[],"groups":[],"mpims":[]})
}
pub(super) fn reply(owner: &WorkspaceSession, mode: Mode) -> Result<Reply, Error> {
    Ok((
        match mode {
            Mode::Failure => {
                return Err(Error::Offline("Synthetic member search unavailable".into()));
            }
            Mode::Malformed => json!({"ok":true,"results":false}),
            Mode::Empty => json!({"ok":true,"results":[]}),
            Mode::Success | Mode::Held => json!({"ok":true,"results":rows(owner)}),
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
    let query = wire::Query::capture(
        owner,
        body["query"].as_str().ok_or_else(refused)?,
        wire::RequestFacts {
            top_users: Some(json!([])),
            default_workspace: Some(owner.team_id.clone()),
        },
    )?;
    let now = Instant::now();
    let endpoint = DirectoryEndpoint::decode(owner, &discovery(owner), now)?;
    Ok((
        query.request(client, owner, &endpoint, now, None)?,
        reply(owner, Mode::Success)?.0,
    ))
}
