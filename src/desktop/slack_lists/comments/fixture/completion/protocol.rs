//! Exact factory validation for closed List completion responses.
use super::*;
use super_platinum_core::{
    block_actions::{
        directory_endpoint::DirectoryEndpoint, home::workflow::directory_observations, name_policy,
        shortcuts::grid,
    },
    drafts::presence,
    slack::{
        SlackClient,
        api::{self, custom::texty::channels},
        client::RequestBody,
    },
};
pub(super) struct Accepted {
    pub method: String,
    pub args: Value,
    pub reply: Reply,
    pub search: bool,
}
pub(in crate::slack_lists::comments::fixture) fn discovery(owner: &WorkspaceSession) -> Value {
    json!({"ok":true,"url":format!("https://edgeapi.slack.com/cache/{}",owner.team_id),"ttl_seconds":300})
}
pub(super) fn rows(owner: &WorkspaceSession) -> Vec<Value> {
    vec![
        json!({"id":PRIVATE,"name":"project-private","team_id":owner.team_id,"is_channel":true,"is_private":true,"is_member":true,"updated":101,"previous_names":[],"fixture_opaque":{"keep":"private"}}),
        json!({"id":REMOTE,"name":"project-cross-team","team_id":"TOTHER001","is_channel":true,"is_member":false,"updated":101,"previous_names":["project-previous"],"fixture_opaque":{"keep":"cross-team"}}),
    ]
}
pub(super) fn search_reply(owner: &WorkspaceSession, mode: Mode) -> Result<Reply, Error> {
    Ok((
        match mode {
            Mode::Offline => {
                return Err(Error::Offline(
                    "Synthetic List channel search unavailable".into(),
                ));
            }
            Mode::Malformed => json!({"ok":true,"results":false}),
            Mode::Empty => json!({"ok":true,"results":[]}),
            Mode::Success | Mode::Held => json!({"ok":true,"results":rows(owner)}),
        },
        None,
    ))
}
pub(in crate::slack_lists::comments::fixture) fn same_request(
    actual: &PreparedRequest,
    expected: &PreparedRequest,
) -> bool {
    actual.method == expected.method
        && actual.headers == expected.headers
        && actual.body == expected.body
        && same_url(&actual.url, &expected.url)
}
fn same_url(actual: &str, expected: &str) -> bool {
    // XParams makes fresh correlation/trace IDs for each factory invocation.
    // Compare their closed wire shapes, not two independently generated UUIDs.
    // Every fixed query value, query key/order and the complete base remain exact.
    let (Some((a_base, a_query)), Some((e_base, e_query))) =
        (actual.split_once('?'), expected.split_once('?'))
    else {
        return false;
    };
    if a_base != e_base || actual.contains('#') {
        return false;
    }
    let a = url::form_urlencoded::parse(a_query.as_bytes()).collect::<Vec<_>>();
    let e = url::form_urlencoded::parse(e_query.as_bytes()).collect::<Vec<_>>();
    a.len() == e.len()
        && a.iter()
            .zip(&e)
            .all(|((key, value), (expected_key, expected_value))| {
                if key != expected_key {
                    return false;
                }
                let length = match key.as_ref() {
                    "_x_id" | "_x_csid" | "fp" | "_x_b3_traceid" => 32,
                    "_x_b3_spanid" => 16,
                    _ => return value == expected_value,
                };
                value.len() == length
                    && value
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            })
}
fn redacted(request: &PreparedRequest) -> Result<Value, Error> {
    let mut args = match &request.body {
        RequestBody::Form(fields) => {
            let mut args = serde_json::Map::new();
            for (key, value) in fields {
                if args.insert(key.clone(), json!(value)).is_some() {
                    return Err(refused());
                }
            }
            Value::Object(args)
        }
        RequestBody::Json(value) => value.clone(),
    };
    let args = args.as_object_mut().ok_or_else(refused)?;
    args.remove("token");
    args.remove("enterprise_token");
    Ok(Value::Object(args.clone()))
}

