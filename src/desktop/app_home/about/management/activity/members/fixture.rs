//! Closed picker transport. Raw replies pass through the unchanged decoders,
//! renderer hydration, matching and ranking before any selectable row exists.
use super::*;
use std::sync::Mutex;
use super_platinum_core::slack::{PreparedRequest, client::RequestBody};
use tokio::sync::oneshot;

pub(crate) const FLAG: &str = "SUPER_PLATINUM_WORKFLOW_ACTIVITY_MEMBERS_CHECK";
pub(crate) const ADA: &str = "UACTADA01";
pub(crate) const ZOE: &str = "UACTZOE01";
const BOT: &str = "UACTBOT01";

#[derive(Clone, Default)]
pub(crate) struct Model(Arc<Mutex<Ledger>>);
#[derive(Default)]
struct Ledger {
    requests: Vec<Value>,
    mode: Mode,
    held: Option<(oneshot::Sender<Result<Value, Error>>, Value, String)>,
    completed: usize,
    cancelled: usize,
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
pub(crate) fn enabled(s: &ShellState) -> bool {
    cfg!(feature = "protected-native-fixture")
        && crate::fixture::is_fixture()
        && std::env::var(FLAG).as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("workflow-activity-members" | "workflow-activity-members-narrow")
        )
        && s.app_home
            .about
            .management
            .activity_member_fixture
            .is_some()
}
fn refused() -> Error {
    Error::Offline("Closed Activity member fixture refused request".into())
}
fn model(s: &ShellState) -> Result<Model, Error> {
    if !enabled(s) {
        return Err(refused());
    }
    s.app_home
        .about
        .management
        .activity_member_fixture
        .clone()
        .ok_or_else(refused)
}
fn user(id: &str, name: &str, display: &str) -> Value {
    json!({"id":id,"name":name,"real_name":display,"team_id":"TWORKFLOW",
        "updated":101,"deleted":false,"is_bot":false,"is_stranger":false,
        "profile":{"real_name":display,"real_name_normalized":display,
            "display_name":display,"display_name_normalized":display,"email":format!("{name}@fixture.test")},
        "fixture_opaque":{"keep":true}})
}
fn users() -> Vec<Value> {
    let mut bot = user(BOT, "reviewbot", "Review requests");
    bot["is_bot"] = json!(true);
    vec![
        user("UWORKFLOW", "reviewer", "Review owner"),
        user(ADA, "ada", "Ada Lovelace"),
        user(ZOE, "zoe", "Zo\u{eb} Alvarez"),
        bot,
    ]
}
fn boot() -> Value {
    json!({"ok":true,"self":users()[0],"team":{"id":"TWORKFLOW","name":"Activity fixture"},
        "users":users(),"workspaces":[{"id":"TWORKFLOW","prefs":{"display_real_names":true}}],
        "prefs":{"display_real_names":true},"channels":[],"ims":[],"groups":[],"mpims":[]})
}
pub(crate) fn install(s: &mut ShellState) {
    s.core.active_account = Some("00000000-0000-4000-8000-000000000093".into());
    super::super::fixture::install(s);
    s.app_home.about.management.activity_member_fixture = Some(Model::default());
    s.signed_in = true;
    s.loading = false;
    let owner = s.core.session.as_ref().unwrap().workspaces["TWORKFLOW"].clone();
    let account = s.core.active_account.clone().unwrap();
    let w = s.core.workspaces.get_mut("TWORKFLOW").unwrap();
    // The older About fixture's U_HOME_BOT is not a valid directory member
    // ID. Start this named fixture's same DM with an explicit raw valid bot;
    // the actual includeBots=false eligibility predicate excludes it.
    w.channels
        .get_mut(s.core.active_channel.as_ref().unwrap())
        .unwrap()
        .user = Some(BOT.into());
    // Initial fixture ingress only: no typed row is promoted into source order.
    // Queue the same explicit raw boot rows; production drain must hydrate them.
    w.users = users()
        .into_iter()
        .map(|raw| {
            let user: super_platinum_core::slack::models::User =
                serde_json::from_value(raw).unwrap();
            (user.id.clone(), user)
        })
        .collect();
    w.directory_members = Default::default();
    w.directory_members
        .observe_boot(
            &account,
            &owner,
            &w.users.keys().cloned().collect(),
            &boot(),
        )
        .unwrap();
    set_connection(s, true);
    // Bind the initial About surface to this fixture's explicit bot identity.
    // This happens before any interaction, read or result is admitted.
    let owner = crate::app_home::about::Owner::capture(s).unwrap();
    s.app_home.about.surface.as_mut().unwrap().owner = owner;
}
fn set_connection(s: &mut ShellState, remote: bool) {
    use super_platinum_core::{
        slack::{realtime::Connection, transport::Health},
        state::RealtimeStatus,
    };
    s.connection.routable = remote;
    s.connection.routed_at = Some(Instant::now());
    s.core.transport.as_ref().unwrap().set_health(if remote {
        Health::Online
    } else {
        Health::Offline
    });
    let w = s.core.workspaces.get_mut("TWORKFLOW").unwrap();
    let (sender, _) = tokio::sync::mpsc::channel(1);
    w.rt = if remote {
        RealtimeStatus::Connected(Connection::from_sender(sender))
    } else {
        RealtimeStatus::Disconnected
    };
}
pub(crate) fn connection(s: &mut ShellState, remote: bool) -> Result<(), String> {
    model(s).map_err(|_| "Picker fixture unavailable")?;
    set_connection(s, remote);
    Ok(())
}
pub(crate) fn mode(s: &ShellState, mode: Mode) -> Result<(), String> {
    model(s)
        .map_err(|_| "Picker fixture unavailable")?
        .0
        .lock()
        .unwrap()
        .mode = mode;
    Ok(())
}

