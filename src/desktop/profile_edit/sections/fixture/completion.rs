//! Closed Profile completion acquisition. No HTTP client is invoked here.
use super::*;
use serde_json::json;
use std::{sync::Mutex, time::Instant};
use super_platinum_core::{
    block_actions::{
        directory_endpoint::DirectoryEndpoint, home::workflow::directory_observations, name_policy,
        shortcuts::grid,
    },
    config::WorkspaceSession,
    drafts::presence,
    slack::{PreparedRequest, SlackClient, api::custom::texty::channels, client::RequestBody},
};
use tokio::sync::oneshot;

pub(crate) const FLAG: &str = "SUPER_PLATINUM_PROFILE_CHANNEL_COMPLETION_CHECK";
pub(crate) const REMOTE: &str = "CCOMPLETE1";
pub(crate) const REMOTE_LABEL: &str = "project-completion";
pub(crate) const SECOND: &str = "CCOMPLETE2";
pub(crate) const SECOND_LABEL: &str = "project-cove";
pub(crate) mod files;
pub(crate) mod groups;
pub(crate) mod members;
pub(crate) mod title_users;

type Reply = (Value, Option<Vec<String>>);

#[derive(Clone, Default)]
pub(crate) struct Model(Arc<Mutex<Ledger>>);
#[derive(Default)]
struct Ledger {
    requests: Vec<Value>,
    mode: Mode,
    mode_method: Option<&'static str>,
    held: Vec<Held>,
    completed: usize,
    cancelled: usize,
}
struct Held {
    serial: usize,
    query: String,
    sender: oneshot::Sender<Result<Reply, Error>>,
    reply: Reply,
}
#[derive(Clone, Copy, Default)]
pub(crate) enum Mode {
    #[default]
    Success,
    Empty,
    Malformed,
    Failure,
    Held,
}
pub(crate) fn named() -> bool {
    mentions::named()
}
pub(crate) fn enabled(shell: &ShellState) -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && named()
        && std::env::var(FLAG).as_deref() == Ok("1")
        && mentions::enabled(shell)
        && shell
            .profile_edit
            .sections
            .fixture
            .as_ref()
            .is_some_and(|f| f.completion.is_some())
}
/// Call after the original mentions installation. Retain its account, transport,
/// source, socket generation, drafts and ranking stores; establish the initial
/// synthetic native connected state so nonempty queries exercise remote reads.
pub(crate) fn install(shell: &mut ShellState) {
    if cfg!(any(test, feature = "protected-native-fixture"))
        && named()
        && std::env::var(FLAG).as_deref() == Ok("1")
        && mentions::enabled(shell)
    {
        if let Some(f) = shell.profile_edit.sections.fixture.as_mut() {
            if f.completion.is_some() {
                return;
            }
            f.completion = Some(Model::default());
        }
        // Positive native connection observation, using the same disconnected
        // synthetic socket constructor as the existing picker fixture. This
        // opens no network and preserves the socket generation/source identity.
        let Some(team) = shell.core.active_team.clone() else {
            return;
        };
        let Some(transport) = shell.core.transport.as_ref() else {
            return;
        };
        let Some(workspace) = shell.core.workspaces.get_mut(&team) else {
            return;
        };
        let (sender, _) = tokio::sync::mpsc::channel(1);
        workspace.rt = super_platinum_core::state::RealtimeStatus::Connected(
            super_platinum_core::slack::realtime::Connection::from_sender(sender),
        );
        transport.set_health(super_platinum_core::slack::transport::Health::Online);
        shell.connection.routable = true;
        shell.connection.routed_at = Some(Instant::now());
        shell.signed_in = true;
        shell.loading = false;
    }
}
pub(crate) fn handle(shell: &ShellState) -> Result<Model, Error> {
    if !enabled(shell) {
        return Err(refused());
    }
    shell
        .profile_edit
        .sections
        .fixture
        .as_ref()
        .and_then(|f| f.completion.clone())
        .ok_or_else(refused)
}
fn owner(shell: &ShellState) -> Result<WorkspaceSession, Error> {
    let token = Token::capture(shell).ok_or_else(refused)?;
    shell
        .core
        .session
        .as_ref()
        .and_then(|s| s.workspaces.get(&token.owner.team))
        .cloned()
        .ok_or_else(refused)
}
fn refused() -> Error {
    Error::Offline("Closed Profile completion fixture refused request".into())
}