pub(super) fn mode_target(method: &str, files: bool, groups: bool) -> bool {
    if files {
        method == "search.autocomplete.files"
    } else {
        method == "channels/search"
            || (method == "users/search" && !groups)
            || (method == "usergroups/search" && groups)
    }
}

pub(super) fn validate(
    client: &SlackClient,
    owner: &WorkspaceSession,
    request: &PreparedRequest,
) -> Result<Accepted, Error> {
    let url = url::Url::parse(&request.url).map_err(|_| refused())?;
    let member_method = members::named()
        .then(|| {
            [
                "users/search",
                "usergroups/search",
                "users/list",
                "users/info",
                "channels/membership",
                "channels/info",
            ]
            .into_iter()
            .find(|method| url.path().ends_with(&format!("/{method}")))
        })
        .flatten();
    let method = if let Some(method) = member_method {
        method
    } else if url.path().ends_with("/channels/search") {
        "channels/search"
    } else {
        url.path().strip_prefix("/api/").ok_or_else(refused)?
    };
    let search = mode_target(method, files::named(), groups::named());
    let (expected, reply) = match method {
        "users.prefs.get" => (
            name_policy::request::user(client, owner),
            if super::title_users::named() {
                super::title_users::policy()
            } else {
                json!({"ok":true,"prefs":{"frecency":"{}"}})
            },
        ),
        "experiments.getByUser" => (api::list_experiments(client, owner), files::assignments()),
        "client.gantryBoot" if super::super::plain_members::named() => (
            grid::discovery(client, owner),
            super::super::plain_members::gantry(owner),
        ),
        "client.gantryBoot" if members::named() => {
            (grid::discovery(client, owner), members::gantry(owner))
        }
        "client.userBoot" if members::named() => {
            (api::user_boot(client, owner), members::boot(owner))
        }
        "search.autocomplete.files" if files::named() && !files::off() => {
            files::plan(client, owner, request)?
        }
        "users/search" if members::named() => members::plan(client, owner, request)?,
        "usergroups/search" if members::named() => groups::plan(client, owner, request)?,
        "users/list" | "users/info" | "channels/membership" | "channels/info"
            if members::named() =>
        {
            members::context_plan(client, owner, request, method)?
        }
        "client.gantryBoot" => (
            grid::discovery(client, owner),
            json!({"ok":true,"self":{"id":owner.user_id,"team_id":owner.team_id,"profile":{}},"team":{"id":owner.team_id},"workspaces":[{"id":owner.team_id,"prefs":{"display_real_names":false}},{"id":"TOTHER001","prefs":{"display_real_names":false}}],"default_workspace":owner.team_id}),
        ),
        "api.getFlannelHttpUrl" => (
            DirectoryEndpoint::discovery(client, owner)?,
            discovery(owner),
        ),
        "drafts.list" => (
            presence::active_request(client, owner, None),
            json!({"ok":true,"drafts":[],"has_more":false}),
        ),
        "drafts.listActive" => (
            presence::active_ids_request(client, owner, None),
            json!({"ok":true,"active_draft_ids":[],"has_more":false}),
        ),
        "search.autocomplete.model" => (
            directory_observations::model_request(client, owner),
            json!({"ok":true,"model":null}),
        ),
        "search.autocomplete.offlineFeatures" => (
            directory_observations::offline_request(client, owner),
            json!({"ok":true}),
        ),
        "channels/search" => {
            let RequestBody::Json(body) = &request.body else {
                return Err(refused());
            };
            let query = channels::Query::capture(
                owner,
                body["query"].as_str().ok_or_else(refused)?,
                channels::RequestFacts {
                    top_channels: Some(json!([])),
                    default_workspace: Some(owner.team_id.clone()),
                },
            )?;
            let now = Instant::now();
            let endpoint = DirectoryEndpoint::decode(owner, &discovery(owner), now)?;
            (
                query.request(client, owner, &endpoint, now, None)?,
                search_reply(owner, Mode::Success)?.0,
            )
        }
        _ => return Err(refused()),
    };
    if !same_request(request, &expected) {
        return Err(refused());
    }
    Ok(Accepted {
        method: method.into(),
        args: redacted(request)?,
        reply: (reply, None),
        search,
    })
}