fn fields(request: &PreparedRequest, edge: bool) -> Result<(String, Value), Error> {
    if request.method != "POST" {
        return Err(refused());
    }
    let url = url::Url::parse(&request.url).map_err(|_| refused())?;
    if url.scheme() != "https"
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(refused());
    }
    let (method, args) = if edge {
        if url.host_str() != Some("edgeapi.slack.com")
            || url.path() != "/cache/TWORKFLOW/users/search"
        {
            return Err(refused());
        }
        let RequestBody::Json(args) = &request.body else {
            return Err(refused());
        };
        if args["count"] != 20
            || args["fuzz"] != 1
            || args["enable_workspace_ranking"] != true
            || args["filter"] != "NOT deactivated"
            || args["include_profile_only_users"] != true
            || !args["query"].is_string()
            || args.get("marker").is_some()
        {
            return Err(refused());
        }
        let mut args = args.clone();
        args.as_object_mut().ok_or_else(refused)?.remove("token");
        ("users/search".into(), args)
    } else {
        if url.host_str() != Some("example.test") {
            return Err(refused());
        }
        let method = url.path().strip_prefix("/api/").ok_or_else(refused)?;
        let RequestBody::Form(fields) = &request.body else {
            return Err(refused());
        };
        let mut args = serde_json::Map::new();
        for (key, value) in fields {
            if key != "token" && args.insert(key.clone(), json!(value)).is_some() {
                return Err(refused());
            }
        }
        (method.into(), Value::Object(args))
    };
    Ok((method, args))
}
fn raw_response(method: &str, args: &Value) -> Result<Value, Error> {
    Ok(match method {
        "client.userBoot" => boot(),
        "client.gantryBoot"
            if args["include_channels"] == "false" && args["_x_reason"] == "fetch-client-data" =>
        {
            boot()
        }
        "users.prefs.get" => json!({"ok":true,"prefs":{"display_real_names_override":1}}),
        "team.prefs.get" if args.get("target_team").is_none_or(|v| v == "TWORKFLOW") => {
            json!({"ok":true,"prefs":{"display_real_names":true}})
        }
        "experiments.getByUser" => json!({"ok":true,"assignments":{}}),
        "drafts.list" => json!({"ok":true,"drafts":[],"has_more":false}),
        "drafts.listActive" => json!({"ok":true,"active_draft_ids":[],"has_more":false}),
        "api.getFlannelHttpUrl"
            if args["include_external_workspaces"] == "true"
                && args["_x_reason"] == "client-needs-valid-flannel-url" =>
        {
            json!({"ok":true,"url":"https://edgeapi.slack.com/cache/TWORKFLOW","ttl_seconds":300})
        }
        "search.autocomplete.model" if args["_x_reason"] == "AutocompleteModelCache" => {
            json!({"ok":true,"model":null})
        }
        "search.autocomplete.offlineFeatures" if args["_x_reason"] == "search-offline-features" => {
            json!({"ok":true})
        }
        _ => return Err(refused()),
    })
}
pub(super) fn request(s: &ShellState, request: &PreparedRequest) -> Result<Value, Error> {
    let model = model(s)?;
    let (method, args) = fields(request, false)?;
    model
        .0
        .lock()
        .unwrap()
        .requests
        .push(json!({"method":method,"args":args}));
    raw_response(&method, &args)
}
pub(crate) fn ranking_request(s: &ShellState, request: &PreparedRequest) -> Result<Value, Error> {
    let (method, _) = fields(request, false)?;
    if !matches!(
        method.as_str(),
        "search.autocomplete.model" | "search.autocomplete.offlineFeatures"
    ) {
        return Err(refused());
    }
    self::request(s, request)
}
fn search_response(mode: Mode) -> Result<Value, Error> {
    match mode {
        Mode::Failure => Err(Error::Offline("Synthetic member read unavailable".into())),
        Mode::Malformed => Ok(json!({"ok":true,"results":false})),
        Mode::Empty => Ok(json!({"ok":true,"results":[]})),
        _ => Ok(json!({"ok":true,"results":users().into_iter()
            .filter(|row| row["id"] == ADA || row["id"] == ZOE).collect::<Vec<_>>() })),
    }
}
pub(super) async fn search(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Result<(Value, Option<Vec<String>>), Error> {
    let model = model(&state.peek())?;
    let (method, args) = fields(request, true)?;
    let receiver = {
        let mut ledger = model.0.lock().unwrap();
        if ledger.held.is_some() {
            return Err(refused());
        }
        ledger.requests.push(json!({"method":method,"args":args}));
        let mode = std::mem::take(&mut ledger.mode);
        if matches!(mode, Mode::Held) {
            let (send, recv) = oneshot::channel();
            ledger.held = Some((
                send,
                search_response(Mode::Success)?,
                args["query"].as_str().unwrap().into(),
            ));
            recv
        } else {
            ledger.completed += 1;
            return search_response(mode).map(|raw| (raw, None));
        }
    };
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
    let mut finish = Finish {
        model: model.clone(),
        completed: false,
    };
    let raw = receiver.await.map_err(|_| refused())??;
    finish.completed = true;
    Ok((raw, None))
}
pub(crate) fn release(s: &ShellState) -> Result<Value, String> {
    let model = model(s).map_err(|_| "Picker fixture unavailable")?;
    let (sender, raw, query) = model
        .0
        .lock()
        .unwrap()
        .held
        .take()
        .ok_or("No held member request")?;
    let delivered = sender.send(Ok(raw)).is_ok();
    Ok(json!({"query":query,"delivered":delivered,"cancelledBeforeReply":!delivered}))
}
pub(crate) fn snapshot(s: &ShellState) -> Value {
    let ledger = s.app_home.about.management.activity_member_fixture.as_ref().map(|model| {
        let m=model.0.lock().unwrap();
        json!({"requests":m.requests,"searches":m.requests.iter().filter(|r|r["method"]=="users/search").count(),
            "held":m.held.is_some(),"heldQuery":m.held.as_ref().map(|v|v.2.as_str()),"completed":m.completed,"cancelled":m.cancelled,"networkCalls":0})
    });
    let picker = s.app_home.about.management.surface.as_ref().and_then(|h| {
        document(s, h.id).map(|d| {
            let (generation, rows, pending, notice) = lookup::view(s, h.id);
            json!({"generation":generation,"rows":rows,"pending":pending,"notice":notice,
            "query":d.activity.user_search,"selection":d.activity.member_selection,
            "filterUser":d.activity.filters.user,"filterRevision":d.activity.filters.revision,
            "failure":d.activity.member_receipt.as_ref().and_then(|q|q.failure_receipt.lock().unwrap().as_ref().map(|e|format!("{e:?}")))})
        })
    });
    json!({"ledger":ledger,"picker":picker,"metadata":super::super::super::fixture::snapshot(s),
        "source":s.core.workspaces.get("TWORKFLOW").and_then(|w|w.directory_members.cached()),
        "nativeIds":s.core.workspaces.get("TWORKFLOW").map(|w| {let mut ids=w.users.keys().collect::<Vec<_>>();ids.sort();ids}),
        "account":s.core.active_account,"epoch":s.core.account_epoch})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn initial_raw_fixture_preserves_about_owner_and_queues_complete_source() {
        use super_platinum_core::block_actions::workflow_management::activity_members::{
            Eligibility, Member,
        };
        let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
        install(&mut s);
        assert!(crate::app_home::eligible(&s));
        let about = s.app_home.about.surface.as_ref().unwrap();
        assert!(about.owner.current(&s));
        let ctx = &about.owner.ctx;
        let account = s.core.active_account.as_deref().unwrap();
        let w = &s.core.workspaces[&ctx.session.team_id];
        assert_eq!(w.users.len(), users().len());
        assert!(
            w.directory_members
                .has_pending(account, &ctx.session)
                .unwrap()
        );
        assert!(
            w.directory_members
                .ready(account, &ctx.session, ctx.epoch, "not-rendered", &w.users)
                .is_err()
        );
        for raw in users() {
            let member = Member::decode(&raw).unwrap();
            let expected = if raw["id"] == ADA || raw["id"] == ZOE {
                Eligibility::Allowed
            } else {
                Eligibility::Denied
            };
            assert_eq!(
                member.eligibility(&ctx.session.user_id, Some(true), None, None),
                expected
            );
        }
        assert_eq!(
            w.channels[s.core.active_channel.as_ref().unwrap()]
                .user
                .as_deref(),
            Some(BOT)
        );
    }
    #[test]
    fn synthetic_search_shapes_use_actual_decoder_and_preserve_raw_fields() {
        use super_platinum_core::block_actions::workflow_management::activity_members::Page;
        let page = Page::decode(search_response(Mode::Success).unwrap()).unwrap();
        assert_eq!(page.rows().len(), 2);
        assert_eq!(page.rows()[0]["id"], ADA);
        assert_eq!(
            page.rows()[1]["profile"]["display_name"],
            "Zo\u{eb} Alvarez"
        );
        assert_eq!(page.rows()[0]["fixture_opaque"]["keep"], true);
        assert!(
            Page::decode(search_response(Mode::Empty).unwrap())
                .unwrap()
                .rows()
                .is_empty()
        );
        assert!(Page::decode(search_response(Mode::Malformed).unwrap()).is_err());
        assert!(matches!(
            search_response(Mode::Failure),
            Err(Error::Offline(_))
        ));
    }
    #[test]
    fn fixture_allowlist_rejects_foreign_hosts_and_non_read_endpoints() {
        let (s, _) = super::super::tests::setup();
        let owner = &s
            .app_home
            .about
            .management
            .surface
            .as_ref()
            .unwrap()
            .owner
            .ctx;
        let mut request =
            super_platinum_core::slack::api::list_experiments(&owner.client, &owner.session);
        let (method, args) = fields(&request, false).unwrap();
        assert!(raw_response(&method, &args).is_ok());
        request.url = request.url.replace("example.test", "foreign.test");
        assert!(fields(&request, false).is_err());
        assert!(raw_response("chat.postMessage", &json!({})).is_err());
    }
}
