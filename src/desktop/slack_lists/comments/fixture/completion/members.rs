//! Closed ordinary member responses, admitted only by production request factories.
use super::*;
use super_platinum_core::{
    block_actions::directory_endpoint::DirectoryEndpoint,
    slack::{SlackClient, api::custom::texty::members as wire, client::RequestBody},
};

pub(crate) const FIRST: &str = "ULISTMEM1";
pub(crate) const SECOND: &str = "ULISTMEM2";
pub(crate) const DEACTIVATED: &str = "ULISTMEM3";
pub(crate) const DELETED_BOT: &str = "ULISTMEM4";
pub(crate) const LABEL: &str = "Alex Harbor";

pub(crate) fn named() -> bool {
    if super::title_users::named() || super::empty_canvas::named() {
        return true;
    }
    matches!(
        std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref(),
        Ok("member-completion"
            | "member-completion-custody"
            | "group-completion"
            | "group-completion-custody"
            | "group-completion-restricted"
            | "file-completion"
            | "file-completion-custody"
            | "file-completion-off"
            | "file-navigation"
            | "file-navigation-custody")
    )
}
fn row(owner: &WorkspaceSession, id: &str, label: &str, deleted: bool, bot: bool) -> Value {
    json!({"id":id,"team_id":owner.team_id,"name":format!("list_{}",id.to_lowercase()),
        "real_name":format!("Top real {id}"),"deleted":deleted,"is_bot":bot,
        "is_restricted":false,"is_ultra_restricted":false,"updated":101,
        "profile":{"display_name":label,"real_name":format!("Profile real {id}"),"email":format!("{}@fixture.invalid",id.to_lowercase())},
        "fixture_opaque":{"keep":id,"ordered":["first","second"]}})
}
pub(super) fn self_row(owner: &WorkspaceSession) -> Value {
    let mut value = row(owner, &owner.user_id, "Alex Self", false, false);
    value["is_admin"] = json!(true);
    value["is_restricted"] = json!(groups::restricted());
    value
}
pub(super) fn local(owner: &WorkspaceSession) -> Value {
    row(owner, FIRST, LABEL, false, false)
}
pub(crate) fn rows(owner: &WorkspaceSession) -> Vec<Value> {
    vec![
        local(owner),
        row(owner, SECOND, LABEL, false, false),
        row(owner, DEACTIVATED, "Alex Retired", true, false),
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
        "users":[self_row(owner),local(owner)],"channels":[{
            "id":LOCAL,"name":"project-local","team_id":owner.team_id,
            "is_channel":true,"is_member":true,"updated":100,"previous_names":[]
        }],"ims":[],"groups":[],"mpims":[]})
}
pub(super) fn reply(owner: &WorkspaceSession, mode: Mode) -> Result<Reply, Error> {
    Ok((
        match mode {
            Mode::Offline => {
                return Err(Error::Offline(
                    "Synthetic List member search unavailable".into(),
                ));
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
    let endpoint = DirectoryEndpoint::decode(
        owner,
        &json!({"ok":true,"url":format!("https://edgeapi.slack.com/cache/{}",owner.team_id),"ttl_seconds":300}),
        now,
    )?;
    Ok((
        query.request(client, owner, &endpoint, now, None)?,
        reply(owner, Mode::Success)?.0,
    ))
}

pub(super) fn context_plan(
    client: &SlackClient,
    owner: &WorkspaceSession,
    request: &PreparedRequest,
    method: &str,
) -> Result<(PreparedRequest, Value), Error> {
    use super_platinum_core::{
        block_actions::directory_endpoint::channel_info, slack::api::custom::texty::list_membership,
    };
    let RequestBody::Json(body) = &request.body else {
        return Err(refused());
    };
    let now = Instant::now();
    let endpoint = DirectoryEndpoint::decode(
        owner,
        &json!({"ok":true,
        "url":format!("https://edgeapi.slack.com/cache/{}",owner.team_id),"ttl_seconds":300}),
        now,
    )?;
    match method {
        "users/info" => member_info_plan(client, owner, body, &endpoint, now),
        "channels/info" => {
            let read = channel_info::Read::capture(
                vec!["CCOMMENT01".into()],
                Some([("CCOMMENT01".into(), 0)].into()),
            )?;
            Ok((
                endpoint.channel_info(client, owner, now, &read)?,
                json!({"ok":true,"results":[{
                    "id":"CCOMMENT01","name":"list-comments","team_id":owner.team_id,
                    "is_file":true,"is_member":true,"is_channel":false,"is_private":false,
                    "updated":101,"previous_names":[],"fixture_opaque":{"keep":"actual-file-channel"}
                }]}),
            ))
        }
        "users/list" => {
            let read = list_membership::Read::first_members(owner, "CCOMMENT01")?;
            Ok((
                endpoint.list_membership(client, owner, now, &read)?,
                json!({"ok":true,"results":[local(owner),row(owner,SECOND,LABEL,false,false)]}),
            ))
        }
        "channels/membership" => {
            let users = body["users"]
                .as_array()
                .ok_or_else(refused)?
                .iter()
                .map(|v| v.as_str().map(str::to_owned).ok_or_else(refused))
                .collect::<Result<Vec<_>, _>>()?;
            if users != [FIRST, SECOND] {
                return Err(refused());
            }
            let read = list_membership::Read::membership(owner, "CCOMMENT01", users)?;
            Ok((
                endpoint.list_membership(client, owner, now, &read)?,
                json!({"ok":true,"channel":"CCOMMENT01","members":[SECOND],"non_members":[FIRST]}),
            ))
        }
        _ => Err(refused()),
    }
}

fn member_info_plan(
    client: &SlackClient,
    owner: &WorkspaceSession,
    body: &Value,
    endpoint: &DirectoryEndpoint,
    now: Instant,
) -> Result<(PreparedRequest, Value), Error> {
    use super_platinum_core::{
        block_actions::{
            directory_endpoint::member_info, workflow_management::member_hydration::Current,
        },
        state::directory_members::Store,
    };
    let (read, member) = if body["updated_ids"] == json!({(FIRST):101}) {
        let source = local(owner);
        let prior = Current {
            native: source.clone(),
            source: source.clone(),
            interaction: None,
        };
        (
            member_info::Read::conditional(vec![FIRST.into()], &[prior])?,
            source,
        )
    } else if body["updated_ids"] == json!({(SECOND):0}) {
        // Closed source-absent fixture observations exercise the same unknown
        // request factory; no missing member is synthesized into a Current row.
        let mut store = Store::default();
        store.begin("member-fixture", owner, &Default::default())?;
        let capture = member_info::unknown::Capture::network_only(
            store.observations("member-fixture", owner, 0, "member-fixture")?,
            &[SECOND.into()],
        )?;
        (
            capture.reads()?.into_iter().next().ok_or_else(refused)?,
            row(owner, SECOND, LABEL, false, false),
        )
    } else {
        return Err(refused());
    };
    let id = member["id"].as_str().ok_or_else(refused)?.to_owned();
    Ok((
        endpoint.member_info(client, owner, now, &read)?,
        json!({"ok":true,"results":[member],"can_interact":{(id):true}}),
    ))
}