fn discovery(owner: &WorkspaceSession) -> Value {
    json!({"ok":true,"url":format!("https://edgeapi.slack.com/cache/{}",owner.team_id),"ttl_seconds":300})
}
fn rows(owner: &WorkspaceSession) -> Vec<Value> {
    [(REMOTE, REMOTE_LABEL), (SECOND, SECOND_LABEL)]
        .into_iter()
        .map(|(id, name)| {
            json!({"id":id,"name":name,"team_id":owner.team_id,"is_channel":true,"is_member":false,
            "updated":101,"previous_names":[],"fixture_opaque":{"preserve":id}})
        })
        .collect()
}
fn search_reply(owner: &WorkspaceSession, mode: Mode) -> Result<Reply, Error> {
    Ok((
        match mode {
            Mode::Failure => {
                return Err(Error::Offline(
                    "Synthetic channel search unavailable".into(),
                ));
            }
            Mode::Malformed => json!({"ok":true,"results":false}),
            Mode::Empty => json!({"ok":true,"results":[]}),
            Mode::Success | Mode::Held => json!({"ok":true,"results":rows(owner)}),
        },
        None,
    ))
}
fn same_request(actual: &PreparedRequest, expected: &PreparedRequest) -> bool {
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
struct Accepted {
    method: String,
    args: Value,
    reply: Reply,
    search: bool,
}
/// Rebuild actual factory plans rather than maintaining a looser fixture-only
/// request grammar. Auth/header values are compared privately and never logged.
fn validate(
    client: &SlackClient,
    owner: &WorkspaceSession,
    request: &PreparedRequest,
) -> Result<Accepted, Error> {
    let url = url::Url::parse(&request.url).map_err(|_| refused())?;
    let member_search = url.path().ends_with("/users/search");
    let group_search = url.path().ends_with("/usergroups/search");
    let file_search = url.path().ends_with("/search.autocomplete.files");
    let search =
        member_search || group_search || file_search || url.path().ends_with("/channels/search");
    let method = if member_search {
        "users/search"
    } else if group_search {
        "usergroups/search"
    } else if file_search {
        "search.autocomplete.files"
    } else if search {
        "channels/search"
    } else {
        url.path().strip_prefix("/api/").ok_or_else(refused)?
    };
    let (expected, reply) = match method {
        "users.prefs.get" => (
            name_policy::request::user(client, owner),
            title_users::prefs(),
        ),
        "experiments.getByUser" => (api::list_experiments(client, owner), files::assignments()),
        "client.gantryBoot" => (grid::discovery(client, owner), members::gantry(owner)),
        "client.userBoot" => (api::user_boot(client, owner), members::boot(owner)),
        "search.autocomplete.files" => files::plan(client, owner, request)?,
        "files.info" => files::info(client, owner, request)?,
        "users/search" => members::plan(client, owner, request)?,
        "usergroups/search" => groups::plan(client, owner, request)?,
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
            // This fixture supplies complete empty priority hints and one actual
            // default workspace. A different supplied hint is not silently accepted.
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
fn record(model: &Model, accepted: &Accepted) -> usize {
    let mut ledger = model.0.lock().unwrap();
    let serial = ledger.requests.len() + 1;
    let mut row = json!({"serial":serial,"method":accepted.method,"args":accepted.args});
    if title_users::named() || super::empty_canvas::completion() || super::link_completion::named()
    {
        row["raw"] = accepted.reply.0.clone();
    }
    ledger.requests.push(row);
    serial
}
pub(crate) fn request(shell: &ShellState, request: &PreparedRequest) -> Result<Reply, Error> {
    let model = handle(shell)?;
    let owner = owner(shell)?;
    let accepted = validate(&shell.core.client, &owner, request)?;
    // Held/cancellable reads always enter search(), never a synchronous bypass.
    if accepted.search {
        return Err(refused());
    }
    record(&model, &accepted);
    Ok(accepted.reply)
}
pub(crate) fn ranking_request(
    shell: &ShellState,
    request: &PreparedRequest,
) -> Result<Reply, Error> {
    let path = url::Url::parse(&request.url).map_err(|_| refused())?;
    if !matches!(
        path.path(),
        "/api/search.autocomplete.model" | "/api/search.autocomplete.offlineFeatures"
    ) {
        return Err(refused());
    }
    self::request(shell, request)
}

enum Start {
    Ready(Result<Reply, Error>),
    Held(oneshot::Receiver<Result<Reply, Error>>),
}
fn start(model: &Model, owner: &WorkspaceSession, accepted: Accepted) -> Result<Start, Error> {
    if !accepted.search {
        return Err(refused());
    }
    let serial = record(model, &accepted);
    let mut ledger = model.0.lock().unwrap();
    let mode = if ledger
        .mode_method
        .is_none_or(|method| method == accepted.method)
    {
        ledger.mode_method = None;
        std::mem::take(&mut ledger.mode)
    } else {
        Mode::Success
    };
    if matches!(mode, Mode::Held) {
        let (sender, receiver) = oneshot::channel();
        ledger.held.push(Held {
            serial,
            query: accepted.args["query"].as_str().unwrap().into(),
            sender,
            reply: accepted.reply,
        });
        Ok(Start::Held(receiver))
    } else {
        ledger.completed += 1;
        Ok(Start::Ready(if accepted.method == "users/search" {
            members::reply(owner, mode)
        } else if accepted.method == "search.autocomplete.files" {
            files::reply(owner, mode)
        } else if accepted.method == "usergroups/search" {
            groups::reply(owner, mode)
        } else {
            search_reply(owner, mode)
        }))
    }
}
struct Finish {
    model: Model,
    completed: bool,
}
impl Drop for Finish {
    fn drop(&mut self) {
        let mut ledger = self.model.0.lock().unwrap();
        ledger.completed += 1;
        if !self.completed {
            ledger.cancelled += 1;
        }
    }
}
async fn wait(
    model: Model,
    receiver: oneshot::Receiver<Result<Reply, Error>>,
) -> Result<Reply, Error> {
    let mut finish = Finish {
        model,
        completed: false,
    };
    let result = receiver.await.map_err(|_| refused())?;
    finish.completed = true;
    result
}
pub(crate) async fn search(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Result<Reply, Error> {
    let (model, started) = {
        let shell = state.peek();
        let model = handle(&shell)?;
        let owner = owner(&shell)?;
        let accepted = validate(&shell.core.client, &owner, request)?;
        let started = start(&model, &owner, accepted)?;
        (model, started)
    };
    // The production query's cancellation watch owns this future. The original
    // held sender survives a dropped receiver so late-delivery refusal is visible.
    match started {
        Start::Ready(reply) => reply,
        Start::Held(receiver) => wait(model, receiver).await,
    }
}
pub(crate) async fn file_search(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Result<super_platinum_core::slack::api::custom::texty::files::OrderedResponse, Error> {
    {
        let shell = state.peek();
        if !files::enabled(&shell)
            || url::Url::parse(&request.url).map_err(|_| refused())?.path()
                != "/api/search.autocomplete.files"
        {
            return Err(refused());
        }
    }
    let (bytes, _) = search(state, request).await?;
    let bytes = bytes.as_str().ok_or_else(refused)?;
    super_platinum_core::slack::api::custom::texty::files::OrderedResponse::decode(bytes.as_bytes())
}
pub(crate) fn mode(shell: &ShellState, value: Mode) -> Result<(), Error> {
    let model = handle(shell)?;
    let mut ledger = model.0.lock().unwrap();
    ledger.mode = value;
    // Existing member checks target the member leg of the unified @ query.
    ledger.mode_method = if mentions::user_mode() {
        Some("users/search")
    } else {
        Some("channels/search")
    };
    Ok(())
}
impl Model {
    /// A fixture may retain this handle before account replacement to deliver
    /// the original held result afterwards. It never grants source publication.
    pub(crate) fn release(&self) -> Result<Value, Error> {
        let held = {
            let mut ledger = self.0.lock().unwrap();
            if ledger.held.is_empty() {
                return Err(refused());
            }
            ledger.held.remove(0)
        };
        let delivered = held.sender.send(Ok(held.reply)).is_ok();
        Ok(
            json!({"serial":held.serial,"query":held.query,"delivered":delivered,"cancelledBeforeReply":!delivered}),
        )
    }
    pub(crate) fn snapshot(&self) -> Value {
        let ledger = self.0.lock().unwrap();
        let channel_searches = ledger
            .requests
            .iter()
            .filter(|r| r["method"] == "channels/search")
            .count();
        let member_searches = ledger
            .requests
            .iter()
            .filter(|r| r["method"] == "users/search")
            .count();
        let group_searches = ledger
            .requests
            .iter()
            .filter(|r| r["method"] == "usergroups/search")
            .count();
        let file_searches = ledger
            .requests
            .iter()
            .filter(|r| r["method"] == "search.autocomplete.files")
            .count();
        let searches = channel_searches + member_searches + group_searches + file_searches;
        json!({"requests":ledger.requests,"searches":searches,"channelSearches":channel_searches,"memberSearches":member_searches,"groupSearches":group_searches,"fileSearches":file_searches,"active":searches.saturating_sub(ledger.completed),
            "held":!ledger.held.is_empty(),"heldQueries":ledger.held.iter().map(|h|json!({"serial":h.serial,"query":h.query})).collect::<Vec<_>>(),
            "completed":ledger.completed,"cancelled":ledger.cancelled,"networkCalls":0})
    }
}
pub(crate) fn release(shell: &ShellState) -> Result<Value, Error> {
    handle(shell)?.release()
}
pub(crate) fn snapshot(shell: &ShellState) -> Value {
    shell
        .profile_edit
        .sections
        .fixture
        .as_ref()
        .and_then(|f| f.completion.as_ref())
        .map(Model::snapshot)
        .unwrap_or_else(|| json!({"enabled":false}))
}

#[cfg(test)]
mod tests;
